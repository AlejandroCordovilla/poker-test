//! PokerNow log import (Milestone 5).
//!
//! - Parses the CSV log PokerNow exports ("Download Full Log").
//! - Preflop: every hero decision in a spot the preflop model covers (first in, facing an open,
//!   facing a 3-bet, facing a jam) is graded against the preflop solver, with EVs. Limps are
//!   outside the model and graded against folding.
//! - Postflop: heads-up pots that match a solved matchup (BTN vs BB, SB vs BB, BTN vs SB, single
//!   raised or 3-bet) are re-solved on the exact flop with the real pot and stacks, then the
//!   real action is replayed through the tree (bet sizes mapped to the nearest tree size) and
//!   every hero decision is graded. This runs in the background, one hand at a time.

use crate::db::{Context, Db};
use crate::drill::{grade_real, ActionResult, ActionView, DrillView, Feedback, Mode};
use crate::scoring;
use crate::spots::SpotInfo;
use anyhow::{anyhow, bail, Result};
use poker_core::card::{card_from_str, card_to_string, Card};
use poker_core::describe::{Bucket, HandDesc};
use poker_core::equity::EquityTable;
use poker_core::flops::label as texture_label;
use poker_core::hand169;
use poker_core::rng::Rng;
use postflop_solver::{solve, Action, PostFlopGame};
use preflop::PostflopSpot;
use regex::Regex;
use serde::{Deserialize, Serialize};
use solver_api::{BetSizes, SpotConfig};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

