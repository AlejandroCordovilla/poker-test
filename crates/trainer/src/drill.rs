//! Builds drills from solved spots, grades answers, and plays hands forward.
//!
//! Modes:
//! - Spot: one decision anywhere on the flop or turn.
//! - Street: every decision of one street (flop or turn), villain auto-plays its own hand.
//! - Hand: flop to river. River spots are solved on the spot from the stored turn node.

use crate::reason;
use crate::scoring::{self, Score};
use crate::spots::{self, SpotInfo};
use anyhow::{anyhow, bail, Result};
use poker_core::card::{card_to_string, Card};
use poker_core::describe::{describe, HandDesc};
use poker_core::eval::{evaluate, mask_of};
use poker_core::flops::label as texture_label;
use poker_core::rng::Rng;
use postflop_solver::{
    load_data_from_file, solve, Action, ActionTree, BetSizeOptions, BoardState, CardConfig, PostFlopGame, Range, TreeConfig,
};
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
    /// "Nuts", "Strong", "Medium", "Weak", "Draw", "Air"
    #[serde(default)]
    pub hand_buckets: Vec<String>,
    /// only decisions where the two best actions are within 3% of the pot
    #[serde(default)]
    pub close_only: bool,
    /// "IP", "OOP"
    #[serde(default)]
    pub positions: Vec<String>,
    /// "" (any), "facing" (respond to a 75%+ bet), "making" (spots where a 75%+ bet is an option)
    #[serde(default)]
    pub big_bets: String,
}

/// A bet of at least this fraction of the pot counts as "big".
pub const BIG: f64 = 0.74;

