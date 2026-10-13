//! Preflop quick-fire: a seat, a situation and a hand; raise / call / fold in a second or two,
//! graded against the preflop solver with EVs.

use crate::import::Analyzer;
use crate::scoring;
use anyhow::{anyhow, Result};
use poker_core::card::card_to_string;
use poker_core::hand169;
use poker_core::rng::Rng;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Quiz {
    pub id: u64,
    pub table: usize,
    pub stack: u32,
    pub scenario: String,
    pub situation: String,
    /// "pf_first_in", "pf_vs_open", "pf_vs_3bet"
    pub kind: String,
    pub seat: String,
    pub hand: Vec<String>,
    pub class: String,
    pub actions: Vec<String>,
    pub kinds: Vec<String>,
    pub pot_bb: f64,
    pub to_call_bb: f64,
    #[serde(skip)]
    ev: Vec<f64>,
    #[serde(skip)]
    freq: Vec<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuizResult {
    pub chosen: usize,
    pub best: usize,
    pub verdict: String,
    pub ev_loss_bb: f64,
    pub ev: Vec<f64>,
    pub freq: Vec<f64>,
    /// one line, e.g. "Solver: Raise 100% (+0.8bb). Folding is 0bb."
    pub why: String,
}

const SCENARIOS3: &[&str] = &["open:BTN", "open:SB", "vs_open:BTN:BB", "vs_open:BTN:SB", "vs_open:SB:BB", "vs_3bet:BTN:BB", "vs_3bet:BTN:SB", "vs_3bet:SB:BB"];
const SCENARIOS6: &[&str] = &["open:UTG", "open:HJ", "open:CO", "open:BTN", "open:SB", "vs_open:CO:BTN", "vs_open:BTN:BB", "vs_open:CO:BB", "vs_open:SB:BB", "vs_3bet:BTN:BB", "vs_3bet:CO:BTN"];

pub fn scenarios(table: usize) -> &'static [&'static str] {
    if table == 3 { SCENARIOS3 } else { SCENARIOS6 }
}

fn describe(scn: &str) -> (String, String, String, f64, f64) {
    let p: Vec<&str> = scn.split(':').collect();
    match p.as_slice() {
        ["open", s] => (format!("Folded to you on the {s}."), s.to_string(), "pf_first_in".into(), if *s == "SB" { 1.5 } else { 1.5 }, if *s == "SB" { 0.5 } else { 0.0 }),
        ["vs_open", o, r] => (format!("{o} raises. You're on the {r}."), r.to_string(), "pf_vs_open".into(), 4.0, if *r == "BB" { 1.5 } else { 2.0 }),
        ["vs_3bet", o, r] => (format!("You raised on the {o}. {r} 3-bets."), o.to_string(), "pf_vs_3bet".into(), 12.0, 7.5),
        _ => (scn.to_string(), String::new(), String::new(), 0.0, 0.0),
    }
}

/// Deals a quiz. `kinds` filters scenario types ("pf_first_in", "pf_vs_open", "pf_vs_3bet").
pub fn deal(an: &mut Analyzer, table: usize, stack: u32, kinds: &[String], rng: &mut Rng) -> Result<Quiz> {
    let pool: Vec<&str> = scenarios(table).iter().copied().filter(|s| kinds.is_empty() || kinds.contains(&describe(s).2)).collect();
    let scn = pool[rng.below(pool.len() as u64) as usize];
    let task = crate::ranges::preflop_task(an, table, stack, scn)?;
    // deal a hand in proportion to how often it reaches this spot, but bias toward hands
    // that aren't trivial (mixed, or the best action isn't obvious from the hand strength)
    let weights: Vec<f64> = task
        .classes
        .iter()
        .map(|c| {
            let maxf = c.freq.iter().cloned().fold(0.0, f64::max);
            c.weight * (0.35 + (1.0 - maxf))
        })
        .collect();
    let total: f64 = weights.iter().sum();
    let mut x = (rng.below(1 << 30) as f64 / (1u64 << 30) as f64) * total;
    let mut cls = 0;
    for (i, w) in weights.iter().enumerate() {
        x -= w;
        if x <= 0.0 && *w > 0.0 {
            cls = i;
            break;
        }
    }
    let combos = hand169::combos(cls);
    let (a, b) = combos[rng.below(combos.len() as u64) as usize];
    let (situation, seat, kind, pot, to_call) = describe(scn);
    let c = &task.classes[cls];
    Ok(Quiz {
        id: rng.next_u64(),
        table,
        stack,
        scenario: scn.into(),
        situation,
        kind,
        seat,
        hand: vec![card_to_string(a.max(b)), card_to_string(a.min(b))],
        class: hand169::name(cls),
        actions: task.actions.clone(),
        kinds: task.kinds.clone(),
        pot_bb: pot,
        to_call_bb: to_call,
        ev: c.ev.clone(),
        freq: c.freq.clone(),
    })
}