const STACKS: [u32; 3] = [40, 50, 100];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    Fold,
    Check,
    Call,
    Bet,
    Raise,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Act {
    /// 0 preflop, 1 flop, 2 turn, 3 river
    pub street: u8,
    pub player: String,
    pub kind: Kind,
    /// total put in on this street after the action, in dollars
    pub amount: f64,
    pub all_in: bool,
    /// position of this act in `Hand::acts`
    #[serde(default)]
    pub index: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Hand {
    pub id: String,
    pub no: u32,
    pub played: String,
    pub dealer: String,
    /// (seat, name, stack $) in seat order
    pub seats: Vec<(u32, String, f64)>,
    pub sb: Option<String>,
    pub bb: Option<String>,
    pub bb_size: f64,
    pub hero_cards: Option<(Card, Card)>,
    pub board: Vec<Card>,
    pub acts: Vec<Act>,
    pub straddle: bool,
    pub collected: Vec<(String, f64)>,
    pub returned: Vec<(String, f64)>,
    /// cards shown at showdown
    #[serde(default)]
    pub shows: Vec<(String, Vec<String>)>,
}

fn name_of(re: &Regex, s: &str) -> Option<String> {
    re.captures(s).map(|c| c[1].to_string())
}

fn cards_in(s: &str) -> Vec<Card> {
    s.split(|c: char| c == ',' || c == '[' || c == ']' || c.is_whitespace())
        .filter(|t| !t.is_empty())
        .filter_map(|t| card_from_str(t.trim_end_matches('.')).ok())
        .collect()
}

fn money(s: &str) -> f64 {
    Regex::new(r"(\d+(?:\.\d+)?)").unwrap().captures(s).and_then(|c| c[1].parse().ok()).unwrap_or(0.0)
}

/// Parses a PokerNow CSV log (columns entry, at, order).
pub fn parse(csv_text: &str) -> Result<Vec<Hand>> {
    let mut rdr = csv::ReaderBuilder::new().flexible(true).from_reader(csv_text.as_bytes());
    let mut rows: Vec<(i64, String, String)> = Vec::new();
    for r in rdr.records() {
        let r = r?;
        let entry = r.get(0).unwrap_or("").to_string();
        let at = r.get(1).unwrap_or("").to_string();
        let order: i64 = r.get(2).and_then(|x| x.trim().parse().ok()).unwrap_or(0);
        rows.push((order, at, entry));
    }
    if rows.is_empty() {
        bail!("the log is empty (only a header row). Download the full log again from PokerNow.");
    }
    rows.sort_by_key(|r| r.0);
    let player = Regex::new(r#""(.+?) @ [^"]+""#).unwrap();
    let start = Regex::new(r"-- starting hand #(\d+) \(id: ([^)]+)\)").unwrap();
    let seat = Regex::new(r#"#(\d+) "(.+?) @ [^"]+" \((\d+(?:\.\d+)?)\)"#).unwrap();
    let mut hands = Vec::new();
    let mut cur: Option<Hand> = None;
    let mut street = 0u8;
    for (_, at, e) in rows {
        if let Some(c) = start.captures(&e) {
            cur = Some(Hand { no: c[1].parse().unwrap_or(0), id: c[2].to_string(), played: at.clone(), dealer: e.split("dealer: ").nth(1).and_then(|d| name_of(&player, d)).unwrap_or_default(), ..Default::default() });
            street = 0;
            continue;
        }
        if e.starts_with("-- ending hand") {
            if let Some(mut h) = cur.take() {
                for (i, a) in h.acts.iter_mut().enumerate() {
                    a.index = i;
                }
                hands.push(h);
            }
            continue;
        }
        let Some(h) = cur.as_mut() else { continue };
        if e.starts_with("Player stacks:") {
            h.seats = seat.captures_iter(&e).map(|c| (c[1].parse().unwrap_or(0), c[2].to_string(), c[3].parse().unwrap_or(0.0))).collect();
            h.seats.sort_by_key(|s| s.0);
        } else if let Some(rest) = e.strip_prefix("Your hand is ") {
            let cs = cards_in(rest);
            if cs.len() == 2 {
                h.hero_cards = Some((cs[0], cs[1]));
            }
        } else if e.starts_with("Flop:") || e.starts_with("flop:") {
            street = 1;
            h.board = cards_in(e.split_once(':').unwrap().1);
        } else if e.starts_with("Turn:") || e.starts_with("turn:") {
            street = 2;
            h.board = cards_in(e.split_once(':').unwrap().1);
        } else if e.starts_with("River:") || e.starts_with("river:") {
            street = 3;
            h.board = cards_in(e.split_once(':').unwrap().1);
        } else if e.starts_with("Uncalled bet") {
            if let Some(n) = name_of(&player, &e) {
                h.returned.push((n, money(&e)));
            }
        } else if let Some(n) = name_of(&player, &e) {
            if !e.starts_with('"') {
                continue; // admin messages that mention a player
            }
            let rest = e.split_once("\" ").map(|x| x.1).unwrap_or("");
            let all_in = rest.contains("all in");
            if rest.starts_with("posts a small blind") {
                // blinds are stored as street 9 "posts", not decisions
                h.sb = Some(n.clone());
                h.acts.push(Act { street: 9, player: n, kind: Kind::Bet, amount: money(rest), all_in: false, index: 0 });
            } else if rest.starts_with("posts a big blind") {
                h.bb = Some(n.clone());
                h.bb_size = money(rest);
                h.acts.push(Act { street: 9, player: n, kind: Kind::Bet, amount: h.bb_size, all_in: false, index: 0 });
            } else if rest.starts_with("posts a straddle") || rest.contains("posts a missing") || rest.contains("posts a missed") || rest.contains("ante") {
                h.straddle = true;
            } else if rest.starts_with("folds") {
                h.acts.push(Act { street, player: n, kind: Kind::Fold, amount: 0.0, all_in, index: 0 });
            } else if rest.starts_with("checks") {
                h.acts.push(Act { street, player: n, kind: Kind::Check, amount: 0.0, all_in, index: 0 });
            } else if rest.starts_with("calls") {
                h.acts.push(Act { street, player: n, kind: Kind::Call, amount: money(rest), all_in, index: 0 });
            } else if rest.starts_with("bets") {
                h.acts.push(Act { street, player: n, kind: Kind::Bet, amount: money(rest), all_in, index: 0 });
            } else if rest.starts_with("raises") {
                h.acts.push(Act { street, player: n, kind: Kind::Raise, amount: money(rest), all_in, index: 0 });
            } else if rest.starts_with("collected") {
                h.collected.push((n, money(rest)));
            } else if rest.starts_with("shows") {
                h.shows.push((n, cards_in(rest).iter().map(|&c| card_to_string(c)).collect()));
            }
        }
    }
    Ok(hands)
}

/// Players seen in the log with how many hands each played, most first.
pub fn players(hands: &[Hand]) -> Vec<(String, usize)> {
    let mut c: BTreeMap<String, usize> = BTreeMap::new();
    for h in hands {
        for s in &h.seats {
            *c.entry(s.1.clone()).or_default() += 1;
        }
    }
    let mut v: Vec<(String, usize)> = c.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1));
    v
}

/// Positions in preflop action order (first to act ... BB).
fn positions(h: &Hand) -> Option<Vec<(String, String)>> {
    let n = h.seats.len();
    if !(3..=6).contains(&n) {
        return None;
    }
    let names = match n {
        3 => vec!["BTN", "SB", "BB"],
        4 => vec!["CO", "BTN", "SB", "BB"],
        5 => vec!["HJ", "CO", "BTN", "SB", "BB"],
        _ => vec!["UTG", "HJ", "CO", "BTN", "SB", "BB"],
    };
    let d = h.seats.iter().position(|s| s.1 == h.dealer)?;
    // clockwise from the dealer: dealer, SB, BB, then first to act...
    let order: Vec<String> = (0..n).map(|i| h.seats[(d + i) % n].1.clone()).collect();
    let bb_name = h.bb.clone()?;
    let bbi = order.iter().position(|x| *x == bb_name)?;
    let pre: Vec<String> = (1..=n).map(|i| order[(bbi + i) % n].clone()).collect();
    if pre.last()? != &bb_name {
        return None;
    }
    Some(pre.into_iter().zip(names.into_iter().map(String::from)).collect())
}

fn class_of(c: (Card, Card)) -> usize {
    hand169::class_of(c.0, c.1)
}

fn nearest_stack(bb: f64) -> u32 {
    *STACKS.iter().min_by(|a, b| ((**a as f64) - bb).abs().partial_cmp(&((**b as f64) - bb).abs()).unwrap()).unwrap()
}

/// A graded decision ready to be stored.
pub struct Graded {
    pub view: DrillView,
    pub fb: Feedback,
    /// index of the hero's act in `Hand::acts`
    pub act_index: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostJob {
    pub hand_id: String,
    pub hand: Hand,
    pub hero: String,
    pub villain: String,
    pub villain_display: String,
    pub spot: SpotConfig,
    pub info: SpotInfo,
    /// postflop acts only
    pub acts: Vec<Act>,
    pub oop: String,
}

pub struct Analyzer {
    data: PathBuf,
    eq: Option<EquityTable>,
    preflop: HashMap<(usize, u32), preflop::Solver>,
}

impl Analyzer {
    pub fn new(data: &Path) -> Self {
        Analyzer { data: data.to_path_buf(), eq: None, preflop: HashMap::new() }
    }

    fn solver(&mut self, table: usize, stack: u32) -> Result<&preflop::Solver> {
        if !self.preflop.contains_key(&(table, stack)) {
            if self.eq.is_none() {
                self.eq = Some(EquityTable::load_or_compute(&self.data.join("equity169.json"), 20_000)?);
            }
            let e = self.eq.as_ref().unwrap();
            let eq = EquityTable { samples: e.samples, eq: e.eq.clone(), w: e.w.clone() };
            let mut s = preflop::Solver::new(preflop::Config::new(table, stack as f64), eq);
            s.solve(3000);
            self.preflop.insert((table, stack), s);
        }
        Ok(&self.preflop[&(table, stack)])
    }

    pub fn solver_pub(&mut self, table: usize, stack: u32) -> Result<&preflop::Solver> {
        self.solver(table, stack)
    }

    fn ranges(&self, stack: u32) -> Result<BTreeMap<String, PostflopSpot>> {
        let p = self.data.join("ranges").join(format!("3max-{stack}bb.json"));
        Ok(serde_json::from_str(&std::fs::read_to_string(&p).map_err(|e| anyhow!("{}: {e}", p.display()))?)?)
    }

    /// Preflop grading for one hand plus, if the hand qualifies, a postflop job.
    /// Returns (graded preflop decisions, status, note, optional postflop job).
    pub fn preflop(&mut self, h: &Hand, hero: &str, aliases: &HashMap<String, String>) -> Result<(Vec<Graded>, String, String, Option<PostJob>)> {
        let Some(hero_cards) = h.hero_cards else { return Ok((vec![], "skipped".into(), "no hole cards in the log".into(), None)) };
        if !h.seats.iter().any(|s| s.1 == hero) {
            return Ok((vec![], "skipped".into(), "you weren't dealt in".into(), None));
        }
        if h.straddle {
            return Ok((vec![], "skipped".into(), "straddle or ante (not modeled)".into(), None));
        }
        let Some(pos) = positions(h) else { return Ok((vec![], "skipped".into(), format!("{} players (model covers 3-6)", h.seats.len()), None)) };
        let bb = h.bb_size.max(1e-9);
        let n = pos.len();
        let table = if n == 3 { 3 } else { 6 };
        let pos_of = |name: &str| pos.iter().find(|p| p.0 == name).map(|p| p.1.clone()).unwrap_or_default();
        let stack_of = |name: &str| h.seats.iter().find(|s| s.1 == name).map(|s| s.2 / bb).unwrap_or(0.0);
        let hero_stack = stack_of(hero);
        let others_max = h.seats.iter().filter(|s| s.1 != hero).map(|s| s.2 / bb).fold(0.0, f64::max);
        let eff = hero_stack.min(others_max);
        let bucket = nearest_stack(eff);
        // 6-max model: seats missing at a 4-5 handed table have folded already
        let mut path: Vec<String> = if table == 6 { vec!["fold".to_string(); 6 - n] } else { vec![] };
        let cls = class_of(hero_cards);
        // street-0 contributions so far
        let mut put: HashMap<String, f64> = HashMap::new();
        if let Some(s) = &h.sb {
            put.insert(s.clone(), h.acts.iter().find(|a| a.street == 9 && &a.player == s).map(|a| a.amount).unwrap_or(bb / 2.0));
        }
        if let Some(b) = &h.bb {
            put.insert(b.clone(), bb);
        }
        #[derive(Clone, PartialEq)]
        enum St {
            Unopened,
            Opened(String),
            Called(String, String),
            ThreeBet(String, String),
            ThreeBetCalled(String, String),
            Done,
            Broken(String),
        }
        let mut st = St::Unopened;
        let mut graded = Vec::new();
        let mut rng = Rng::new(h.no as u64);
        let deep = bucket >= 60;
        for a in h.acts.iter().filter(|a| a.street == 0) {
            let me = a.player == hero;
            let before_pot: f64 = put.values().sum::<f64>() / bb;
            let to_call = (put.values().cloned().fold(0.0, f64::max) - put.get(&a.player).cloned().unwrap_or(0.0)).max(0.0) / bb;
            let stack_bb = stack_of(&a.player);
            let is_jam = a.all_in || a.amount / bb >= 0.9 * stack_bb;
            // the decision options in the model at this point, and what the action maps to
            let (situation, mapped): (Option<&str>, Option<&str>) = match &st {
                St::Unopened => (Some("pf_first_in"), Some(match a.kind { Kind::Fold => "fold", Kind::Raise | Kind::Bet => "open", _ => "limp" })),
                St::Opened(o) if &a.player != o => (Some("pf_vs_open"), Some(match a.kind { Kind::Fold => "fold", Kind::Call => "call", _ => if is_jam && bucket < 35 { "jam" } else { "3bet" } })),
                St::ThreeBet(o, _) if &a.player == o => (Some("pf_vs_3bet"), Some(match a.kind { Kind::Fold => "fold", Kind::Call => "call", _ => if deep { "4bet" } else { "jam" } })),
                _ => (None, None),
            };
            if me {
                if let (Some(sit), Some(mv)) = (situation, mapped) {
                    let path_ref: Vec<&str> = path.iter().map(|s| s.as_str()).collect();
                    let solver = self.solver(table, bucket)?;
                    if let Ok((_p, names, evs)) = solver.action_evs(&path_ref) {
                        let (_, _, avg) = solver.strategy_at(&path_ref).map_err(|e| anyhow!(e))?;
                        let range = solver.action_freqs(&path_ref).map_err(|e| anyhow!(e))?;
                        let k = names.len();
                        let mut labels: Vec<String> = names.iter().map(|n| pf_label(n, to_call, bb, &put, &a.player)).collect();
                        let mut kinds: Vec<String> = names.iter().map(|n| pf_kind(n).to_string()).collect();
                        let mut ev: Vec<f64> = (0..k).map(|i| evs[i][cls]).collect();
                        let mut freq: Vec<f64> = (0..k).map(|i| avg[i * hand169::N + cls]).collect();
                        let mut rfreq: Vec<f64> = range.iter().map(|r| r.1).collect();
                        let chosen = match names.iter().position(|n| n == mv) {
                            Some(i) => i,
                            None => {
                                // limp (or an option the model lacks): graded against folding
                                let fold_ev = names.iter().position(|n| n == "fold").map(|i| ev[i]).unwrap_or(0.0);
                                labels.push(if mv == "limp" { "Limp".into() } else { mv.to_string() });
                                kinds.push(mv.to_string());
                                ev.push(fold_ev);
                                freq.push(0.0);
                                rfreq.push(0.0);
                                k
                            }
                        };
                        let score = scoring::score(&ev, &freq, chosen, before_pot, None);
                        let acts: Vec<ActionResult> = (0..labels.len())
                            .map(|i| ActionResult { label: labels[i].clone(), kind: kinds[i].clone(), ev_bb: ev[i], freq_hand: freq[i], freq_range: rfreq[i] })
                            .collect();
                        let mut reason = format!(
                            "Preflop model ({}-handed, {}bb): with {} the solver plays {}.",
                            table,
                            bucket,
                            hand169::name(cls),
                            acts.iter().filter(|x| x.freq_hand >= 0.05).map(|x| format!("{} {:.0}%", x.label, x.freq_hand * 100.0)).collect::<Vec<_>>().join(", ")
                        );
                        if mv == "limp" {
                            reason.push_str(" Limping isn't in the model (raise or fold first in); it's graded as if you folded, so the loss shown is a minimum.");
                        }
                        let view = DrillView {
                            spot_id: format!("pokernow:{}", h.id),
                            mode: Mode::Spot,
                            stack: bucket,
                            matchup: format!("{} preflop", pos_of(hero)),
                            pot_type: "preflop".into(),
                            street: "preflop".into(),
                            texture: String::new(),
                            board: vec![],
                            hero: pos_of(hero),
                            villain: String::new(),
                            hero_in_position: pos_of(hero) == "BTN",
                            hero_is_aggressor: false,
                            preflop: String::new(),
                            line: vec![],
                            hand: vec![card_to_string(hero_cards.0), card_to_string(hero_cards.1)],
                            pot_bb: before_pot,
                            to_call_bb: to_call,
                            hero_stack_bb: hero_stack,
                            villain_stack_bb: 0.0,
                            actions: labels.iter().enumerate().map(|(i, l)| ActionView { index: i, label: l.clone(), kind: kinds[i].clone(), amount_bb: 0.0 }).collect(),
                            node_type: sit.replace("pf_", "").replace('_', " "),
                            situation: sit.into(),
                            decision: 1,
                            live_solved: false,
                            seats: vec![],
                        };
                        let hand_desc = HandDesc { made: hand169::name(cls), draw: None, bucket: Bucket::Medium };
                        let _ = &mut rng;
                        graded.push(Graded { view, fb: Feedback { chosen, verdict: score.verdict.label().into(), score, actions: acts, hand_desc, equity: 0.0, reason, lesson: None, continues: false, exploit: None }, act_index: Some(a.index) });
                    }
                }
            }
            // advance the model state
            put.insert(a.player.clone(), if a.kind == Kind::Fold { put.get(&a.player).cloned().unwrap_or(0.0) } else { a.amount.max(put.get(&a.player).cloned().unwrap_or(0.0)) });
            st = match (st.clone(), a.kind) {
                (St::Unopened, Kind::Fold) => {
                    path.push("fold".into());
                    St::Unopened
                }
                (St::Unopened, Kind::Raise | Kind::Bet) => {
                    path.push("open".into());
                    St::Opened(a.player.clone())
                }
                (St::Unopened, _) => St::Broken("limped pot (not modeled)".into()),
                (St::Opened(o), Kind::Fold) if a.player != o => {
                    path.push("fold".into());
                    St::Opened(o)
                }
                (St::Opened(o), Kind::Call) if a.player != o => {
                    path.push("call".into());
                    St::Called(o, a.player.clone())
                }
                (St::Opened(o), Kind::Raise | Kind::Bet) if a.player != o => {
                    path.push(mapped.unwrap_or("3bet").to_string());
                    if mapped == Some("jam") { St::Done } else { St::ThreeBet(o, a.player.clone()) }
                }
                (St::Called(o, c), Kind::Fold) => St::Called(o, c),
                (St::Called(..), _) => St::Broken("multiway or squeezed pot (not modeled)".into()),
                (St::ThreeBet(o, t), Kind::Fold) if a.player != o => St::ThreeBet(o, t),
                (St::ThreeBet(_o, t), Kind::Fold) => {
                    path.push("fold".into());
                    let _ = t;
                    St::Done
                }
                (St::ThreeBet(o, t), Kind::Call) if a.player == o => {
                    path.push("call".into());
                    St::ThreeBetCalled(o, t)
                }
                (St::ThreeBet(..), _) => St::Broken("4-bet or multiway pot (postflop not modeled)".into()),
                (St::ThreeBetCalled(o, t), Kind::Fold) => St::ThreeBetCalled(o, t),
                (s @ St::Broken(_), _) => s,
                (_, _) => St::Broken("action outside the model".into()),
            };
        }

        // postflop eligibility
        let has_post = h.acts.iter().any(|a| a.street >= 1 && a.street <= 3);
        let (status, note, job) = match st {
            St::Called(o, c) | St::ThreeBetCalled(o, c) if has_post && (o == hero || c == hero) => {
                let srp = h.acts.iter().filter(|a| a.street == 0 && matches!(a.kind, Kind::Raise | Kind::Bet)).count() == 1;
                let (po, pc) = (pos_of(&o), pos_of(&c));
                let key = format!("{po}v{pc}-{}", if srp { "srp" } else { "3bp" });
                let villain = if o == hero { c.clone() } else { o.clone() };
                match self.ranges(bucket)?.get(&key).cloned() {
                    None => ("preflop only".to_string(), format!("{po} vs {pc} isn't a solved matchup yet"), None),
                    Some(r) => {
                        let pot: f64 = put.values().sum::<f64>() / bb;
                        let rem = |name: &str| stack_of(name) - put.get(name).cloned().unwrap_or(0.0) / bb;
                        let eff = rem(&o).min(rem(&c));
                        let flop: String = h.board.iter().take(3).map(|&c| card_to_string(c)).collect();
                        if h.board.len() < 3 {
                            ("preflop only".into(), "no flop".into(), None)
                        } else {
                            let oop = if r.oop == po { o.clone() } else { c.clone() };
                            let spot = SpotConfig {
                                id: format!("pokernow-{}", h.id),
                                oop_range: r.oop_range.clone(),
                                ip_range: r.ip_range.clone(),
                                flop: flop.clone(),
                                pot_bb: pot,
                                stack_bb: eff,
                                sizes: BetSizes::for_spot(srp, eff / pot),
                                store_river: true,
                                target_exploitability_pct: 1.0,
                                max_iterations: 500,
                            };
                            let fl: Vec<Card> = h.board[..3].to_vec();
                            let info = SpotInfo {
                                id: spot.id.clone(),
                                file: PathBuf::new(),
                                stack: bucket,
                                matchup: format!("{po} vs {pc}"),
                                pot_type: if srp { "srp".into() } else { "3bp".into() },
                                flop,
                                texture: texture_label([fl[0], fl[1], fl[2]]),
                                flop_weight: 0.0,
                                oop: r.oop.clone(),
                                ip: r.ip.clone(),
                                preflop_aggressor: if srp { po.clone() } else { pc.clone() },
                                pot_bb: pot,
                                stack_bb: eff,
                                river_bet: spot.sizes.river_bet.clone(),
                                river_raise: spot.sizes.river_raise.clone(),
                            };
                            let job = PostJob {
                                hand_id: h.id.clone(),
                                hand: h.clone(),
                                hero: hero.to_string(),
                                villain_display: aliases.get(&villain).cloned().unwrap_or(villain.clone()),
                                villain,
                                spot,
                                info,
                                acts: h.acts.iter().filter(|a| (1..=3).contains(&a.street)).cloned().collect(),
                                oop,
                            };
                            ("queued".into(), String::new(), Some(job))
                        }
                    }
                }
            }
            St::Broken(why) => (if graded.is_empty() { "skipped".into() } else { "preflop only".into() }, why, None),
            _ => (if graded.is_empty() { "skipped".into() } else { "preflop only".into() }, if has_post { "you weren't in the postflop pot".into() } else { String::new() }, None),
        };
        Ok((graded, status, note, job))
    }
}

fn pf_kind(n: &str) -> &'static str {
    match n {
        "fold" => "fold",
        "call" => "call",
        "jam" => "allin",
        _ => "raise",
    }
}

fn pf_label(n: &str, to_call: f64, _bb: f64, _put: &HashMap<String, f64>, _who: &str) -> String {
    match n {
        "fold" => "Fold".into(),
        "call" => format!("Call {:.1}bb", to_call),
        "open" => "Raise (open)".into(),
        "3bet" => "3-bet".into(),
        "4bet" => "4-bet".into(),
        "jam" => "All-in".into(),
        x => x.to_string(),
    }
}

/// Re-solves one hand's flop and grades every hero postflop decision. Slow (about a minute).
pub fn analyze_postflop(job: &PostJob, max_gb: f64) -> Result<(Vec<Graded>, String)> {
    let mut spot = job.spot.clone();
    let mut game = solver_pfs::build(&spot)?;
    let gb = |g: &PostFlopGame| g.memory_usage().1 as f64 / 1024f64.powi(3);
    if gb(&game) > max_gb {
        spot.sizes = BetSizes::slim();
        game = solver_pfs::build(&spot)?;
        if gb(&game) > max_gb {
            bail!("too big to solve here ({:.1} GB)", gb(&game));
        }
    }
    game.allocate_memory(true);
    let target = (spot.pot_bb * solver_pfs::BB) as f32 * (spot.target_exploitability_pct as f32 / 100.0);
    solve(&mut game, spot.max_iterations, target, false);
    game.cache_normalized_weights();

    let h = &job.hand;
    let bb = h.bb_size.max(1e-9);
    let hero_cards = h.hero_cards.ok_or_else(|| anyhow!("no hole cards"))?;
    let hero_idx = if job.hero == job.oop { 0 } else { 1 };
    let mut rng = Rng::new(h.no as u64 ^ 0x9e37);
    let mut line = Vec::new();
    let mut events: Vec<(usize, usize, char)> = Vec::new();
    let mut graded = Vec::new();
    let mut notes = Vec::new();
    let mut cur_street = 1u8;
    for a in &job.acts {
        // deal the turn / river when the street changes
        while a.street > cur_street {
            cur_street += 1;
            let idx = cur_street as usize + 1; // board index: turn (street 2) = 3, river (street 3) = 4
            let card = *h.board.get(idx).ok_or_else(|| anyhow!("board shorter than the action"))?;
            if !game.is_chance_node() {
                notes.push("action didn't close the street the way the tree expects".to_string());
                return Ok((graded, notes.join("; ")));
            }
            if cur_street == 3 {
                // river: re-solve live with the full size menu (incl. pot and overbets)
                let keep: Vec<(usize, (Card, Card))> = h.hero_cards.map(|c| vec![(hero_idx, c)]).unwrap_or_default();
                game = crate::drill::river_subgame(&mut game, card, &keep)?;
            } else {
                game.play(card as usize);
                game.cache_normalized_weights();
            }
            line.push(format!("{} {}", if cur_street == 2 { "Turn" } else { "River" }, card_to_string(card)));
        }
        if game.is_terminal_node() || game.is_chance_node() {
            notes.push("action continued past where the tree ends".into());
            break;
        }
        let p = game.current_player();
        let who_idx = if a.player == job.oop { 0 } else { 1 };
        if p != who_idx {
            notes.push("action order didn't match the tree".into());
            break;
        }
        let actions = game.available_actions();
        let chips = (a.amount / bb * solver_pfs::BB).round() as i32;
        let pick = match a.kind {
            Kind::Fold => actions.iter().position(|x| matches!(x, Action::Fold)),
            Kind::Check => actions.iter().position(|x| matches!(x, Action::Check)),
            Kind::Call => actions.iter().position(|x| matches!(x, Action::Call)),
            Kind::Bet | Kind::Raise => actions
                .iter()
                .enumerate()
                .filter(|(_, x)| matches!(x, Action::Bet(_) | Action::Raise(_) | Action::AllIn(_)))
                .min_by_key(|(_, x)| match x {
                    Action::Bet(v) | Action::Raise(v) | Action::AllIn(v) => (v - chips).abs(),
                    _ => i32::MAX,
                })
                .map(|(i, _)| i),
        };
        let Some(ai) = pick else {
            notes.push(format!("{:?} isn't available in the tree at this point", a.kind));
            break;
        };
        let tree_amt = match actions[ai] {
            Action::Bet(v) | Action::Raise(v) | Action::AllIn(v) => Some(v),
            _ => None,
        };
        if p == hero_idx {
            let (mut view, mut fb) = grade_real(&mut game, &job.info, hero_idx, hero_cards, &line, &events, ai, &mut rng)?;
            view.spot_id = format!("pokernow:{}", h.id);
            if let Some(v) = tree_amt {
                let real = a.amount / bb;
                let tree = v as f64 / solver_pfs::BB;
                if (real - tree).abs() / real.max(0.01) > 0.15 {
                    fb.reason = format!("(You put in {real:.1}bb; graded as {tree:.1}bb, the nearest size in the tree.) {}", fb.reason);
                }
            }
            graded.push(Graded { view, fb, act_index: Some(a.index) });
        }
        let before = game.total_bet_amount();
        let who = if p == 0 { job.info.oop.clone() } else { job.info.ip.clone() };
        line.push(crate::drill::describe_action_pub(&who, actions[ai], before, p));
        let kc = match actions[ai] {
            Action::Fold => 'f',
            Action::Check => 'x',
            Action::Call => 'c',
            Action::Bet(_) => 'b',
            _ => 'r',
        };
        events.push((game.current_board().len(), p, kc));
        game.play(ai);
        game.cache_normalized_weights();
    }
    Ok((graded, notes.join("; ")))
}

/// Hero's net result in a hand, in big blinds (from the log's own accounting).
pub fn net_bb(h: &Hand, hero: &str) -> f64 {
    let mut put = 0.0;
    let mut street_put: HashMap<u8, f64> = HashMap::new();
    for a in &h.acts {
        if a.player != hero {
            continue;
        }
        let s = if a.street == 9 { 0 } else { a.street };
        if matches!(a.kind, Kind::Bet | Kind::Raise | Kind::Call) {
            let e = street_put.entry(s).or_insert(0.0);
            *e = e.max(a.amount);
        }
    }
    put += street_put.values().sum::<f64>();
    let won: f64 = h.collected.iter().filter(|c| c.0 == hero).map(|c| c.1).sum::<f64>() + h.returned.iter().filter(|c| c.0 == hero).map(|c| c.1).sum::<f64>();
    (won - put) / h.bb_size.max(1e-9)
}

pub fn save_graded(db: &Db, g: &Graded, opponent: Option<&str>) -> Result<()> {
    db.record(&g.view, &g.fb, &Context { source: "import".into(), opponent: opponent.map(String::from), history: None, hero_cards: None, act_index: g.act_index.map(|i| i as i64) })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOG: &str = r#"entry,at,order
"-- starting hand #1 (id: abc123)  No Limit Texas Hold'em (dealer: ""Hero @ h1"") --",2026-10-08T03:28:56.000Z,2
"Player stacks: #1 ""Hero @ h1"" (20.00) | #2 ""Vil @ v1"" (20.00) | #3 ""Third @ t1"" (20.00)",2026-10-08T03:28:56.000Z,3
"Your hand is A♠, Q♦",2026-10-08T03:28:56.000Z,4
"""Vil @ v1"" posts a small blind of 0.20",2026-10-08T03:28:56.000Z,5
"""Third @ t1"" posts a big blind of 0.40",2026-10-08T03:28:56.000Z,6
"""Hero @ h1"" raises to 1.00",2026-10-08T03:28:56.000Z,7
"""Vil @ v1"" folds",2026-10-08T03:28:56.000Z,8
"""Third @ t1"" calls 1.00",2026-10-08T03:28:56.000Z,9
"Flop:  [10♦, 2♣, 2♥]",2026-10-08T03:28:56.000Z,10
"""Third @ t1"" checks",2026-10-08T03:28:56.000Z,11
"""Hero @ h1"" bets 0.70",2026-10-08T03:28:56.000Z,12
"""Third @ t1"" folds",2026-10-08T03:28:56.000Z,13
"Uncalled bet of 0.70 returned to ""Hero @ h1""",2026-10-08T03:28:56.000Z,14
"""Hero @ h1"" collected 2.20 from pot",2026-10-08T03:28:56.000Z,15
"-- ending hand #1 --",2026-10-08T03:28:56.000Z,16
"#;

    #[test]
    fn parses_a_hand() {
        let hands = parse(LOG).unwrap();
        assert_eq!(hands.len(), 1);
        let h = &hands[0];
        assert_eq!(h.id, "abc123");
        assert_eq!(h.dealer, "Hero");
        assert_eq!(h.seats.len(), 3);
        assert_eq!(h.sb.as_deref(), Some("Vil"));
        assert_eq!(h.bb.as_deref(), Some("Third"));
        assert!((h.bb_size - 0.4).abs() < 1e-9);
        assert!(h.hero_cards.is_some());
        assert_eq!(h.board.len(), 3);
        let pos = positions(h).unwrap();
        assert_eq!(pos[0], ("Hero".to_string(), "BTN".to_string()));
        assert_eq!(pos[2], ("Third".to_string(), "BB".to_string()));
        let decisions: Vec<&Act> = h.acts.iter().filter(|a| a.street != 9).collect();
        assert_eq!(decisions.len(), 6); // raise, fold, call, check, bet, fold
        assert!((net_bb(h, "Hero") - (2.2 + 0.7 - 1.0 - 0.7) / 0.4).abs() < 1e-9);
    }

    #[test]
    fn empty_log_is_an_error() {
        assert!(parse("entry,at,order\n").is_err());
    }
}

// ---------- storage for imported hands ----------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandRow {
    pub opps: String,
    pub hand_id: String,
    pub hand_no: i64,
    pub played: String,
    pub hero_cards: String,
    pub board: String,
    pub hero_pos: String,
    pub villain: String,
    pub pot_type: String,
    pub status: String,
    pub note: String,
    pub decisions: i64,
    pub ev_loss: f64,
    pub net_bb: f64,
}

/// True if the hand is already imported with its full log (older imports without it are
/// redone so the replayer and results have everything).
pub fn hand_exists(db: &Db, hand_id: &str) -> Result<bool> {
    let n: i64 = db.conn().query_row("SELECT COUNT(*) FROM import_hand WHERE hand_id = ?1 AND hand_json IS NOT NULL", [hand_id], |r| r.get(0))?;
    if n == 0 {
        db.conn().execute("DELETE FROM attempt WHERE spot_id = ?1", [format!("pokernow:{hand_id}")])?;
        db.conn().execute("DELETE FROM import_hand WHERE hand_id = ?1", [hand_id])?;
    }
    Ok(n > 0)
}

/// Players who were still in at the end (or showed), excluding the hero, by display name.
pub fn opponents(h: &Hand, hero: &str, names: &HashMap<String, String>) -> Vec<String> {
    let folded: std::collections::HashSet<&str> = h.acts.iter().filter(|a| a.kind == Kind::Fold).map(|a| a.player.as_str()).collect();
    h.seats
        .iter()
        .map(|s| s.1.as_str())
        .filter(|p| *p != hero && !folded.contains(p) && h.acts.iter().any(|a| &a.player == p))
        .map(|p| names.get(p).cloned().unwrap_or(p.to_string()))
        .collect()
}

#[allow(clippy::too_many_arguments)]
pub fn save_hand(db: &Db, file: &str, h: &Hand, hero: &str, hero_pos: &str, villain: &str, pot_type: &str, status: &str, note: &str, graded: &[Graded], opps: &[String]) -> Result<()> {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs() as i64;
    let cards = h.hero_cards.map(|(a, b)| format!("{}{}", card_to_string(a.max(b)), card_to_string(a.min(b)))).unwrap_or_default();
    let board: String = h.board.iter().map(|&c| card_to_string(c)).collect();
    let loss: f64 = graded.iter().map(|g| g.fb.score.ev_loss_bb).sum();
    db.conn().execute(
        "INSERT OR REPLACE INTO import_hand (hand_id, file, hand_no, played, hero, hero_cards, board, players, hero_pos, villain, pot_type, stack_bb, status, note, decisions, ev_loss, net_bb, imported_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
        rusqlite::params![h.id, file, h.no, h.played, hero, cards, board, h.seats.len() as i64, hero_pos, villain, pot_type, 0.0, status, note, graded.len() as i64, loss, net_bb(h, hero), now],
    )?;
    db.conn().execute("UPDATE import_hand SET hand_json = ?2, opps = ?3 WHERE hand_id = ?1", rusqlite::params![h.id, serde_json::to_string(h)?, opps.join(", ")])?;
    Ok(())
}

pub fn save_job(db: &Db, job: &PostJob) -> Result<()> {
    db.conn().execute("UPDATE import_hand SET job = ?2 WHERE hand_id = ?1", rusqlite::params![job.hand_id, serde_json::to_string(job)?])?;
    Ok(())
}

/// Postflop jobs that were queued but never finished (app closed mid-import).
pub fn pending_jobs(db: &Db) -> Result<Vec<PostJob>> {
    let mut st = db.conn().prepare("SELECT job FROM import_hand WHERE status = 'queued' AND job IS NOT NULL ORDER BY played")?;
    let jobs = st
        .query_map([], |r| r.get::<_, String>(0))?
        .filter_map(|r| r.ok())
        .filter_map(|j| serde_json::from_str(&j).ok())
        .collect();
    Ok(jobs)
}

pub fn finish_hand(db: &Db, hand_id: &str, status: &str, note: &str, graded: &[Graded]) -> Result<()> {
    let loss: f64 = graded.iter().map(|g| g.fb.score.ev_loss_bb).sum();
    db.conn().execute(
        "UPDATE import_hand SET status = ?2, note = CASE WHEN ?3 = '' THEN note ELSE ?3 END, decisions = decisions + ?4, ev_loss = ev_loss + ?5, job = NULL WHERE hand_id = ?1",
        rusqlite::params![hand_id, status, note, graded.len() as i64, loss],
    )?;
    Ok(())
}

pub fn list_hands(db: &Db) -> Result<Vec<HandRow>> {
    let mut st = db.conn().prepare(
        "SELECT hand_id, hand_no, played, hero_cards, board, hero_pos, villain, pot_type, status, note, decisions, ev_loss, COALESCE(net_bb, 0), COALESCE(opps, '')
         FROM import_hand ORDER BY played DESC, hand_no DESC",
    )?;
    let rows = st
        .query_map([], |r| {
            Ok(HandRow {
                hand_id: r.get(0)?,
                hand_no: r.get(1)?,
                played: r.get(2)?,
                hero_cards: r.get(3)?,
                board: r.get(4)?,
                hero_pos: r.get(5)?,
                villain: r.get(6)?,
                pot_type: r.get(7)?,
                status: r.get(8)?,
                note: r.get(9)?,
                decisions: r.get(10)?,
                ev_loss: r.get(11)?,
                net_bb: r.get(12)?,
                opps: r.get(13)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionRow {
    pub street: String,
    pub situation: String,
    pub board: String,
    pub chosen: String,
    pub best: String,
    pub ev_loss_bb: f64,
    pub ev_loss_pct: f64,
    pub verdict: String,
}

pub fn hand_decisions(db: &Db, hand_id: &str) -> Result<Vec<DecisionRow>> {
    let mut st = db.conn().prepare(
        "SELECT street, situation, board, chosen, best, ev_loss_bb, COALESCE(ev_loss_pct, 0), verdict FROM attempt WHERE spot_id = ?1 ORDER BY id",
    )?;
    let rows = st
        .query_map([format!("pokernow:{hand_id}")], |r| {
            Ok(DecisionRow {
                street: r.get(0)?,
                situation: r.get(1)?,
                board: r.get(2)?,
                chosen: r.get(3)?,
                best: r.get(4)?,
                ev_loss_bb: r.get(5)?,
                ev_loss_pct: r.get(6)?,
                verdict: r.get(7)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayDecision {
    pub act_index: usize,
    pub feedback: Feedback,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Replay {
    pub hand: Hand,
    pub hero: String,
    /// (player, position) in preflop order
    pub positions: Vec<(String, String)>,
    pub names: HashMap<String, String>,
    pub decisions: Vec<ReplayDecision>,
    /// the same cards as text ("Ah") for the UI
    pub board: Vec<String>,
    pub hero_cards: Vec<String>,
}

pub fn replay(db: &Db, hand_id: &str) -> Result<Replay> {
    let (json, hero): (Option<String>, String) = db.conn().query_row("SELECT hand_json, hero FROM import_hand WHERE hand_id = ?1", [hand_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
    let hand: Hand = serde_json::from_str(&json.ok_or_else(|| anyhow!("this hand was imported before replays existed: import the log again"))?)?;
    let mut st = db.conn().prepare("SELECT act_index, feedback FROM attempt WHERE spot_id = ?1 AND act_index IS NOT NULL AND feedback IS NOT NULL ORDER BY id")?;
    let decisions = st
        .query_map([format!("pokernow:{hand_id}")], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
        .filter_map(|x| x.ok())
        .filter_map(|(i, f)| serde_json::from_str(&f).ok().map(|feedback| ReplayDecision { act_index: i as usize, feedback }))
        .collect();
    let board = hand.board.iter().map(|&c| card_to_string(c)).collect();
    let hero_cards = hand.hero_cards.map(|(a, b)| vec![card_to_string(a), card_to_string(b)]).unwrap_or_default();
    Ok(Replay { positions: positions(&hand).unwrap_or_default(), names: aliases(db)?, hand, hero, decisions, board, hero_cards })
}

pub fn aliases(db: &Db) -> Result<HashMap<String, String>> {
    let mut st = db.conn().prepare("SELECT name, display FROM alias")?;
    let m = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?.collect::<std::result::Result<HashMap<_, _>, _>>()?;
    Ok(m)
}

pub fn set_alias(db: &Db, name: &str, display: &str) -> Result<()> {
    if display.trim().is_empty() {
        db.conn().execute("DELETE FROM alias WHERE name = ?1", [name])?;
    } else {
        db.conn().execute("INSERT OR REPLACE INTO alias (name, display) VALUES (?1, ?2)", [name, display.trim()])?;
    }
    Ok(())
}

/// Positions of every player in a hand ("" if the table size isn't modeled).
pub fn position_of(h: &Hand, name: &str) -> String {
    positions(h).and_then(|p| p.into_iter().find(|x| x.0 == name).map(|x| x.1)).unwrap_or_default()
}

/// Tendencies of one opponent across the imported hands.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OppStats {
    pub name: String,
    pub display: String,
    pub hands: usize,
    /// voluntarily put money in preflop / raised preflop, as fractions of hands dealt
    pub vpip: f64,
    pub pfr: f64,
    /// postflop bets and raises per call
    pub aggression: f64,
    /// postflop: folds when facing a bet
    pub fold_to_bet: f64,
    pub faced_bets: usize,
    pub postflop_actions: usize,
}

/// Solver-ish baselines the log profiles are measured against (heads-up postflop).
const BASE_AGGRESSION: f64 = 2.0;
const BASE_FOLD: f64 = 0.40;

pub fn opponent_stats(db: &Db) -> Result<Vec<OppStats>> {
    let names = aliases(db)?;
    let mut st = db.conn().prepare("SELECT hand_json, hero FROM import_hand WHERE hand_json IS NOT NULL")?;
    let rows: Vec<(String, String)> = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.filter_map(|r| r.ok()).collect();
    #[derive(Default)]
    struct Acc { hands: usize, vpip: usize, pfr: usize, aggr: usize, calls: usize, faced: usize, folds: usize }
    let mut acc: BTreeMap<String, Acc> = BTreeMap::new();
    for (json, hero) in rows {
        let Ok(h) = serde_json::from_str::<Hand>(&json) else { continue };
        for s in &h.seats {
            if s.1 != hero {
                acc.entry(s.1.clone()).or_default().hands += 1;
            }
        }
        let mut street = 0u8;
        let mut put: HashMap<&str, f64> = HashMap::new();
        let mut top = 0.0f64;
        let mut vp: std::collections::HashSet<&str> = Default::default();
        let mut pr: std::collections::HashSet<&str> = Default::default();
        for a in &h.acts {
            let s = if a.street == 9 { 0 } else { a.street };
            if s != street {
                street = s;
                put.clear();
                top = 0.0;
            }
            let mine = put.get(a.player.as_str()).copied().unwrap_or(0.0);
            let facing = top > mine + 1e-9;
            if a.street != 9 && a.player != hero {
                let e = acc.entry(a.player.clone()).or_default();
                if s == 0 {
                    if matches!(a.kind, Kind::Call | Kind::Raise | Kind::Bet) {
                        vp.insert(a.player.as_str());
                    }
                    if a.kind == Kind::Raise {
                        pr.insert(a.player.as_str());
                    }
                } else {
                    match a.kind {
                        Kind::Bet | Kind::Raise => e.aggr += 1,
                        Kind::Call => e.calls += 1,
                        _ => {}
                    }
                    if facing {
                        e.faced += 1;
                        if a.kind == Kind::Fold {
                            e.folds += 1;
                        }
                    }
                }
            }
            if matches!(a.kind, Kind::Bet | Kind::Raise | Kind::Call) {
                put.insert(a.player.as_str(), a.amount);
                top = top.max(a.amount);
            }
        }
        for p in vp {
            acc.entry(p.to_string()).or_default().vpip += 1;
        }
        for p in pr {
            acc.entry(p.to_string()).or_default().pfr += 1;
        }
    }
    let mut out: Vec<OppStats> = acc
        .into_iter()
        .filter(|(_, a)| a.hands > 0)
        .map(|(name, a)| OppStats {
            display: names.get(&name).cloned().unwrap_or(name.clone()),
            name,
            hands: a.hands,
            vpip: a.vpip as f64 / a.hands as f64,
            pfr: a.pfr as f64 / a.hands as f64,
            aggression: a.aggr as f64 / (a.calls.max(1)) as f64,
            fold_to_bet: if a.faced > 0 { a.folds as f64 / a.faced as f64 } else { BASE_FOLD },
            faced_bets: a.faced,
            postflop_actions: a.aggr + a.calls,
        })
        .collect();
    out.sort_by(|a, b| b.hands.cmp(&a.hands));
    Ok(out)
}

/// A villain profile from an opponent's logged tendencies (None below 10 faced bets or 15
/// postflop bets/calls: too little to say).
pub fn log_profile(s: &OppStats) -> Option<crate::exploit::Profile> {
    if s.faced_bets < 10 || s.postflop_actions < 15 {
        return None;
    }
    let ag = s.aggression / BASE_AGGRESSION;
    let bluff = ag.clamp(0.3, 3.0);
    let value = ag.sqrt().clamp(0.5, 2.0);
    let fold = (s.fold_to_bet / BASE_FOLD).clamp(0.25, 2.5);
    let call = ((1.0 - s.fold_to_bet) / (1.0 - BASE_FOLD)).clamp(0.5, 2.0);
    let blurb = format!(
        "From your logs ({} hands): VPIP {:.0}%, PFR {:.0}%, aggression {:.1}, folds {:.0}% to bets.",
        s.hands,
        s.vpip * 100.0,
        s.pfr * 100.0,
        s.aggression,
        s.fold_to_bet * 100.0
    );
    Some(crate::exploit::Profile::new(&format!("log:{}", s.name), &s.display, &blurb, bluff, value, fold, call))
}
