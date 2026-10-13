//! SQLite log of every graded decision (drills and imported hands) and the leak statistics
//! computed from it.

use crate::drill::{DrillView, Feedback};
use anyhow::Result;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub struct Db {
    conn: Connection,
}

#[derive(Debug, Serialize)]
pub struct Summary {
    pub attempts: i64,
    pub avg_loss_bb: f64,
    pub avg_loss_pct: f64,
    pub good_pct: f64,
    pub by_category: Vec<CategoryStat>,
    pub by_day: Vec<DayStat>,
}

#[derive(Debug, Serialize)]
pub struct CategoryStat {
    pub category: String,
    pub n: i64,
    pub avg_loss_bb: f64,
}

#[derive(Debug, Serialize)]
pub struct DayStat {
    pub day: String,
    pub n: i64,
    pub avg_loss_bb: f64,
    pub good_pct: f64,
}

/// Extra context for a decision that isn't in the drill view.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Context {
    pub source: String,
    pub opponent: Option<String>,
    /// stored-game action history (JSON) so the spot can be drilled again
    pub history: Option<String>,
    pub hero_cards: Option<String>,
}

const COLUMNS: &[(&str, &str)] = &[
    ("ts", "INTEGER"),
    ("source", "TEXT"),
    ("spot_id", "TEXT"),
    ("stack", "INTEGER"),
    ("matchup", "TEXT"),
    ("pot_type", "TEXT"),
    ("street", "TEXT"),
    ("texture", "TEXT"),
    ("hero", "TEXT"),
    ("in_position", "INTEGER"),
    ("aggressor", "INTEGER"),
    ("node_type", "TEXT"),
    ("situation", "TEXT"),
    ("hand", "TEXT"),
    ("hand_bucket", "TEXT"),
    ("board", "TEXT"),
    ("chosen", "TEXT"),
    ("chosen_kind", "TEXT"),
    ("best", "TEXT"),
    ("best_kind", "TEXT"),
    ("pot_bb", "REAL"),
    ("ev_loss_bb", "REAL"),
    ("ev_loss_pct", "REAL"),
    ("freq_chosen", "REAL"),
    ("freq_diff", "REAL"),
    ("rare", "INTEGER"),
    ("verdict", "TEXT"),
    ("p_fold", "REAL"),
    ("p_check", "REAL"),
    ("p_call", "REAL"),
    ("p_aggr", "REAL"),
    ("rng", "INTEGER"),
    ("mode", "TEXT"),
    ("opponent", "TEXT"),
    ("history", "TEXT"),
    ("hero_cards", "TEXT"),
];

/// One leak statistic: how often the player takes an action in a situation, against how
/// often the solver takes it with the same hands.
#[derive(Debug, Serialize, Clone)]
pub struct LeakStat {
    pub key: String,
    pub name: String,
    pub n: i64,
    /// observed frequency
    pub you: f64,
    /// solver frequency with the same hands
    pub solver: f64,
    /// shrunk estimate of the player's true frequency (Beta prior at the solver frequency,
    /// 20 pseudo-decisions)
    pub estimate: f64,
    pub ci_low: f64,
    pub ci_high: f64,
    /// probability the true gap to the solver is more than 5 points in the observed direction
    pub confidence: f64,
    pub flagged: bool,
    /// average EV loss in these decisions, bb per 100 decisions
    pub bb_per_100: f64,
    pub verdict: String,
}

