//! SQLite log of every graded decision (drills now, imported hands later).

use crate::drill::{DrillView, Feedback};
use anyhow::Result;
use rusqlite::{params, Connection};
use serde::Serialize;
use std::path::Path;

pub struct Db {
    conn: Connection,
}

#[derive(Debug, Serialize)]
pub struct Summary {
    pub attempts: i64,
    pub avg_loss_bb: f64,
    pub best_or_fine_pct: f64,
    pub by_category: Vec<CategoryStat>,
}

#[derive(Debug, Serialize)]
pub struct CategoryStat {
    pub category: String,
    pub n: i64,
    pub avg_loss_bb: f64,
}

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS attempt (
                id INTEGER PRIMARY KEY,
                ts INTEGER NOT NULL,
                source TEXT NOT NULL,
                spot_id TEXT NOT NULL,
                stack INTEGER, matchup TEXT, pot_type TEXT, street TEXT, texture TEXT,
                hero TEXT, in_position INTEGER, node_type TEXT,
                hand TEXT, hand_bucket TEXT, board TEXT,
                chosen TEXT, chosen_kind TEXT, best TEXT, best_kind TEXT,
                ev_loss_bb REAL, freq_chosen REAL, rare INTEGER, verdict TEXT,
                range_aggr_freq REAL, opponent TEXT, session_id INTEGER
             );
             CREATE INDEX IF NOT EXISTS attempt_ts ON attempt(ts);",
        )?;
        Ok(Db { conn })
    }

    pub fn record(&self, v: &DrillView, f: &Feedback, source: &str, opponent: Option<&str>) -> Result<i64> {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs() as i64;
        let chosen = &v.actions[f.chosen];
        let best = &v.actions[f.score.best];
        let aggr: f64 = f.actions.iter().zip(&v.actions).filter(|(_, a)| matches!(a.kind.as_str(), "bet" | "raise" | "allin")).map(|(r, _)| r.freq_range).sum();
        self.conn.execute(
            "INSERT INTO attempt (ts, source, spot_id, stack, matchup, pot_type, street, texture, hero, in_position, node_type,
                hand, hand_bucket, board, chosen, chosen_kind, best, best_kind, ev_loss_bb, freq_chosen, rare, verdict, range_aggr_freq, opponent)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24)",
            params![
                now, source, v.spot_id, v.stack, v.matchup, v.pot_type, v.street, v.texture, v.hero, v.hero_in_position as i64, v.node_type,
                v.hand.join(""), format!("{:?}", f.hand_desc.bucket), v.board.join(""), chosen.label, chosen.kind, best.label, best.kind,
                f.score.ev_loss_bb, f.actions[f.chosen].freq_hand, f.score.rare as i64, f.verdict, aggr, opponent
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn summary(&self) -> Result<Summary> {
        let (attempts, avg, good): (i64, f64, f64) = self.conn.query_row(
            "SELECT COUNT(*), COALESCE(AVG(ev_loss_bb), 0), COALESCE(AVG(verdict IN ('Best','Fine')), 0) FROM attempt",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        let mut st = self.conn.prepare(
            "SELECT street || ' · ' || matchup || ' ' || pot_type || ' · ' || node_type AS c, COUNT(*), AVG(ev_loss_bb)
             FROM attempt GROUP BY c ORDER BY AVG(ev_loss_bb) DESC",
        )?;
        let by_category = st
            .query_map([], |r| Ok(CategoryStat { category: r.get(0)?, n: r.get(1)?, avg_loss_bb: r.get(2)? }))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(Summary { attempts, avg_loss_bb: avg, best_or_fine_pct: good * 100.0, by_category })
    }

    /// Filters for "my leaks" mode: the costliest category with at least `min_n` attempts.
    pub fn worst_category(&self, min_n: i64) -> Result<Option<(String, String, String, String)>> {
        let mut st = self.conn.prepare(
            "SELECT street, matchup, pot_type, node_type FROM attempt GROUP BY street, matchup, pot_type, node_type
             HAVING COUNT(*) >= ?1 ORDER BY AVG(ev_loss_bb) DESC LIMIT 1",
        )?;
        let mut rows = st.query(params![min_n])?;
        Ok(match rows.next()? {
            Some(r) => Some((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            None => None,
        })
    }
}
