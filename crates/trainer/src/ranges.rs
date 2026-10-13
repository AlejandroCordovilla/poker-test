//! Range Builder tasks: the full strategy of one node, per hand class, for the player to
//! paint and be graded against (preflop from the preflop solver, flop from a solved spot).

use crate::drill::Engine;
use crate::import::Analyzer;
use anyhow::{anyhow, bail, Result};
use poker_core::card::card_to_string;
use poker_core::hand169::{self, N};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassInfo {
    pub class: usize,
    pub name: String,
    /// share of the range at this node (0 = not in the range)
    pub weight: f64,
    /// solver frequency of each action
    pub freq: Vec<f64>,
    /// EV of each action in bb
    pub ev: Vec<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RangeTask {
    pub title: String,
    pub description: String,
    pub actions: Vec<String>,
    pub kinds: Vec<String>,
    pub pot_bb: f64,
    pub board: Vec<String>,
    pub classes: Vec<ClassInfo>,
}

/// Preflop task. `scenario`: "open:BTN", "vs_open:BTN:BB" (BB facing a BTN open),
/// "vs_3bet:BTN:BB" (BTN facing a BB 3-bet).
pub fn preflop_task(an: &mut Analyzer, table: usize, stack: u32, scenario: &str) -> Result<RangeTask> {
    let parts: Vec<&str> = scenario.split(':').collect();
    let s = an.solver_pub(table, stack)?;
    let pos = s.cfg.positions();
    let (path, title, pot): (Vec<&str>, String, f64) = match parts.as_slice() {
        ["open", p] => {
            let i = pos.iter().position(|x| x == p).ok_or_else(|| anyhow!("unknown seat {p}"))?;
            (vec!["fold"; i], format!("{p}: first in"), 1.5)
        }
        ["vs_open", o, r] => (s.path(o, r, &[]).map_err(|e| anyhow!(e))?, format!("{r} facing a {o} open"), 4.0),
        ["vs_3bet", o, r] => (s.path(o, r, &["3bet"]).map_err(|e| anyhow!(e))?, format!("{o} facing a {r} 3-bet"), 12.0),
        _ => bail!("unknown scenario {scenario}"),
    };
    let (player, names, evs) = s.action_evs(&path).map_err(|e| anyhow!(e))?;
    let (_, _, avg) = s.strategy_at(&path).map_err(|e| anyhow!(e))?;
    let (_, reach) = s.walk(&path).map_err(|e| anyhow!(e))?;
    let k = names.len();
    let classes = (0..N)
        .map(|c| ClassInfo {
            class: c,
            name: hand169::name(c),
            weight: reach[player][c] * hand169::combo_count(c) / 1326.0,
            freq: (0..k).map(|a| avg[a * N + c]).collect(),
            ev: (0..k).map(|a| evs[a][c]).collect(),
        })
        .collect();
    Ok(RangeTask {
        title: format!("{title} · {}-handed · {stack}bb", table),
        description: "Paint what you'd do with every hand, then check it against the solver. Grey hands aren't in your range here.".into(),
        kinds: names.iter().map(|n| match n.as_str() { "fold" => "fold", "call" => "call", "jam" => "allin", _ => "raise" }.to_string()).collect(),
        actions: names.iter().map(|n| match n.as_str() { "fold" => "Fold", "call" => "Call", "open" => "Raise", "3bet" => "3-bet", "4bet" => "4-bet", "jam" => "All-in", x => x }.to_string()).collect(),
        pot_bb: pot,
        board: vec![],
        classes,
    })
}

/// Flop task from a solved spot: the first decision of the out-of-position player
/// ("oop"), or the in-position player after a check ("ip").
pub fn flop_task(engine: &mut Engine, spot_id: &str, who: &str) -> Result<RangeTask> {
    let spot = engine.spots().iter().find(|s| s.id == spot_id).cloned().ok_or_else(|| anyhow!("spot not solved"))?;
    let history: Vec<usize> = if who == "ip" { vec![0] } else { vec![] }; // action 0 at the root is Check
    let (game, view) = engine.game_at(&spot, &history)?;
    let p = game.current_player();
    let nh = game.private_cards(p).len();
    let actions = game.available_actions();
    let k = actions.len();
    let strat = game.strategy();
    let evd = game.expected_values_detail(p);
    let w = game.normalized_weights(p).to_vec();
    let mut wsum = vec![0.0; N];
    let mut fsum = vec![vec![0.0; k]; N];
    let mut esum = vec![vec![0.0; k]; N];
    for (i, &(a, b)) in game.private_cards(p).iter().enumerate() {
        let c = hand169::class_of(a, b);
        let wi = w[i] as f64;
        wsum[c] += wi;
        for x in 0..k {
            fsum[c][x] += wi * strat[x * nh + i] as f64;
            esum[c][x] += wi * evd[x * nh + i] as f64 / solver_pfs::BB;
        }
    }
    let total: f64 = wsum.iter().sum::<f64>().max(1e-12);
    let classes = (0..N)
        .map(|c| ClassInfo {
            class: c,
            name: hand169::name(c),
            weight: wsum[c] / total,
            freq: (0..k).map(|x| if wsum[c] > 0.0 { fsum[c][x] / wsum[c] } else { 0.0 }).collect(),
            ev: (0..k).map(|x| if wsum[c] > 0.0 { esum[c][x] / wsum[c] } else { 0.0 }).collect(),
        })
        .collect();
    Ok(RangeTask {
        title: format!("{} · {} {} · {}bb", view.hero, spot.matchup, if spot.pot_type == "srp" { "single-raised" } else { "3-bet pot" }, spot.stack),
        description: format!("{} on {}. {}", if who == "ip" { "Checked to you" } else { "You act first" }, view.texture, "Paint your strategy for every hand in your range."),
        actions: view.actions.iter().map(|a| a.label.clone()).collect(),
        kinds: view.actions.iter().map(|a| a.kind.clone()).collect(),
        pot_bb: view.pot_bb,
        board: game.current_board().iter().map(|&c| card_to_string(c)).collect(),
        classes,
    })
}