fn is_big(a: Action, pot_chips: i32, tb: [i32; 2], p: usize) -> bool {
    match a {
        // raise/all-in amounts are street totals: compare the extra chips to the pot
        Action::Bet(x) | Action::Raise(x) | Action::AllIn(x) => (x - tb[p].min(x)).max(0) as f64 >= BIG * pot_chips as f64 || matches!(a, Action::AllIn(_)),
        _ => false,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Spot,
    Street,
    Hand,
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
    pub mode: Mode,
    pub stack: u32,
    pub matchup: String,
    pub pot_type: String,
    pub street: String,
    pub texture: String,
    pub board: Vec<String>,
    pub hero: String,
    pub villain: String,
    pub hero_in_position: bool,
    pub hero_is_aggressor: bool,
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
    /// leak-stat situation: cbet, barrel, river_barrel, probe, vs_cbet, vs_bet, river_vs_bet, facing_raise, other
    pub situation: String,
    /// decision number within a Street/Hand drill (1-based)
    pub decision: usize,
    /// the river was solved on the spot for this decision
    pub live_solved: bool,
    /// every seat at the table, PokerNow style
    #[serde(default)]
    pub seats: Vec<SeatView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeatView {
    pub pos: String,
    /// "You", "Villain", or "" for players out of the hand
    pub who: String,
    pub dealer: bool,
    pub in_hand: bool,
    pub stack_bb: f64,
    /// what they did preflop: "Raise 2.5bb", "Call", "3-bet", "Fold"
    pub preflop: String,
    /// their latest action on this street, e.g. "Bet 1.8bb", "Check" ("" if none yet)
    pub action: String,
    /// "bet", "raise", "call", "check", "fold", "allin" for coloring
    pub action_kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionResult {
    pub label: String,
    pub kind: String,
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
    /// Karpathy-style breakdown (verdict, pain, idea, analogy, proof, rule)
    pub lesson: Option<reason::Lesson>,
    /// Street/Hand mode: the hand goes on after this decision (call `continue_hand`)
    pub continues: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandResult {
    pub summary: String,
    pub villain_hand: Option<Vec<String>>,
    pub board: Vec<String>,
    pub pot_bb: f64,
    pub hero_won: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Step {
    /// actions taken since the hero's last decision (villain moves, new cards)
    pub events: Vec<String>,
    pub next: Option<DrillView>,
    pub result: Option<HandResult>,
}

/// One hero decision in a hand, kept so the player can go back to it or replay from it.
#[derive(Clone)]
struct Snapshot {
    stored_history: Vec<usize>,
    /// river card and river-subgame history, for decisions on the river
    river: Option<(Card, Vec<usize>)>,
    line: Vec<String>,
    events: Vec<Ev>,
    decision: usize,
    view: DrillView,
    feedback: Option<Feedback>,
}

/// A past decision for the review screen.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewItem {
    pub index: usize,
    pub view: DrillView,
    pub feedback: Option<Feedback>,
}

#[derive(Clone, Copy)]
struct Ev {
    board_len: usize,
    player: usize,
    kind: char,
}

struct Current {
    spot: SpotInfo,
    mode: Mode,
    river: Option<Box<PostFlopGame>>,
    hero: usize,
    hero_cards: (Card, Card),
    villain_cards: Option<(Card, Card)>,
    start_len: usize,
    line: Vec<String>,
    events: Vec<Ev>,
    view: DrillView,
    chosen: Option<usize>,
    decision: usize,
    stored_history: Vec<usize>,
    snapshots: Vec<Snapshot>,
    /// cards dealt so far, by board length before the deal (3 = turn, 4 = river): replays reuse them
    dealt: std::collections::HashMap<usize, Card>,
    /// villain choices by (board length, history): replays repeat them on the same line
    villain_moves: std::collections::HashMap<(usize, Vec<usize>), usize>,
}

impl Current {
    fn snap(&mut self, game: &PostFlopGame) {
        let river = if game.current_board().len() == 5 { self.dealt.get(&4).map(|&c| (c, game.history().to_vec())) } else { None };
        self.snapshots.push(Snapshot {
            stored_history: self.stored_history.clone(),
            river,
            line: self.line.clone(),
            events: self.events.clone(),
            decision: self.decision,
            view: self.view.clone(),
            feedback: None,
        });
    }
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

fn unit(rng: &mut Rng) -> f64 {
    rng.below(1 << 30) as f64 / (1u64 << 30) as f64
}

fn sample(weights: &[f64], rng: &mut Rng) -> Option<usize> {
    let total: f64 = weights.iter().sum();
    if total <= 0.0 {
        return None;
    }
    let mut x = unit(rng) * total;
    for (i, &w) in weights.iter().enumerate() {
        x -= w;
        if x <= 0.0 && w > 0.0 {
            return Some(i);
        }
    }
    weights.iter().rposition(|&w| w > 0.0)
}

fn hand_index(game: &PostFlopGame, p: usize, cards: (Card, Card)) -> Option<usize> {
    game.private_cards(p).iter().position(|&(a, b)| (a, b) == cards || (b, a) == cards)
}

fn kind_char(a: Action) -> char {
    match a {
        Action::Fold => 'f',
        Action::Check => 'x',
        Action::Call => 'c',
        Action::Bet(_) => 'b',
        Action::Raise(_) | Action::AllIn(_) => 'r',
        _ => '?',
    }
}

fn kind_name(a: Action) -> &'static str {
    match a {
        Action::Fold => "fold",
        Action::Check => "check",
        Action::Call => "call",
        Action::Bet(_) => "bet",
        Action::Raise(_) => "raise",
        Action::AllIn(_) => "allin",
        _ => "other",
    }
}

fn street_name(len: usize) -> &'static str {
    match len {
        3 => "flop",
        4 => "turn",
        _ => "river",
    }
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

    fn load(&mut self, spot: &SpotInfo) -> Result<()> {
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
        Ok(())
    }

    fn stored(&mut self) -> &mut PostFlopGame {
        &mut self.cache.last_mut().unwrap().1
    }

    fn matches(f: &Filters, s: &SpotInfo) -> bool {
        (f.stacks.is_empty() || f.stacks.contains(&s.stack))
            && (f.matchups.is_empty() || f.matchups.contains(&s.matchup))
            && (f.pot_types.is_empty() || f.pot_types.contains(&s.pot_type))
            && (f.textures.is_empty() || f.textures.iter().any(|t| s.texture.contains(t.as_str())))
    }

    /// New drill. Spots are chosen in proportion to how often their flop type comes up.
    pub fn new_drill(&mut self, f: &Filters, mode: Mode) -> Result<DrillView> {
        let pool: Vec<SpotInfo> = self.spots.iter().filter(|s| Self::matches(f, s)).cloned().collect();
        if pool.is_empty() {
            bail!("no solved spots match these filters yet");
        }
        let weights: Vec<f64> = pool.iter().map(|s| s.flop_weight.max(1e-6)).collect();
        for _ in 0..60 {
            let spot = pool[sample(&weights, &mut self.rng).unwrap_or(0)].clone();
            let street_len = match mode {
                Mode::Hand => 3,
                _ if f.streets.is_empty() => {
                    if self.rng.below(100) < 60 { 3 } else { 4 }
                }
                _ => {
                    if f.streets[self.rng.below(f.streets.len() as u64) as usize] == "turn" { 4 } else { 3 }
                }
            };
            self.load(&spot)?;
            let built = match mode {
                Mode::Spot => self.build_spot(&spot, street_len, f)?,
                _ => self.build_played(&spot, street_len, mode, f)?,
            };
            if let Some(v) = built {
                return Ok(v);
            }
        }
        bail!("couldn't find a decision for these filters; try a wider selection")
    }

    /// Spot mode: walk the solver's range-level strategy to a decision, then deal a hand.
    fn build_spot(&mut self, spot: &SpotInfo, target: usize, f: &Filters) -> Result<Option<DrillView>> {
        let mut rng = Rng::new(self.rng.next_u64());
        let game = self.stored();
        game.back_to_root();
        game.cache_normalized_weights();
        let mut line = Vec::new();
        let mut events = Vec::new();
        let mut big_last = false;
        loop {
            if game.is_terminal_node() {
                return Ok(None);
            }
            if game.is_chance_node() {
                if game.current_board().len() >= 4 {
                    return Ok(None);
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
            let actions = game.available_actions();
            let on_target = game.current_board().len() == target;
            let tb = game.total_bet_amount();
            let pot_chips = game.tree_config().starting_pot + tb[0] + tb[1];
            let strat = game.strategy();
            let w = game.normalized_weights(player).to_vec();
            let freqs = scoring::range_freqs(&strat, &w, actions.len());
            let big_opts: Vec<usize> = (0..actions.len()).filter(|&i| is_big(actions[i], pot_chips, tb, player) && freqs[i] >= 0.01).collect();
            let facing = tb[1 - player] > tb[player];
            match f.big_bets.as_str() {
                "facing" if on_target => {
                    if facing && events.last().is_some_and(|e: &Ev| e.kind == 'b' || e.kind == 'r') && big_last {
                        break;
                    }
                    if !facing && !big_opts.is_empty() {
                        // force a big bet so the next player faces it
                        let a = *big_opts.iter().max_by_key(|&&i| match actions[i] { Action::Bet(x) | Action::Raise(x) | Action::AllIn(x) => x, _ => 0 }).unwrap();
                        let before = game.total_bet_amount();
                        events.push(Ev { board_len: game.current_board().len(), player, kind: kind_char(actions[a]) });
                        line.push(describe_action(&pos_name(spot, player), actions[a], before, player));
                        game.play(a);
                        game.cache_normalized_weights();
                        big_last = true;
                        continue;
                    }
                }
                "making" if on_target => {
                    if !facing && !big_opts.is_empty() && rng.below(100) < 70 {
                        break;
                    }
                }
                _ => {
                    if on_target && rng.below(100) < 55 {
                        break;
                    }
                }
            }
            let mut a = sample(&freqs, &mut rng).unwrap_or(0);
            if matches!(actions[a], Action::Fold) {
                a = (0..actions.len()).filter(|&i| !matches!(actions[i], Action::Fold)).max_by(|&p, &q| freqs[p].partial_cmp(&freqs[q]).unwrap()).unwrap_or(a);
            }
            big_last = is_big(actions[a], pot_chips, tb, player);
            let before = game.total_bet_amount();
            events.push(Ev { board_len: game.current_board().len(), player, kind: kind_char(actions[a]) });
            line.push(describe_action(&pos_name(spot, player), actions[a], before, player));
            game.play(a);
            game.cache_normalized_weights();
            if game.current_board().len() > target {
                return Ok(None);
            }
        }
        let hero = game.current_player();
        if !position_ok(f, hero) {
            return Ok(None);
        }
        let mut weights: Vec<f64> = game.normalized_weights(hero).iter().map(|&w| w as f64).collect();
        let tb = game.total_bet_amount();
        let pot = chips_bb(game.tree_config().starting_pot + tb[0] + tb[1]);
        if f.big_bets == "making" {
            let acts = game.available_actions();
            let pot_chips = game.tree_config().starting_pot + tb[0] + tb[1];
            let st = game.strategy();
            let n = weights.len();
            for (h, w) in weights.iter_mut().enumerate() {
                let big: f64 = (0..acts.len()).filter(|&a| is_big(acts[a], pot_chips, tb, hero)).map(|a| st[a * n + h] as f64).sum();
                *w *= 0.4 + big;
            }
        }
        if f.big_bets == "facing" {
            // favor hands where folding vs continuing is a real decision (mixed or close)
            let na = game.available_actions().len();
            let st = game.strategy();
            let n = weights.len();
            for (h, w) in weights.iter_mut().enumerate() {
                let maxf = (0..na).map(|a| st[a * n + h] as f64).fold(0.0, f64::max);
                *w *= 0.15 + (1.0 - maxf);
            }
        }
        let board = game.current_board();
        let nh = weights.len();
        let evd = if f.close_only { game.expected_values_detail(hero) } else { Vec::new() };
        let na = game.available_actions().len();
        let mut hand = None;
        for _ in 0..80 {
            let Some(h) = sample(&weights, &mut rng) else { break };
            let (c1, c2) = game.private_cards(hero)[h];
            if !f.hand_buckets.is_empty() {
                let b = format!("{:?}", describe([c1, c2], &board).bucket);
                if !f.hand_buckets.contains(&b) {
                    continue;
                }
            }
            if f.close_only {
                let mut evs: Vec<f64> = (0..na).map(|a| evd[a * nh + h] as f64 / BB).collect();
                evs.sort_by(|a, b| b.partial_cmp(a).unwrap());
                if evs.len() < 2 || evs[0] - evs[1] > 0.03 * pot {
                    continue;
                }
            }
            hand = Some((c1, c2));
            break;
        }
        let Some(hero_cards) = hand else { return Ok(None) };
        let history = game.history().to_vec();
        let mut cur = Current {
            spot: spot.clone(),
            mode: Mode::Spot,
            river: None,
            hero,
            hero_cards,
            villain_cards: None,
            start_len: target,
            line,
            events,
            view: placeholder_view(),
            chosen: None,
            decision: 1,
            stored_history: history,
            snapshots: vec![],
            dealt: Default::default(),
            villain_moves: Default::default(),
        };
        cur.view = make_view(game, &cur, false);
        cur.snap(game);
        let v = cur.view.clone();
        self.current = Some(cur);
        Ok(Some(v))
    }

    /// Street / Hand mode: deal both hands at the flop and play them with their own strategies.
    fn build_played(&mut self, spot: &SpotInfo, target: usize, mode: Mode, f: &Filters) -> Result<Option<DrillView>> {
        let mut rng = Rng::new(self.rng.next_u64());
        let hero = rng.below(2) as usize;
        if !position_ok(f, hero) {
            return Ok(None);
        }
        let game = self.stored();
        game.back_to_root();
        game.cache_normalized_weights();
        let hw: Vec<f64> = game.normalized_weights(hero).iter().map(|&w| w as f64).collect();
        let Some(h) = sample(&hw, &mut rng) else { return Ok(None) };
        let hero_cards = game.private_cards(hero)[h];
        let vw: Vec<f64> = game
            .private_cards(1 - hero)
            .iter()
            .zip(game.normalized_weights(1 - hero))
            .map(|(&(a, b), &w)| if [a, b].iter().any(|c| *c == hero_cards.0 || *c == hero_cards.1) { 0.0 } else { w as f64 })
            .collect();
        let Some(v) = sample(&vw, &mut rng) else { return Ok(None) };
        let villain_cards = game.private_cards(1 - hero)[v];
        let mut cur = Current {
            spot: spot.clone(),
            mode,
            river: None,
            hero,
            hero_cards,
            villain_cards: Some(villain_cards),
            start_len: target,
            line: Vec::new(),
            events: Vec::new(),
            view: placeholder_view(),
            chosen: None,
            decision: 0,
            stored_history: Vec::new(),
            snapshots: vec![],
            dealt: Default::default(),
            villain_moves: Default::default(),
        };
        // auto-play both hands until the hero's first decision on the target street
        let mut out = Vec::new();
        if !matches!(advance(game, &mut cur, &mut rng, &mut out, true)?, Advance::HeroTurn) {
            return Ok(None);
        }
        if game.current_board().len() != target {
            return Ok(None);
        }
        cur.decision = 1;
        cur.stored_history = game.history().to_vec();
        cur.view = make_view(game, &cur, false);
        cur.snap(game);
        let v = cur.view.clone();
        self.current = Some(cur);
        Ok(Some(v))
    }

    pub fn current(&self) -> Option<&DrillView> {
        self.current.as_ref().map(|c| &c.view)
    }

    /// The stored game positioned at `history`, plus a view for whoever acts there.
    pub fn game_at(&mut self, spot: &SpotInfo, history: &[usize]) -> Result<(&mut PostFlopGame, DrillView)> {
        self.load(spot)?;
        let game = &mut self.cache.last_mut().unwrap().1;
        game.apply_history(history);
        game.cache_normalized_weights();
        let hero = game.current_player();
        let (a, b) = game.private_cards(hero)[0];
        let mut cur = Current {
            spot: spot.clone(),
            mode: Mode::Spot,
            river: None,
            hero,
            hero_cards: (a, b),
            villain_cards: None,
            start_len: game.current_board().len(),
            line: vec![],
            events: vec![],
            view: placeholder_view(),
            chosen: None,
            decision: 1,
            stored_history: history.to_vec(),
            snapshots: vec![],
            dealt: Default::default(),
            villain_moves: Default::default(),
        };
        cur.view = make_view(game, &cur, false);
        Ok((game, cur.view))
    }

    /// Every hero decision of the current drill so far (with its grade once answered).
    pub fn review(&self) -> Vec<ReviewItem> {
        self.current
            .as_ref()
            .map(|c| c.snapshots.iter().enumerate().map(|(i, s)| ReviewItem { index: i, view: s.view.clone(), feedback: s.feedback.clone() }).collect())
            .unwrap_or_default()
    }

    /// Goes back to decision `i` (0 = the start of the hand) with the same cards, so the
    /// player can try another line. Later decisions are dropped.
    pub fn rewind(&mut self, i: usize) -> Result<DrillView> {
        let mut cur = self.current.take().ok_or_else(|| anyhow!("no drill in progress"))?;
        if i >= cur.snapshots.len() {
            self.current = Some(cur);
            bail!("no such decision");
        }
        let snap = cur.snapshots[i].clone();
        cur.snapshots.truncate(i + 1);
        cur.snapshots[i].feedback = None;
        cur.stored_history = snap.stored_history.clone();
        cur.line = snap.line;
        cur.events = snap.events;
        cur.decision = snap.decision;
        cur.view = snap.view.clone();
        cur.chosen = None;
        cur.river = None;
        if let Err(e) = self.load(&cur.spot.clone()) {
            self.current = Some(cur);
            return Err(e);
        }
        let game = &mut self.cache.last_mut().unwrap().1;
        game.apply_history(&cur.stored_history);
        game.cache_normalized_weights();
        if let Some((card, rh)) = snap.river {
            let mut rg = river_subgame(game, card)?;
            rg.apply_history(&rh);
            rg.cache_normalized_weights();
            cur.river = Some(Box::new(rg));
        }
        let v = cur.view.clone();
        self.current = Some(cur);
        Ok(v)
    }

    /// (history JSON, hero cards) of the current decision, for logging and re-drilling.
    /// River decisions (solved live) can't be re-drilled from the stored game.
    pub fn current_context(&self) -> Option<(Option<String>, String)> {
        self.current.as_ref().map(|c| {
            let hist = if c.river.is_none() { serde_json::to_string(&c.stored_history).ok() } else { None };
            (hist, format!("{}{}", card_to_string(c.hero_cards.0), card_to_string(c.hero_cards.1)))
        })
    }

    /// Re-deal one logged spot decision: same spot, same action history, same hand.
    pub fn redrill(&mut self, spot_id: &str, history_json: &str, hero_cards: &str) -> Result<DrillView> {
        let spot = self.spots.iter().find(|s| s.id == spot_id).cloned().ok_or_else(|| anyhow!("spot {spot_id} is not solved on this machine"))?;
        let history: Vec<usize> = serde_json::from_str(history_json)?;
        let cards = poker_core::card::cards_from_str(hero_cards).map_err(|e| anyhow!(e))?;
        self.load(&spot)?;
        let game = self.stored();
        game.apply_history(&history);
        game.cache_normalized_weights();
        let hero = game.current_player();
        let mut cur = Current {
            spot,
            mode: Mode::Spot,
            river: None,
            hero,
            hero_cards: (cards[0], cards[1]),
            villain_cards: None,
            start_len: game.current_board().len(),
            line: vec![],
            events: vec![],
            view: placeholder_view(),
            chosen: None,
            decision: 1,
            stored_history: history,
            snapshots: vec![],
            dealt: Default::default(),
            villain_moves: Default::default(),
        };
        cur.line = replay_line(game, &cur);
        cur.view = make_view(game, &cur, false);
        cur.snap(game);
        let v = cur.view.clone();
        self.current = Some(cur);
        Ok(v)
    }

    fn with_game<R>(&mut self, f: impl FnOnce(&mut PostFlopGame, &mut Current, &mut Rng) -> Result<R>) -> Result<R> {
        let mut cur = self.current.take().ok_or_else(|| anyhow!("no drill in progress"))?;
        let mut rng = Rng::new(self.rng.next_u64());
        let res = if let Some(mut rg) = cur.river.take() {
            let r = f(&mut rg, &mut cur, &mut rng);
            if cur.river.is_none() {
                cur.river = Some(rg);
            }
            r
        } else {
            if let Err(e) = self.load(&cur.spot.clone()) {
                self.current = Some(cur);
                return Err(e);
            }
            let game = &mut self.cache.last_mut().unwrap().1;
            game.apply_history(&cur.stored_history.clone());
            game.cache_normalized_weights();
            f(game, &mut cur, &mut rng)
        };
        self.current = Some(cur);
        res
    }

    pub fn answer(&mut self, chosen: usize, rng_roll: Option<u32>) -> Result<Feedback> {
        self.with_game(|game, cur, rng| {
            if cur.chosen.is_some() {
                bail!("already answered");
            }
            let fb = grade(game, cur, chosen, rng_roll, rng)?;
            cur.chosen = Some(chosen);
            if let Some(sn) = cur.snapshots.last_mut() {
                sn.feedback = Some(fb.clone());
            }
            Ok(fb)
        })
    }

    /// Street/Hand mode: apply the hero's answer and play on to the next hero decision.
    pub fn continue_hand(&mut self) -> Result<Step> {
        self.with_game(|game, cur, rng| {
            let chosen = cur.chosen.ok_or_else(|| anyhow!("answer first"))?;
            if cur.mode == Mode::Spot {
                bail!("spot drills don't continue");
            }
            let mut out = Vec::new();
            let actions = game.available_actions();
            let before = game.total_bet_amount();
            let a = actions[chosen];
            out.push(describe_action("You", a, before, cur.hero));
            cur.line.push(describe_action(&pos_name(&cur.spot, cur.hero), a, before, cur.hero));
            cur.events.push(Ev { board_len: game.current_board().len(), player: cur.hero, kind: kind_char(a) });
            game.play(chosen);
            game.cache_normalized_weights();
            cur.chosen = None;
            match advance(game, cur, rng, &mut out, false)? {
                Advance::HeroTurn => {
                    cur.decision += 1;
                    let on_river = game.current_board().len() == 5;
                    if !on_river {
                        cur.stored_history = game.history().to_vec();
                    }
                    cur.view = make_view(game, cur, on_river);
                    cur.snap(game);
                    Ok(Step { events: out, next: Some(cur.view.clone()), result: None })
                }
                Advance::StreetDone => Ok(Step { events: out, next: None, result: Some(street_result(game, cur)) }),
                Advance::Terminal => Ok(Step { events: out, next: None, result: Some(finish(game, cur, rng)) }),
                Advance::NeedRiver => {
                    // solve the river subgame from this turn node, then continue in it
                    let mut rg = river_game(game, cur, rng)?;
                    let card = rg.current_board()[4];
                    out.push(format!("River {}", card_to_string(card)));
                    cur.line.push(format!("River {}", card_to_string(card)));
                    let step = match advance(&mut rg, cur, rng, &mut out, false)? {
                        Advance::HeroTurn => {
                            cur.decision += 1;
                            cur.view = make_view(&mut rg, cur, true);
                            cur.snap(&rg);
                            Step { events: out, next: Some(cur.view.clone()), result: None }
                        }
                        Advance::Terminal => Step { events: out, next: None, result: Some(finish(&mut rg, cur, rng)) },
                        _ => Step { events: out, next: None, result: Some(street_result(&mut rg, cur)) },
                    };
                    cur.river = Some(Box::new(rg));
                    Ok(step)
                }
            }
        })
    }
}

/// Rebuilds the readable action line of a stored history (for re-drills).
fn replay_line(game: &mut PostFlopGame, cur: &Current) -> Vec<String> {
    let hist = cur.stored_history.clone();
    game.back_to_root();
    let mut out = Vec::new();
    for &a in &hist {
        if game.is_chance_node() {
            out.push(format!("Turn {}", card_to_string(a as u8)));
        } else {
            let p = game.current_player();
            let acts = game.available_actions();
            out.push(describe_action(&pos_name(&cur.spot, p), acts[a], game.total_bet_amount(), p));
        }
        game.play(a);
    }
    game.cache_normalized_weights();
    out
}

/// Grades `chosen` at the current node for the hero in `cur`.
fn grade(game: &mut PostFlopGame, cur: &Current, chosen: usize, rng_roll: Option<u32>, rng: &mut Rng) -> Result<Feedback> {
    let hero = cur.hero;
    let n_actions = game.available_actions().len();
    if chosen >= n_actions {
        bail!("invalid action");
    }
    let h = hand_index(game, hero, cur.hero_cards).ok_or_else(|| anyhow!("hand not in range"))?;
    let nh = game.private_cards(hero).len();
    let strat = game.strategy();
    let evd = game.expected_values_detail(hero);
    let weights = game.normalized_weights(hero).to_vec();
    let villain_w = game.normalized_weights(1 - hero).to_vec();
    let villain_cards = game.private_cards(1 - hero).to_vec();
    let board = game.current_board();
    let range = scoring::range_freqs(&strat, &weights, n_actions);
    let ev_bb: Vec<f64> = (0..n_actions).map(|a| evd[a * nh + h] as f64 / BB).collect();
    let freq: Vec<f64> = (0..n_actions).map(|a| strat[a * nh + h] as f64).collect();
    let score = scoring::score(&ev_bb, &freq, chosen, cur.view.pot_bb, rng_roll);
    let (c1, c2) = cur.hero_cards;
    let equity = mc_equity([c1, c2], &board, &villain_cards, &villain_w, 4000, rng);
    let hand_desc = describe([c1, c2], &board);
    let actions: Vec<ActionResult> = (0..n_actions)
        .map(|a| ActionResult {
            label: cur.view.actions[a].label.clone(),
            kind: cur.view.actions[a].kind.clone(),
            ev_bb: ev_bb[a],
            freq_hand: freq[a],
            freq_range: range[a],
        })
        .collect();
    let reason = reason::explain(&cur.view, &actions, &score, chosen, &hand_desc, equity, &cur.spot);
    let lesson = Some(reason::lesson(&cur.view, &actions, &score, chosen, &hand_desc, equity, &cur.spot));
    Ok(Feedback { chosen, verdict: score.verdict.label().into(), score, actions, hand_desc, equity, reason, lesson, continues: cur.mode != Mode::Spot })
}

/// For imported hands: view + grade of a real decision at the game's current node.
/// `events`: (board length, player 0=OOP/1=IP, kind char f/x/c/b/r) of every action so far.
pub(crate) fn grade_real(
    game: &mut PostFlopGame,
    spot: &SpotInfo,
    hero: usize,
    hero_cards: (Card, Card),
    line: &[String],
    events: &[(usize, usize, char)],
    chosen: usize,
    rng: &mut Rng,
) -> Result<(DrillView, Feedback)> {
    let mut cur = Current {
        spot: spot.clone(),
        mode: Mode::Spot,
        river: None,
        hero,
        hero_cards,
        villain_cards: None,
        start_len: game.current_board().len(),
        line: line.to_vec(),
        events: events.iter().map(|&(board_len, player, kind)| Ev { board_len, player, kind }).collect(),
        view: placeholder_view(),
        chosen: None,
        decision: 1,
        stored_history: vec![],
        snapshots: vec![],
        dealt: Default::default(),
        villain_moves: Default::default(),
    };
    cur.view = make_view(game, &cur, false);
    let fb = grade(game, &cur, chosen, None, rng)?;
    Ok((cur.view, fb))
}

pub(crate) fn describe_action_pub(who: &str, a: Action, before: [i32; 2], player: usize) -> String {
    describe_action(who, a, before, player)
}

enum Advance {
    HeroTurn,
    StreetDone,
    NeedRiver,
    Terminal,
}

/// Plays villain decisions (with its own hand) and chance cards until the hero must act.
/// `auto_hero`: also auto-play the hero's hand (setup before the target street).
fn advance(game: &mut PostFlopGame, cur: &mut Current, rng: &mut Rng, out: &mut Vec<String>, auto_hero: bool) -> Result<Advance> {
    loop {
        if game.is_terminal_node() {
            return Ok(Advance::Terminal);
        }
        if game.is_chance_node() {
            let len = game.current_board().len();
            if cur.mode == Mode::Street && !auto_hero && len >= cur.start_len {
                return Ok(Advance::StreetDone);
            }
            if len >= 4 {
                return Ok(if auto_hero { Advance::Terminal } else { Advance::NeedRiver });
            }
            let (h, v) = (cur.hero_cards, cur.villain_cards.unwrap());
            let possible = game.possible_cards();
            let cards: Vec<u8> = (0..52u8).filter(|&c| possible >> c & 1 == 1 && ![h.0, h.1, v.0, v.1].contains(&c)).collect();
            let c = match cur.dealt.get(&len) {
                Some(&c) if cards.contains(&c) => c,
                _ => cards[rng.below(cards.len() as u64) as usize],
            };
            cur.dealt.insert(len, c);
            game.play(c as usize);
            game.cache_normalized_weights();
            let s = format!("Turn {}", card_to_string(c));
            out.push(s.clone());
            cur.line.push(s);
            continue;
        }
        let p = game.current_player();
        let len = game.current_board().len();
        if p == cur.hero && (!auto_hero || len == cur.start_len) {
            return Ok(Advance::HeroTurn);
        }
        let cards = if p == cur.hero { cur.hero_cards } else { cur.villain_cards.unwrap() };
        let idx = hand_index(game, p, cards).ok_or_else(|| anyhow!("hand left the range"))?;
        let nh = game.private_cards(p).len();
        let actions = game.available_actions();
        let strat = game.strategy();
        let probs: Vec<f64> = (0..actions.len()).map(|a| strat[a * nh + idx] as f64).collect();
        let key = (len, game.history().to_vec());
        let a = match cur.villain_moves.get(&key) {
            Some(&a) if a < actions.len() => a,
            _ => sample(&probs, rng).unwrap_or(0),
        };
        cur.villain_moves.insert(key, a);
        let before = game.total_bet_amount();
        let s = describe_action(&pos_name(&cur.spot, p), actions[a], before, p);
        out.push(s.clone());
        cur.line.push(s);
        cur.events.push(Ev { board_len: len, player: p, kind: kind_char(actions[a]) });
        game.play(a);
        game.cache_normalized_weights();
    }
}

fn position_ok(f: &Filters, hero: usize) -> bool {
    f.positions.is_empty() || f.positions.iter().any(|p| (p == "IP") == (hero == 1))
}

/// River subgame from the turn node that just closed: both ranges as they stand, real pot
/// and stacks, the full live river menu (33 / 75 / 100 / 150% / all-in). One street, so it
/// solves in about a second.
fn river_game(game: &mut PostFlopGame, cur: &mut Current, rng: &mut Rng) -> Result<PostFlopGame> {
    let (h, v) = (cur.hero_cards, cur.villain_cards.unwrap());
    let possible = game.possible_cards();
    let cards: Vec<u8> = (0..52u8).filter(|&c| possible >> c & 1 == 1 && ![h.0, h.1, v.0, v.1].contains(&c)).collect();
    let river = match cur.dealt.get(&4) {
        Some(&c) if cards.contains(&c) => c,
        _ => cards[rng.below(cards.len() as u64) as usize],
    };
    cur.dealt.insert(4, river);
    river_subgame(game, river)
}

/// Solves the river after `river` comes, starting from a closed turn node of `game`.
pub(crate) fn river_subgame(game: &mut PostFlopGame, river: Card) -> Result<PostFlopGame> {
    let board = game.current_board();
    let range = |p: usize| -> Result<Range> {
        let w = game.normalized_weights(p);
        let max = w.iter().cloned().fold(0.0f32, f32::max).max(1e-12);
        let ws: Vec<f32> = w.iter().map(|x| (x / max).clamp(0.0, 1.0)).collect();
        Range::from_hands_weights(game.private_cards(p), &ws).map_err(|e| anyhow!(e))
    };
    let tb = game.total_bet_amount();
    let tc = game.tree_config().clone();
    let card_config = CardConfig { range: [range(0)?, range(1)?], flop: [board[0], board[1], board[2]], turn: board[3], river };
    let sizes = BetSizeOptions::try_from((solver_api::RIVER_LIVE_BET, solver_api::RIVER_LIVE_RAISE)).map_err(|e| anyhow!(e))?;
    let tree = TreeConfig {
        initial_state: BoardState::River,
        starting_pot: tc.starting_pot + tb[0] + tb[1],
        effective_stack: tc.effective_stack - tb[0].max(tb[1]),
        rake_rate: 0.0,
        rake_cap: 0.0,
        flop_bet_sizes: [sizes.clone(), sizes.clone()],
        turn_bet_sizes: [sizes.clone(), sizes.clone()],
        river_bet_sizes: [sizes.clone(), sizes],
        turn_donk_sizes: None,
        river_donk_sizes: None,
        add_allin_threshold: 1.5,
        force_allin_threshold: 0.15,
        merging_threshold: 0.1,
    };
    let mut rg = PostFlopGame::with_config(card_config, ActionTree::new(tree).map_err(|e| anyhow!(e))?).map_err(|e| anyhow!(e))?;
    rg.allocate_memory(false);
    let target = rg.tree_config().starting_pot as f32 * 0.005;
    solve(&mut rg, 400, target, false);
    rg.cache_normalized_weights();
    Ok(rg)
}

fn street_result(game: &mut PostFlopGame, cur: &Current) -> HandResult {
    let tb = game.total_bet_amount();
    let pot = chips_bb(game.tree_config().starting_pot + tb[0] + tb[1]);
    HandResult {
        summary: format!("End of the {}. Pot is now {}.", street_name(cur.start_len), fmt_bb(pot)),
        villain_hand: None,
        board: game.current_board().iter().map(|&c| card_to_string(c)).collect(),
        pot_bb: pot,
        hero_won: None,
    }
}

fn finish(game: &mut PostFlopGame, cur: &Current, rng: &mut Rng) -> HandResult {
    let tb = game.total_bet_amount();
    let pot = chips_bb(game.tree_config().starting_pot + tb[0] + tb[1]);
    let villain = pos_name(&cur.spot, 1 - cur.hero);
    let last = cur.events.last().copied();
    let mut board = game.current_board();
    let fmt = |cs: &[Card]| cs.iter().map(|&c| card_to_string(c)).collect::<Vec<_>>();
    if let Some(e) = last.filter(|e| e.kind == 'f') {
        let hero_won = e.player != cur.hero;
        // the folded-to bet comes back to the bettor; the rest is the pot won
        let won_pot = pot - chips_bb((tb[0] - tb[1]).abs());
        let summary = if hero_won { format!("{villain} folds. You win {}.", fmt_bb(won_pot)) } else { format!("You fold. {villain} wins {}.", fmt_bb(won_pot)) };
        return HandResult { summary, villain_hand: None, board: fmt(&board), pot_bb: won_pot, hero_won: Some(hero_won) };
    }
    // showdown, running out the board after an all-in
    let (h, v) = (cur.hero_cards, cur.villain_cards.unwrap());
    let mut used = mask_of(&board) | mask_of(&[h.0, h.1, v.0, v.1]);
    while board.len() < 5 {
        let c = rng.below(52) as u8;
        if used >> c & 1 == 0 {
            used |= 1 << c;
            board.push(c);
        }
    }
    let hv = evaluate(mask_of(&board) | mask_of(&[h.0, h.1]));
    let vv = evaluate(mask_of(&board) | mask_of(&[v.0, v.1]));
    // "a flush" -> "flush" after "your" / "BTN's"
    let bare = |x: String| x.strip_prefix("a ").or_else(|| x.strip_prefix("an ")).map(String::from).unwrap_or(x);
    let vd = bare(describe([v.0, v.1], &board).made);
    let hd = bare(describe([h.0, h.1], &board).made);
    let (summary, won) = match hv.cmp(&vv) {
        std::cmp::Ordering::Greater => (format!("Showdown: your {hd} beats {villain}'s {vd}. You win {}.", fmt_bb(pot)), Some(true)),
        std::cmp::Ordering::Less => (format!("Showdown: {villain}'s {vd} beats your {hd}. {villain} wins {}.", fmt_bb(pot)), Some(false)),
        std::cmp::Ordering::Equal => (format!("Showdown: split pot ({}).", fmt_bb(pot)), None),
    };
    HandResult { summary, villain_hand: Some(fmt(&[v.0.max(v.1), v.0.min(v.1)])), board: fmt(&board), pot_bb: pot, hero_won: won }
}

fn placeholder_view() -> DrillView {
    DrillView {
        spot_id: String::new(),
        mode: Mode::Spot,
        stack: 0,
        matchup: String::new(),
        pot_type: String::new(),
        street: String::new(),
        texture: String::new(),
        board: vec![],
        hero: String::new(),
        villain: String::new(),
        hero_in_position: false,
        hero_is_aggressor: false,
        preflop: String::new(),
        line: vec![],
        hand: vec![],
        pot_bb: 0.0,
        to_call_bb: 0.0,
        hero_stack_bb: 0.0,
        villain_stack_bb: 0.0,
        actions: vec![],
        node_type: String::new(),
        situation: String::new(),
        decision: 0,
        live_solved: false,
        seats: vec![],
    }
}

fn situation(cur: &Current, len: usize, to_call: f64, raised: bool) -> String {
    let hero_pa = pos_name(&cur.spot, cur.hero) == cur.spot.preflop_aggressor;
    let on = |l: usize| cur.events.iter().filter(move |e| e.board_len == l);
    let hero_bet = |l: usize| on(l).any(|e| e.player == cur.hero && (e.kind == 'b' || e.kind == 'r'));
    let villain_bet_first = on(len).find(|e| e.kind == 'b').is_some_and(|e| e.player != cur.hero);
    if to_call > 0.0 {
        if raised {
            return "facing_raise".into();
        }
        if len == 5 {
            return "river_vs_bet".into();
        }
        if len == 3 && !hero_pa && villain_bet_first {
            return "vs_cbet".into();
        }
        return "vs_bet".into();
    }
    match (hero_pa, len) {
        (true, 3) => "cbet".into(),
        (true, 4) if hero_bet(3) => "barrel".into(),
        (true, 5) if hero_bet(4) => "river_barrel".into(),
        (false, 4) if on(3).all(|e| e.kind == 'x') => "probe".into(),
        _ => "other".into(),
    }
}

fn make_view(game: &mut PostFlopGame, cur: &Current, live: bool) -> DrillView {
    let spot = &cur.spot;
    let hero = cur.hero;
    let tb = game.total_bet_amount();
    let tc = game.tree_config().clone();
    let pot = chips_bb(tc.starting_pot + tb[0] + tb[1]);
    let to_call = chips_bb((tb[1 - hero] - tb[hero]).max(0));
    let actions: Vec<ActionView> = game.available_actions().iter().enumerate().map(|(i, &a)| action_view(i, a, tb, hero, pot)).collect();
    let board: Vec<Card> = game.current_board();
    let len = board.len();
    let this_street: Vec<&Ev> = cur.events.iter().filter(|e| e.board_len == len).collect();
    let raised = this_street.iter().any(|e| e.kind == 'r' && e.player != hero);
    let node_type = if to_call > 0.0 {
        if raised { "facing a raise" } else { "facing a bet" }
    } else if this_street.last().is_some_and(|e| e.kind == 'x') {
        "checked to"
    } else {
        "first to act"
    };
    let (c1, c2) = cur.hero_cards;
    DrillView {
        spot_id: spot.id.clone(),
        mode: cur.mode,
        stack: spot.stack,
        matchup: spot.matchup.clone(),
        pot_type: spot.pot_type.clone(),
        street: street_name(len).into(),
        texture: texture_label([board[0], board[1], board[2]]),
        board: board.iter().map(|&c| card_to_string(c)).collect(),
        hero: pos_name(spot, hero),
        villain: pos_name(spot, 1 - hero),
        hero_in_position: hero == 1,
        hero_is_aggressor: pos_name(spot, hero) == spot.preflop_aggressor,
        preflop: preflop_line(spot),
        line: cur.line.clone(),
        hand: vec![card_to_string(c1.max(c2)), card_to_string(c1.min(c2))],
        pot_bb: pot,
        to_call_bb: to_call,
        hero_stack_bb: chips_bb(tc.effective_stack - tb[hero]),
        villain_stack_bb: chips_bb(tc.effective_stack - tb[1 - hero]),
        actions,
        node_type: node_type.into(),
        situation: situation(cur, len, to_call, raised),
        decision: cur.decision.max(1),
        live_solved: live,
        seats: seats(cur, len, chips_bb(tc.effective_stack - tb[hero]), chips_bb(tc.effective_stack - tb[1 - hero])),
    }
}

/// Three-handed table: the two players in the pot plus the one who folded preflop.
fn seats(cur: &Current, board_len: usize, hero_stack: f64, villain_stack: f64) -> Vec<SeatView> {
    let spot = &cur.spot;
    let mut it = spot.matchup.split(" vs ");
    let (opener, responder) = (it.next().unwrap_or("").to_string(), it.next().unwrap_or("").to_string());
    let hero_pos = pos_name(spot, cur.hero);
    let pre = |p: &str| -> String {
        if p == opener {
            let open = if p == "SB" { 3.0 } else { 2.5 };
            if spot.pot_type == "srp" { format!("Raise {open}bb") } else { format!("Raise {open}bb, call 3-bet") }
        } else if p == responder {
            if spot.pot_type == "srp" { "Call".into() } else { "3-bet".into() }
        } else {
            "Fold".into()
        }
    };
    // last action of each position on the current street, read from the line
    let street_start = cur.line.iter().rposition(|l| l.starts_with("Turn ") || l.starts_with("River ")).map(|i| i + 1).unwrap_or(0);
    let on_street = board_len > 3 || street_start == 0;
    let last = |p: &str| -> (String, String) {
        if !on_street && board_len == 3 && street_start > 0 {
            return (String::new(), String::new());
        }
        cur.line[street_start..]
            .iter()
            .rev()
            .find_map(|l| l.strip_prefix(&format!("{p} ")).map(|rest| rest.to_string()))
            .map(|rest| {
                let k = rest.split_whitespace().next().unwrap_or("").to_string();
                let kind = match k.as_str() {
                    "bets" => "bet",
                    "raises" => "raise",
                    "calls" => "call",
                    "checks" => "check",
                    "folds" => "fold",
                    "goes" => "allin",
                    _ => "",
                };
                let label = rest
                    .replacen("bets", "Bet", 1)
                    .replacen("raises to", "Raise to", 1)
                    .replacen("calls", "Call", 1)
                    .replacen("checks", "Check", 1)
                    .replacen("folds", "Fold", 1)
                    .replacen("goes all-in", "All-in", 1);
                (label, kind.to_string())
            })
            .unwrap_or_default()
    };
    ["BTN", "SB", "BB"]
        .iter()
        .map(|&p| {
            let in_hand = p == opener || p == responder;
            let (action, action_kind) = if in_hand { last(p) } else { (String::new(), String::new()) };
            SeatView {
                pos: p.into(),
                who: if p == hero_pos { "You".into() } else if in_hand { "Villain".into() } else { String::new() },
                dealer: p == "BTN",
                in_hand,
                stack_bb: if p == hero_pos { hero_stack } else if in_hand { villain_stack } else { spot.stack as f64 - if p == "SB" { 0.5 } else if p == "BB" { 1.0 } else { 0.0 } },
                preflop: pre(p),
                action,
                action_kind,
            }
        })
        .collect()
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

fn describe_action(who: &str, a: Action, before: [i32; 2], player: usize) -> String {
    // "You check" vs "BB checks"
    let you = who == "You";
    let v = |third: &str, base: &str| if you { base.to_string() } else { third.to_string() };
    match a {
        Action::Fold => format!("{who} {}", v("folds", "fold")),
        Action::Check => format!("{who} {}", v("checks", "check")),
        Action::Call => format!("{who} {} {}", v("calls", "call"), fmt_bb(chips_bb(before[1 - player] - before[player]))),
        Action::Bet(x) => format!("{who} {} {}", v("bets", "bet"), fmt_bb(chips_bb(x))),
        Action::Raise(x) => format!("{who} {} to {}", v("raises", "raise"), fmt_bb(chips_bb(x))),
        Action::AllIn(x) => format!("{who} {} all-in ({})", v("goes", "go"), fmt_bb(chips_bb(x))),
        _ => String::new(),
    }
}

fn action_view(i: usize, a: Action, tb: [i32; 2], hero: usize, pot: f64) -> ActionView {
    let to_call = chips_bb((tb[1 - hero] - tb[hero]).max(0));
    let (label, amt) = match a {
        Action::Fold => ("Fold".to_string(), 0.0),
        Action::Check => ("Check".to_string(), 0.0),
        Action::Call => (format!("Call {}", fmt_bb(to_call)), to_call),
        Action::Bet(x) => {
            let b = chips_bb(x);
            (format!("Bet {} ({:.0}% pot)", fmt_bb(b), 100.0 * b / pot), b)
        }
        Action::Raise(x) => (format!("Raise to {}", fmt_bb(chips_bb(x))), chips_bb(x)),
        Action::AllIn(x) => (format!("All-in {}", fmt_bb(chips_bb(x))), chips_bb(x)),
        _ => ("?".to_string(), 0.0),
    };
    ActionView { index: i, label, kind: kind_name(a).into(), amount_bb: amt }
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
        let mut x = unit(rng) * total;
        let mut vi = live[0].0;
        for &(i, wt) in &live {
            x -= wt;
            if x <= 0.0 {
                vi = i;
                break;
            }
        }
        let (va, vb) = villain[vi];
        let mut b = mask_of(board);
        let mut u = dead | 1u64 << va | 1u64 << vb;
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
