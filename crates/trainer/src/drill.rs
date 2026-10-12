//! Builds drills from solved spots and grades answers.

use crate::reason;
use crate::scoring::{self, Score};
use crate::spots::{self, SpotInfo};
use anyhow::{anyhow, bail, Result};
use poker_core::card::{card_to_string, Card};
use poker_core::describe::{describe, HandDesc};
use poker_core::eval::{evaluate, mask_of};
use poker_core::flops::label as texture_label;
use poker_core::rng::Rng;
use postflop_solver::{load_data_from_file, Action, PostFlopGame};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const BB: f64 = solver_pfs::BB;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Filters {
    #[serde(default)]
    pub stacks: Vec<u32>,
    #[serde(default)]
    pub matchups: Vec<String>,
    #[serde(default)]
    pub pot_types: Vec<String>,
    /// "flop", "turn"
    #[serde(default)]
    pub streets: Vec<String>,
    /// substring of the texture label, e.g. "A-high", "monotone", "paired"
    #[serde(default)]
    pub textures: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionView {
    pub index: usize,
    pub label: String,
    pub kind: String,
    pub amount_bb: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrillView {
    pub spot_id: String,
    pub stack: u32,
    pub matchup: String,
    pub pot_type: String,
    pub street: String,
    pub texture: String,
    pub board: Vec<String>,
    pub hero: String,
    pub villain: String,
    pub hero_in_position: bool,
    pub preflop: String,
    pub line: Vec<String>,
    pub hand: Vec<String>,
    pub pot_bb: f64,
    pub to_call_bb: f64,
    pub hero_stack_bb: f64,
    pub villain_stack_bb: f64,
    pub actions: Vec<ActionView>,
    /// "first to act", "checked to", "facing a bet", "facing a raise"
    pub node_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionResult {
    pub label: String,
    pub ev_bb: f64,
    /// solver frequency with this exact hand
    pub freq_hand: f64,
    /// solver frequency across the whole range at this spot
    pub freq_range: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Feedback {
    pub chosen: usize,
    pub score: Score,
    pub verdict: String,
    pub actions: Vec<ActionResult>,
    pub hand_desc: HandDesc,
    pub equity: f64,
    pub reason: String,
}

struct Current {
    spot: SpotInfo,
    history: Vec<usize>,
    hero: usize,
    hand_idx: usize,
    view: DrillView,
}

pub struct Engine {
    data: PathBuf,
    spots: Vec<SpotInfo>,
    cache: Vec<(String, PostFlopGame)>,
    current: Option<Current>,
    rng: Rng,
}

fn pos_name(spot: &SpotInfo, player: usize) -> String {
    if player == 0 { spot.oop.clone() } else { spot.ip.clone() }
}

fn chips_bb(x: i32) -> f64 {
    x as f64 / BB
}

fn fmt_bb(x: f64) -> String {
    let r = (x * 10.0).round() / 10.0;
    if (r - r.round()).abs() < 1e-9 { format!("{}bb", r as i64) } else { format!("{r:.1}bb") }
}

impl Engine {
    pub fn new(data: &Path, seed: u64) -> Self {
        Engine { data: data.to_path_buf(), spots: spots::load(data), cache: Vec::new(), current: None, rng: Rng::new(seed) }
    }

    pub fn refresh_spots(&mut self) {
        self.spots = spots::load(&self.data);
    }

    pub fn spots(&self) -> &[SpotInfo] {
        &self.spots
    }

    fn game(&mut self, spot: &SpotInfo) -> Result<&mut PostFlopGame> {
        if let Some(i) = self.cache.iter().position(|(id, _)| *id == spot.id) {
            let g = self.cache.remove(i);
            self.cache.push(g);
        } else {
            let (game, _memo): (PostFlopGame, String) = load_data_from_file(&spot.file, None).map_err(|e| anyhow!("load {}: {e}", spot.id))?;
            self.cache.push((spot.id.clone(), game));
            if self.cache.len() > 3 {
                self.cache.remove(0);
            }
        }
        Ok(&mut self.cache.last_mut().unwrap().1)
    }

    fn matches(f: &Filters, s: &SpotInfo) -> bool {
        (f.stacks.is_empty() || f.stacks.contains(&s.stack))
            && (f.matchups.is_empty() || f.matchups.contains(&s.matchup))
            && (f.pot_types.is_empty() || f.pot_types.contains(&s.pot_type))
            && (f.textures.is_empty() || f.textures.iter().any(|t| s.texture.contains(t.as_str())))
    }

    /// New drill matching the filters. Spots are chosen in proportion to how often their
    /// flop texture comes up in real play.
    pub fn new_drill(&mut self, f: &Filters) -> Result<DrillView> {
        let pool: Vec<SpotInfo> = self.spots.iter().filter(|s| Self::matches(f, s)).cloned().collect();
        if pool.is_empty() {
            bail!("no solved spots match these filters yet");
        }
        let total: f64 = pool.iter().map(|s| s.flop_weight.max(1e-6)).sum();
        for _ in 0..40 {
            let mut x = (self.rng.below(1 << 30) as f64 / (1u64 << 30) as f64) * total;
            let spot = pool.iter().find(|s| { x -= s.flop_weight.max(1e-6); x <= 0.0 }).unwrap_or(&pool[0]).clone();
            let street = if f.streets.is_empty() {
                if self.rng.below(100) < 60 { "flop" } else { "turn" }
            } else {
                f.streets[self.rng.below(f.streets.len() as u64) as usize].as_str()
            }
            .to_string();
            if let Some(v) = self.try_build(&spot, &street)? {
                return Ok(v);
            }
        }
        bail!("couldn't find a decision for these filters; try a wider selection")
    }

    fn try_build(&mut self, spot: &SpotInfo, street: &str) -> Result<Option<DrillView>> {
        let target_cards = if street == "turn" { 4 } else { 3 };
        let mut rng = Rng::new(self.rng.next_u64());
        let spot_c = spot.clone();
        let game = self.game(&spot_c)?;
        game.back_to_root();
        game.cache_normalized_weights();
        let mut line = Vec::new();
        loop {
            if game.is_terminal_node() {
                return Ok(None);
            }
            if game.is_chance_node() {
                if game.current_board().len() >= 4 {
                    return Ok(None); // rivers are not stored
                }
                let possible = game.possible_cards();
                let cards: Vec<u8> = (0..52u8).filter(|c| possible >> c & 1 == 1).collect();
                let c = cards[rng.below(cards.len() as u64) as usize];
                game.play(c as usize);
                game.cache_normalized_weights();
                line.push(format!("Turn {}", card_to_string(c)));
                continue;
            }
            let player = game.current_player();
            let on_street = game.current_board().len() == target_cards;
            // stop here with some probability once on the target street, so drills cover both
            // first decisions and spots facing bets
            let actions = game.available_actions();
            if on_street && rng.below(100) < 55 {
                break;
            }
            let strat = game.strategy();
            let w = game.normalized_weights(player).to_vec();
            let freqs = scoring::range_freqs(&strat, &w, actions.len());
            let mut x = rng.below(1 << 30) as f64 / (1u64 << 30) as f64;
            let mut a = actions.len() - 1;
            for (i, f) in freqs.iter().enumerate() {
                x -= f;
                if x <= 0.0 {
                    a = i;
                    break;
                }
            }
            if matches!(actions[a], Action::Fold) {
                // a fold ends the hand; prefer continuing lines so we reach a decision
                a = (0..actions.len()).filter(|&i| !matches!(actions[i], Action::Fold)).max_by(|&p, &q| freqs[p].partial_cmp(&freqs[q]).unwrap()).unwrap_or(a);
            }
            let before = game.total_bet_amount();
            line.push(describe_action(&pos_name(&spot_c, player), actions[a], before, player, spot_c.pot_bb));
            game.play(a);
            game.cache_normalized_weights();
            if game.current_board().len() > target_cards {
                return Ok(None);
            }
        }
        let hero = game.current_player();
        let weights = game.normalized_weights(hero).to_vec();
        let total: f64 = weights.iter().map(|&w| w as f64).sum();
        if total <= 0.0 {
            return Ok(None);
        }
        let mut x = rng.below(1 << 30) as f64 / (1u64 << 30) as f64 * total;
        let mut hand_idx = 0;
        for (i, &w) in weights.iter().enumerate() {
            x -= w as f64;
            if x <= 0.0 {
                hand_idx = i;
                break;
            }
        }
        let (c1, c2) = game.private_cards(hero)[hand_idx];
        let tb = game.total_bet_amount();
        let pot = spot_c.pot_bb + chips_bb(tb[0] + tb[1]);
        let to_call = chips_bb((tb[1 - hero] - tb[hero]).max(0));
        let actions: Vec<ActionView> = game
            .available_actions()
            .iter()
            .enumerate()
            .map(|(i, &a)| action_view(i, a, tb, hero, pot))
            .collect();
        let node_type = if to_call > 0.0 {
            if line.iter().rev().take_while(|l| !l.starts_with("Turn")).any(|l| l.contains("raises")) { "facing a raise" } else { "facing a bet" }
        } else if line.last().is_some_and(|l| l.ends_with("checks")) {
            "checked to"
        } else {
            "first to act"
        };
        let board: Vec<Card> = game.current_board();
        let history = game.history().to_vec();
        let view = DrillView {
            spot_id: spot_c.id.clone(),
            stack: spot_c.stack,
            matchup: spot_c.matchup.clone(),
            pot_type: spot_c.pot_type.clone(),
            street: if board.len() == 4 { "turn" } else { "flop" }.into(),
            texture: texture_label([board[0], board[1], board[2]]),
            board: board.iter().map(|&c| card_to_string(c)).collect(),
            hero: pos_name(&spot_c, hero),
            villain: pos_name(&spot_c, 1 - hero),
            hero_in_position: hero == 1,
            preflop: preflop_line(&spot_c),
            line,
            hand: vec![card_to_string(c1.max(c2)), card_to_string(c1.min(c2))],
            pot_bb: pot,
            to_call_bb: to_call,
            hero_stack_bb: spot_c.stack_bb - chips_bb(tb[hero]),
            villain_stack_bb: spot_c.stack_bb - chips_bb(tb[1 - hero]),
            actions,
            node_type: node_type.into(),
        };
        self.current = Some(Current { spot: spot_c, history, hero, hand_idx, view: view.clone() });
        Ok(Some(view))
    }

    pub fn current(&self) -> Option<&DrillView> {
        self.current.as_ref().map(|c| &c.view)
    }

    pub fn answer(&mut self, chosen: usize) -> Result<Feedback> {
        let cur = self.current.take().ok_or_else(|| anyhow!("no drill in progress"))?;
        let (spot, history, hero, h) = (cur.spot.clone(), cur.history.clone(), cur.hero, cur.hand_idx);
        let game = self.game(&spot)?;
        game.apply_history(&history);
        game.cache_normalized_weights();
        let n_actions = game.available_actions().len();
        if chosen >= n_actions {
            self.current = Some(cur);
            bail!("invalid action");
        }
        let nh = game.private_cards(hero).len();
        let strat = game.strategy();
        let evd = game.expected_values_detail(hero);
        let weights = game.normalized_weights(hero).to_vec();
        let villain_w = game.normalized_weights(1 - hero).to_vec();
        let villain_cards = game.private_cards(1 - hero).to_vec();
        let (c1, c2) = game.private_cards(hero)[h];
        let board = game.current_board();
        let range = scoring::range_freqs(&strat, &weights, n_actions);
        let ev_bb: Vec<f64> = (0..n_actions).map(|a| evd[a * nh + h] as f64 / BB).collect();
        let freq: Vec<f64> = (0..n_actions).map(|a| strat[a * nh + h] as f64).collect();
        let score = scoring::score(&ev_bb, &freq, chosen, cur.view.pot_bb);
        let equity = mc_equity([c1, c2], &board, &villain_cards, &villain_w, 4000, &mut self.rng);
        let hand_desc = describe([c1, c2], &board);
        let actions: Vec<ActionResult> = (0..n_actions)
            .map(|a| ActionResult { label: cur.view.actions[a].label.clone(), ev_bb: ev_bb[a], freq_hand: freq[a], freq_range: range[a] })
            .collect();
        let reason = reason::explain(&cur.view, &actions, &score, chosen, &hand_desc, equity, &spot);
        Ok(Feedback { chosen, verdict: score.verdict.label().into(), score, actions, hand_desc, equity, reason })
    }
}

fn preflop_line(s: &SpotInfo) -> String {
    let mut it = s.matchup.split(" vs ");
    let (o, r) = (it.next().unwrap_or(""), it.next().unwrap_or(""));
    if s.pot_type == "srp" {
        format!("{o} raises, {r} calls")
    } else {
        format!("{o} raises, {r} 3-bets, {o} calls")
    }
}

fn describe_action(who: &str, a: Action, before: [i32; 2], player: usize, _pot0: f64) -> String {
    match a {
        Action::Fold => format!("{who} folds"),
        Action::Check => format!("{who} checks"),
        Action::Call => format!("{who} calls {}", fmt_bb(chips_bb(before[1 - player] - before[player]))),
        Action::Bet(x) => format!("{who} bets {}", fmt_bb(chips_bb(x))),
        Action::Raise(x) => format!("{who} raises to {}", fmt_bb(chips_bb(x))),
        Action::AllIn(x) => format!("{who} goes all-in ({})", fmt_bb(chips_bb(x))),
        _ => String::new(),
    }
}

fn action_view(i: usize, a: Action, tb: [i32; 2], hero: usize, pot: f64) -> ActionView {
    let to_call = chips_bb((tb[1 - hero] - tb[hero]).max(0));
    let (label, kind, amt) = match a {
        Action::Fold => ("Fold".to_string(), "fold", 0.0),
        Action::Check => ("Check".to_string(), "check", 0.0),
        Action::Call => (format!("Call {}", fmt_bb(to_call)), "call", to_call),
        Action::Bet(x) => {
            let b = chips_bb(x);
            (format!("Bet {} ({:.0}% pot)", fmt_bb(b), 100.0 * b / pot), "bet", b)
        }
        Action::Raise(x) => (format!("Raise to {}", fmt_bb(chips_bb(x))), "raise", chips_bb(x)),
        Action::AllIn(x) => (format!("All-in {}", fmt_bb(chips_bb(x))), "allin", chips_bb(x)),
        _ => ("?".to_string(), "other", 0.0),
    };
    ActionView { index: i, label, kind: kind.into(), amount_bb: amt }
}

/// Monte Carlo equity of `hole` against a weighted range on `board`.
pub fn mc_equity(hole: [Card; 2], board: &[Card], villain: &[(Card, Card)], w: &[f32], samples: u32, rng: &mut Rng) -> f64 {
    let dead = mask_of(&[hole[0], hole[1]]) | mask_of(board);
    let live: Vec<(usize, f64)> = villain
        .iter()
        .enumerate()
        .filter(|(i, (a, b))| w[*i] > 0.0 && dead >> a & 1 == 0 && dead >> b & 1 == 0)
        .map(|(i, _)| (i, w[i] as f64))
        .collect();
    let total: f64 = live.iter().map(|x| x.1).sum();
    if total <= 0.0 {
        return 0.5;
    }
    let mut score = 0.0;
    for _ in 0..samples {
        let mut x = rng.below(1 << 30) as f64 / (1u64 << 30) as f64 * total;
        let mut vi = live[0].0;
        for &(i, wt) in &live {
            x -= wt;
            if x <= 0.0 {
                vi = i;
                break;
            }
        }
        let (va, vb) = villain[vi];
        let used = dead | 1u64 << va | 1u64 << vb;
        let mut b = mask_of(board);
        let mut u = used;
        while b.count_ones() < 5 {
            let c = rng.below(52);
            if u >> c & 1 == 0 {
                b |= 1 << c;
                u |= 1 << c;
            }
        }
        let h = evaluate(b | mask_of(&hole));
        let v = evaluate(b | 1u64 << va | 1u64 << vb);
        score += if h > v { 1.0 } else if h == v { 0.5 } else { 0.0 };
    }
    score / samples as f64
}