#[derive(Debug, Serialize)]
pub struct LeakReport {
    pub decisions: i64,
    pub stats: Vec<LeakStat>,
    pub top: Vec<LeakStat>,
    pub opponents: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct AttemptRow {
    pub id: i64,
    pub ts: i64,
    pub source: String,
    pub spot_id: String,
    pub street: String,
    pub hand: String,
    pub board: String,
    pub chosen: String,
    pub best: String,
    pub ev_loss_bb: f64,
    pub ev_loss_pct: f64,
    pub verdict: String,
    pub can_redrill: bool,
}

/// Stat definitions: (key, name, SQL condition on the situation, action predicate, solver column).
const STATS: &[(&str, &str, &str, &str, &str)] = &[
    ("aggr", "Aggression postflop (bet/raise when possible)", "street <> 'preflop'", "chosen_kind IN ('bet','raise','allin')", "p_aggr"),
    ("cbet", "C-bet flop as the preflop raiser", "situation='cbet'", "chosen_kind IN ('bet','raise','allin')", "p_aggr"),
    ("barrel", "Bet turn again after c-betting", "situation='barrel'", "chosen_kind IN ('bet','raise','allin')", "p_aggr"),
    ("river_barrel", "Bet river after betting the turn", "situation='river_barrel'", "chosen_kind IN ('bet','raise','allin')", "p_aggr"),
    ("probe", "Probe the turn after flop checks through", "situation='probe'", "chosen_kind IN ('bet','raise','allin')", "p_aggr"),
    ("fold_cbet", "Fold to a flop c-bet", "situation='vs_cbet'", "chosen_kind='fold'", "p_fold"),
    ("xr_cbet", "Check-raise a flop c-bet (out of position)", "situation='vs_cbet' AND in_position=0", "chosen_kind IN ('raise','allin')", "p_aggr"),
    ("fold_river", "Fold to a river bet", "situation='river_vs_bet'", "chosen_kind='fold'", "p_fold"),
    ("fold_raise", "Fold to a raise", "situation='facing_raise'", "chosen_kind='fold'", "p_fold"),
    ("call_bets", "Call when facing a bet", "situation IN ('vs_cbet','vs_bet','river_vs_bet')", "chosen_kind='call'", "p_call"),
    ("pf_limp", "Limp first in (preflop)", "situation='pf_first_in'", "chosen_kind='limp'", "0.0"),
    ("pf_open", "Raise first in (preflop)", "situation='pf_first_in'", "chosen_kind IN ('raise','allin')", "p_aggr"),
    ("pf_3bet", "3-bet vs an open (preflop)", "situation='pf_vs_open'", "chosen_kind IN ('raise','allin')", "p_aggr"),
    ("pf_fold_open", "Fold vs an open (preflop)", "situation='pf_vs_open'", "chosen_kind='fold'", "p_fold"),
    ("pf_fold_3bet", "Fold to a 3-bet (preflop)", "situation='pf_vs_3bet'", "chosen_kind='fold'", "p_fold"),
];

const PRIOR: f64 = 20.0;
const GAP: f64 = 0.05;
const MIN_N: i64 = 30;

fn norm_cdf(z: f64) -> f64 {
    // Abramowitz-Stegun approximation of the standard normal CDF
    let t = 1.0 / (1.0 + 0.2316419 * z.abs());
    let d = 0.3989423 * (-z * z / 2.0).exp();
    let p = d * t * (0.3193815 + t * (-0.3565638 + t * (1.781478 + t * (-1.821256 + t * 1.330274))));
    if z > 0.0 { 1.0 - p } else { p }
}

/// Shrinkage estimate and 90% interval for a frequency, with a Beta prior centered on the
/// solver frequency. Returns (estimate, low, high, confidence of a 5+ point gap).
pub fn shrink(x: f64, n: f64, solver: f64) -> (f64, f64, f64, f64) {
    let s = solver.clamp(0.01, 0.99);
    let a = PRIOR * s + x;
    let b = PRIOR * (1.0 - s) + (n - x);
    let mean = a / (a + b);
    let var = a * b / ((a + b).powi(2) * (a + b + 1.0));
    let sd = var.sqrt();
    let (lo, hi) = ((mean - 1.645 * sd).max(0.0), (mean + 1.645 * sd).min(1.0));
    let conf = if mean >= solver { 1.0 - norm_cdf((solver + GAP - mean) / sd.max(1e-9)) } else { norm_cdf((solver - GAP - mean) / sd.max(1e-9)) };
    (mean, lo, hi, conf)
}

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch("CREATE TABLE IF NOT EXISTS attempt (id INTEGER PRIMARY KEY);")?;
        // add any missing columns (older databases)
        let existing: Vec<String> = conn
            .prepare("PRAGMA table_info(attempt)")?
            .query_map([], |r| r.get::<_, String>(1))?
            .collect::<std::result::Result<_, _>>()?;
        for (c, t) in COLUMNS {
            if !existing.iter().any(|e| e == c) {
                conn.execute_batch(&format!("ALTER TABLE attempt ADD COLUMN {c} {t};"))?;
            }
        }
        conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS attempt_ts ON attempt(ts);
             CREATE INDEX IF NOT EXISTS attempt_spot ON attempt(spot_id);
             CREATE TABLE IF NOT EXISTS import_hand (
                hand_id TEXT PRIMARY KEY, file TEXT, hand_no INTEGER, played TEXT, hero TEXT, hero_cards TEXT, board TEXT,
                players INTEGER, hero_pos TEXT, villain TEXT, pot_type TEXT, stack_bb REAL, status TEXT, note TEXT,
                decisions INTEGER DEFAULT 0, ev_loss REAL DEFAULT 0, net_bb REAL, imported_at INTEGER, job TEXT);
             CREATE TABLE IF NOT EXISTS alias (name TEXT PRIMARY KEY, display TEXT NOT NULL);",
        )?;
        Ok(Db { conn })
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn record(&self, v: &DrillView, f: &Feedback, ctx: &Context) -> Result<i64> {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs() as i64;
        let chosen = &v.actions[f.chosen];
        let best = &v.actions[f.score.target];
        let p = |kinds: &[&str]| -> f64 { f.actions.iter().filter(|a| kinds.contains(&a.kind.as_str())).map(|a| a.freq_hand).sum() };
        self.conn.execute(
            "INSERT INTO attempt (ts, source, spot_id, stack, matchup, pot_type, street, texture, hero, in_position, aggressor, node_type, situation,
                hand, hand_bucket, board, chosen, chosen_kind, best, best_kind, pot_bb, ev_loss_bb, ev_loss_pct, freq_chosen, freq_diff, rare, verdict,
                p_fold, p_check, p_call, p_aggr, rng, mode, opponent, history, hero_cards)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27,
                ?28, ?29, ?30, ?31, ?32, ?33, ?34, ?35, ?36)",
            params![
                now, ctx.source, v.spot_id, v.stack, v.matchup, v.pot_type, v.street, v.texture, v.hero, v.hero_in_position as i64,
                v.hero_is_aggressor as i64, v.node_type, v.situation, v.hand.join(""), format!("{:?}", f.hand_desc.bucket), v.board.join(""),
                chosen.label, chosen.kind, best.label, best.kind, v.pot_bb, f.score.ev_loss_bb, f.score.ev_loss_pct, f.actions[f.chosen].freq_hand,
                f.score.freq_diff, f.score.rare as i64, f.verdict, p(&["fold"]), p(&["check"]), p(&["call"]), p(&["bet", "raise", "allin"]),
                f.score.rng, format!("{:?}", v.mode).to_lowercase(), ctx.opponent, ctx.history, ctx.hero_cards
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    fn filter_sql(source: Option<&str>, opponent: Option<&str>) -> (String, Vec<String>) {
        let mut w = vec!["1=1".to_string()];
        let mut args = Vec::new();
        if let Some(s) = source {
            w.push(format!("source = ?{}", args.len() + 1));
            args.push(s.to_string());
        }
        if let Some(o) = opponent {
            w.push(format!("opponent = ?{}", args.len() + 1));
            args.push(o.to_string());
        }
        (w.join(" AND "), args)
    }

    pub fn summary(&self) -> Result<Summary> {
        let (attempts, avg, pct, good): (i64, f64, f64, f64) = self.conn.query_row(
            "SELECT COUNT(*), COALESCE(AVG(ev_loss_bb), 0), COALESCE(AVG(ev_loss_pct), 0), COALESCE(AVG(verdict IN ('Best','Correct')), 0) FROM attempt",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )?;
        let mut st = self.conn.prepare(
            "SELECT street || ' · ' || matchup || ' ' || pot_type || ' · ' || node_type AS c, COUNT(*), AVG(ev_loss_bb)
             FROM attempt GROUP BY c ORDER BY AVG(ev_loss_bb) DESC",
        )?;
        let by_category = st
            .query_map([], |r| Ok(CategoryStat { category: r.get(0)?, n: r.get(1)?, avg_loss_bb: r.get(2)? }))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut st = self.conn.prepare(
            "SELECT date(ts, 'unixepoch', 'localtime') AS d, COUNT(*), AVG(ev_loss_bb), AVG(verdict IN ('Best','Correct'))
             FROM attempt GROUP BY d ORDER BY d",
        )?;
        let by_day = st
            .query_map([], |r| Ok(DayStat { day: r.get(0)?, n: r.get(1)?, avg_loss_bb: r.get(2)?, good_pct: r.get::<_, f64>(3)? * 100.0 }))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(Summary { attempts, avg_loss_bb: avg, avg_loss_pct: pct, good_pct: good * 100.0, by_category, by_day })
    }

    /// Leak statistics against the solver, optionally only drills/imports or one opponent.
    pub fn leaks(&self, source: Option<&str>, opponent: Option<&str>) -> Result<LeakReport> {
        let (base, args) = Self::filter_sql(source, opponent);
        let decisions: i64 = self.conn.query_row(&format!("SELECT COUNT(*) FROM attempt WHERE {base}"), rusqlite::params_from_iter(args.iter()), |r| r.get(0))?;
        let mut stats = Vec::new();
        for (key, name, cond, pred, col) in STATS {
            let sql = format!(
                "SELECT COUNT(*), COALESCE(SUM({pred}), 0), COALESCE(AVG({col}), 0), COALESCE(AVG(ev_loss_bb), 0) FROM attempt WHERE {base} AND {cond} AND {col} IS NOT NULL"
            );
            let (n, x, solver, loss): (i64, f64, f64, f64) =
                self.conn.query_row(&sql, rusqlite::params_from_iter(args.iter()), |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
            if n == 0 {
                continue;
            }
            let you = x / n as f64;
            let (estimate, lo, hi, conf) = shrink(x, n as f64, solver);
            let outside = lo > solver + GAP || hi < solver - GAP;
            let flagged = n >= MIN_N && outside;
            let dir = if estimate > solver { "too often" } else { "too rarely" };
            let verdict = if n < MIN_N {
                format!("not enough data yet ({n} of {MIN_N})")
            } else if flagged {
                format!("leak: {dir} (about {:.0} points)", (estimate - solver).abs() * 100.0)
            } else if (estimate - solver).abs() > GAP {
                format!("possible leak, not confident yet: {dir}")
            } else {
                "close to the solver".into()
            };
            stats.push(LeakStat {
                key: key.to_string(),
                name: name.to_string(),
                n,
                you,
                solver,
                estimate,
                ci_low: lo,
                ci_high: hi,
                confidence: conf,
                flagged,
                bb_per_100: loss * 100.0,
                verdict,
            });
        }
        let mut top: Vec<LeakStat> = stats.iter().filter(|s| s.flagged && s.key != "aggr").cloned().collect();
        top.sort_by(|a, b| b.bb_per_100.partial_cmp(&a.bb_per_100).unwrap());
        top.truncate(3);
        let opponents = self
            .conn
            .prepare("SELECT DISTINCT opponent FROM attempt WHERE opponent IS NOT NULL ORDER BY opponent")?
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(LeakReport { decisions, stats, top, opponents })
    }

    /// The decisions behind one stat (or all), worst first.
    pub fn attempts(&self, stat: Option<&str>, source: Option<&str>, opponent: Option<&str>, limit: i64) -> Result<Vec<AttemptRow>> {
        let (base, args) = Self::filter_sql(source, opponent);
        let cond = stat.and_then(|k| STATS.iter().find(|s| s.0 == k)).map(|s| s.2).unwrap_or("1=1");
        let sql = format!(
            "SELECT id, ts, COALESCE(source,''), spot_id, street, hand, board, chosen, best, ev_loss_bb, COALESCE(ev_loss_pct, 0), verdict, history IS NOT NULL
             FROM attempt WHERE {base} AND {cond} ORDER BY ev_loss_bb DESC LIMIT {limit}"
        );
        let mut st = self.conn.prepare(&sql)?;
        let rows = st
            .query_map(rusqlite::params_from_iter(args.iter()), |r| {
                Ok(AttemptRow {
                    id: r.get(0)?,
                    ts: r.get(1)?,
                    source: r.get(2)?,
                    spot_id: r.get(3)?,
                    street: r.get(4)?,
                    hand: r.get(5)?,
                    board: r.get(6)?,
                    chosen: r.get(7)?,
                    best: r.get(8)?,
                    ev_loss_bb: r.get(9)?,
                    ev_loss_pct: r.get(10)?,
                    verdict: r.get(11)?,
                    can_redrill: r.get::<_, i64>(12)? == 1,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// (spot_id, history JSON, hero cards) for re-drilling one logged decision.
    pub fn redrill_info(&self, id: i64) -> Result<Option<(String, String, String)>> {
        let mut st = self.conn.prepare("SELECT spot_id, history, hero_cards FROM attempt WHERE id = ?1 AND history IS NOT NULL")?;
        let mut rows = st.query(params![id])?;
        Ok(match rows.next()? {
            Some(r) => Some((r.get(0)?, r.get(1)?, r.get(2)?)),
            None => None,
        })
    }

    /// Filters for "my leaks" mode: the costliest category with at least `min_n` attempts.
    pub fn worst_category(&self, min_n: i64) -> Result<Option<(String, String, String, String)>> {
        let mut st = self.conn.prepare(
            "SELECT street, matchup, pot_type, node_type FROM attempt WHERE source='drill' GROUP BY street, matchup, pot_type, node_type
             HAVING COUNT(*) >= ?1 ORDER BY AVG(ev_loss_bb) DESC LIMIT 1",
        )?;
        let mut rows = st.query(params![min_n])?;
        Ok(match rows.next()? {
            Some(r) => Some((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            None => None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shrinkage_needs_samples() {
        // 6 hands, all folds, vs a 40% solver fold rate: the estimate moves only part of the way
        let (est, lo, hi, conf) = shrink(6.0, 6.0, 0.4);
        assert!(est > 0.4 && est < 0.7, "{est}");
        assert!(lo < 0.45 && hi > 0.6);
        assert!(conf < 0.95, "6 hands should not be confident ({conf})");
        // 120 hands at 70% vs 40%: a confident leak
        let (est, lo, _hi, conf) = shrink(84.0, 120.0, 0.4);
        assert!(est > 0.6);
        assert!(lo > 0.45);
        assert!(conf > 0.99);
    }

    #[test]
    fn matches_solver_is_not_a_leak() {
        let (est, lo, hi, conf) = shrink(40.0, 100.0, 0.4);
        assert!((est - 0.4).abs() < 0.01);
        assert!(lo < 0.4 && hi > 0.4);
        assert!(conf < 0.5);
    }

    #[test]
    fn normal_cdf_sane() {
        assert!((norm_cdf(0.0) - 0.5).abs() < 1e-6);
        assert!((norm_cdf(1.645) - 0.95).abs() < 1e-3);
        assert!((norm_cdf(-1.645) - 0.05).abs() < 1e-3);
    }
}