pub fn grade(q: &Quiz, chosen: usize) -> Result<QuizResult> {
    if chosen >= q.actions.len() {
        return Err(anyhow!("invalid action"));
    }
    let s = scoring::score(&q.ev, &q.freq, chosen, q.pot_bb, None);
    let mix: Vec<String> = (0..q.actions.len()).filter(|&i| q.freq[i] >= 0.05).map(|i| format!("{} {:.0}%", q.actions[i], q.freq[i] * 100.0)).collect();
    let why = format!(
        "Solver with {}: {}. EV: {}.",
        q.class,
        mix.join(", "),
        (0..q.actions.len()).map(|i| format!("{} {:+.2}bb", q.actions[i], q.ev[i])).collect::<Vec<_>>().join(" · ")
    );
    Ok(QuizResult { chosen, best: s.best, verdict: s.verdict.label().into(), ev_loss_bb: s.ev_loss_bb, ev: q.ev.clone(), freq: q.freq.clone(), why })
}

/// Drill view + feedback for logging a quiz answer into the attempt table (feeds preflop leaks).
pub fn as_record(q: &Quiz, r: &QuizResult) -> (crate::drill::DrillView, crate::drill::Feedback) {
    use crate::drill::{ActionResult, ActionView, DrillView, Feedback, Mode};
    use poker_core::describe::{Bucket, HandDesc};
    let s = scoring::score(&q.ev, &q.freq, r.chosen, q.pot_bb, None);
    let view = DrillView {
        spot_id: format!("quickfire:{}:{}bb:{}", q.table, q.stack, q.scenario),
        mode: Mode::Spot,
        stack: q.stack,
        matchup: format!("{} preflop", q.seat),
        pot_type: "preflop".into(),
        street: "preflop".into(),
        texture: String::new(),
        board: vec![],
        hero: q.seat.clone(),
        villain: String::new(),
        hero_in_position: q.seat == "BTN",
        hero_is_aggressor: false,
        preflop: q.situation.clone(),
        line: vec![],
        hand: q.hand.clone(),
        pot_bb: q.pot_bb,
        to_call_bb: q.to_call_bb,
        hero_stack_bb: q.stack as f64,
        villain_stack_bb: 0.0,
        actions: q.actions.iter().enumerate().map(|(i, l)| ActionView { index: i, label: l.clone(), kind: q.kinds[i].clone(), amount_bb: 0.0 }).collect(),
        node_type: q.kind.replace("pf_", "").replace('_', " "),
        situation: q.kind.clone(),
        decision: 1,
        live_solved: false,
        seats: vec![],
    };
    let fb = Feedback {
        chosen: r.chosen,
        verdict: r.verdict.clone(),
        score: s,
        actions: (0..q.actions.len()).map(|i| ActionResult { label: q.actions[i].clone(), kind: q.kinds[i].clone(), ev_bb: q.ev[i], freq_hand: q.freq[i], freq_range: 0.0 }).collect(),
        hand_desc: HandDesc { made: q.class.clone(), draw: None, bucket: Bucket::Medium },
        equity: 0.0,
        reason: r.why.clone(),
        lesson: None,
        continues: false,
    };
    (view, fb)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grades_obvious_hands() {
        let q = Quiz {
            id: 1, table: 3, stack: 50, scenario: "open:BTN".into(), situation: String::new(), kind: "pf_first_in".into(), seat: "BTN".into(),
            hand: vec![], class: "AA".into(), actions: vec!["Fold".into(), "Raise".into()], kinds: vec!["fold".into(), "raise".into()],
            pot_bb: 1.5, to_call_bb: 0.0, ev: vec![0.0, 1.9], freq: vec![0.0, 1.0],
        };
        assert_eq!(grade(&q, 1).unwrap().verdict, "Best");
        let r = grade(&q, 0).unwrap();
        assert_eq!(r.verdict, "Blunder");
        assert!((r.ev_loss_bb - 1.9).abs() < 1e-9);
    }
}
