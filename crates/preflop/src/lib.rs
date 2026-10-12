//! Small preflop solver for 40-50bb no-rake games (3-handed and 6-max).
//!
//! Model (documented in docs/PREFLOP.md):
//! - Actions: first in: fold / open (2.5bb, SB 3bb). Facing an open: fold / call / 3-bet / jam.
//!   Facing a 3-bet: fold / call / jam. Facing a jam: fold / call.
//! - No limps, no multiway pots: once someone calls an open or 3-bets, later players fold.
//! - Pots that reach the flop are valued with equity times a realization factor
//!   (position, pot type, and a small playability bonus for suited and connected hands),
//!   normalized so the two players' shares sum to the pot. All-ins use raw equity.
//! - Solved with vector CFR+ over the 169 hand classes, card removal only between the two
//!   players who see the flop.

use poker_core::equity::EquityTable;
use poker_core::hand169::{self, N};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug)]
pub struct Config {
    pub players: usize,
    pub stack: f64,
    pub open: f64,
    pub sb_open: f64,
    pub threebet_ip: f64,
    pub threebet_blind: f64,
    pub threebet_bb_vs_sb: f64,
    pub real_ip_srp: f64,
    pub real_oop_srp: f64,
    pub real_ip_3bp: f64,
    pub real_oop_3bp: f64,
    /// extra realization for the last preflop aggressor (range advantage, initiative)
    pub initiative: f64,
    /// offer a direct jam over an open (off by default at 35bb+, where small 3-bets are standard)
    pub jam_over_open: bool,
    /// deep stacks: facing a 3-bet the opener can 4-bet small (2.25x) instead of jamming
    pub small_4bet: bool,
}

impl Config {
    pub fn new(players: usize, stack: f64) -> Self {
        Config {
            players,
            stack,
            open: 2.5,
            sb_open: 3.0,
            threebet_ip: 3.0,
            threebet_blind: 4.0,
            threebet_bb_vs_sb: 3.0,
            real_ip_srp: 1.0,
            // deeper stacks make position worth more (more streets of betting behind)
            real_oop_srp: if stack >= 80.0 { 0.8 } else { 0.9 },
            real_ip_3bp: 1.0,
            real_oop_3bp: if stack >= 80.0 { 0.88 } else { 0.95 },
            initiative: 0.08,
            jam_over_open: stack < 35.0,
            small_4bet: stack >= 60.0,
        }
    }

    pub fn positions(&self) -> Vec<&'static str> {
        match self.players {
            3 => vec!["BTN", "SB", "BB"],
            6 => vec!["UTG", "HJ", "CO", "BTN", "SB", "BB"],
            n => panic!("unsupported table size {n}"),
        }
    }

    fn sb(&self) -> usize {
        self.players - 2
    }
    fn bb(&self) -> usize {
        self.players - 1
    }

    /// Postflop order: SB, BB, then the rest in seat order. Returns true if `a` acts after `b`.
    fn in_position(&self, a: usize, b: usize) -> bool {
        let key = |p: usize| if p >= self.sb() { p - self.sb() } else { p + 2 };
        key(a) > key(b)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum PotType {
    Srp,
    ThreeBet,
    FourBet,
}

#[derive(Debug)]
enum Kind {
    Fold { winner: usize },
    Flop { a: usize, b: usize, pot_type: PotType },
    AllIn { a: usize, b: usize },
}

#[derive(Debug)]
struct Terminal {
    kind: Kind,
    invest: Vec<f64>,
    /// for two-player terminals: a's payoff matrix pre-multiplied by P(b holds j | a holds i)
    ma: Vec<f64>,
    mb: Vec<f64>,
    cond: Vec<f64>,
}

#[derive(Debug)]
struct Decision {
    player: usize,
    actions: Vec<(String, usize)>,
    regret: Vec<f64>,
    strat_sum: Vec<f64>,
}

#[derive(Debug)]
enum Node {
    D(Decision),
    T(Terminal),
}

pub struct Solver {
    pub cfg: Config,
    nodes: Vec<Node>,
    prior: Vec<f64>,
    eqr: Vec<f64>,
    eq: EquityTable,
    iterations: u32,
}

/// Realization multiplier by hand: weak hands realize less of their equity postflop (they
/// fold too often to see showdown), strong and well-connected hands realize more.
/// `eqr` is the hand's all-in equity against a random hand.
fn playability(i: usize, eqr: f64) -> f64 {
    let mut r = (0.75 + 0.9 * (eqr - 0.5)).clamp(0.55, 1.12);
    if hand169::is_pair(i) {
        return r;
    }
    let (hi, lo) = hand169::ranks(i);
    if hand169::is_suited(i) {
        r *= 1.06;
    }
    if hi - lo <= 2 {
        r *= 1.03;
    }
    r
}

impl Solver {
    pub fn new(cfg: Config, eq: EquityTable) -> Self {
        let prior = (0..N).map(|i| hand169::combo_count(i) / 1326.0).collect();
        let eqr: Vec<f64> = (0..N)
            .map(|i| {
                let tot: f64 = (0..N).map(|j| eq.w(i, j)).sum();
                (0..N).map(|j| eq.w(i, j) * eq.eq(i, j)).sum::<f64>() / tot
            })
            .collect();
        let mut s = Solver { cfg, nodes: Vec::new(), prior, eqr, eq, iterations: 0 };
        let mut invest = vec![0.0; s.cfg.players];
        invest[s.cfg.sb()] = 0.5;
        invest[s.cfg.bb()] = 1.0;
        s.first_in(0, invest);
        s
    }

    fn push(&mut self, n: Node) -> usize {
        self.nodes.push(n);
        self.nodes.len() - 1
    }

    fn decision(&mut self, player: usize, actions: Vec<(String, usize)>) -> usize {
        let k = actions.len();
        self.push(Node::D(Decision { player, actions, regret: vec![0.0; k * N], strat_sum: vec![0.0; k * N] }))
    }

    fn terminal(&mut self, kind: Kind, invest: Vec<f64>) -> usize {
        let pot: f64 = invest.iter().sum();
        let (mut ma, mut mb, mut cond) = (Vec::new(), Vec::new(), Vec::new());
        let two = match kind {
            Kind::Flop { a, b, pot_type } => Some((a, b, Some(pot_type))),
            Kind::AllIn { a, b } => Some((a, b, None)),
            Kind::Fold { .. } => None,
        };
        if let Some((a, b, pt)) = two {
            ma = vec![0.0; N * N];
            mb = vec![0.0; N * N];
            cond = vec![0.0; N * N];
            let (ra, rb) = match pt {
                None => (1.0, 1.0),
                Some(pt) => {
                    let a_ip = self.cfg.in_position(a, b);
                    let (ip, oop) = match pt {
                        PotType::Srp => (self.cfg.real_ip_srp, self.cfg.real_oop_srp),
                        PotType::ThreeBet | PotType::FourBet => (self.cfg.real_ip_3bp, self.cfg.real_oop_3bp),
                    };
                    // `b` is always the last aggressor's opponent in Srp (a = opener) and the
                    // 3-bettor in ThreeBet (a = opener who called)
                    let (ra, rb) = if a_ip { (ip, oop) } else { (oop, ip) };
                    match pt {
                        PotType::Srp | PotType::FourBet => (ra * (1.0 + self.cfg.initiative), rb),
                        PotType::ThreeBet => (ra, rb * (1.0 + self.cfg.initiative)),
                    }
                }
            };
            for i in 0..N {
                for j in 0..N {
                    let c = self.eq.w(i, j) / (hand169::combo_count(i) * 1225.0);
                    cond[i * N + j] = c;
                    let e = self.eq.eq(i, j);
                    let share = if pt.is_none() {
                        e
                    } else {
                        let x = e * ra * playability(i, self.eqr[i]);
                        let y = (1.0 - e) * rb * playability(j, self.eqr[j]);
                        x / (x + y)
                    };
                    ma[i * N + j] = c * (share * pot - invest[a]);
                    let cb = self.eq.w(j, i) / (hand169::combo_count(j) * 1225.0);
                    mb[j * N + i] = cb * ((1.0 - share) * pot - invest[b]);
                }
            }
        }
        self.push(Node::T(Terminal { kind, invest, ma, mb, cond }))
    }

    fn first_in(&mut self, p: usize, invest: Vec<f64>) -> usize {
        if p == self.cfg.bb() {
            return self.terminal(Kind::Fold { winner: p }, invest);
        }
        let fold = self.first_in(p + 1, invest.clone());
        let mut inv = invest.clone();
        inv[p] = if p == self.cfg.sb() { self.cfg.sb_open } else { self.cfg.open };
        let open = self.facing_open(p, p + 1, inv);
        self.decision(p, vec![("fold".into(), fold), ("open".into(), open)])
    }

    fn facing_open(&mut self, o: usize, r: usize, invest: Vec<f64>) -> usize {
        if r == self.cfg.players {
            return self.terminal(Kind::Fold { winner: o }, invest);
        }
        let open = invest[o];
        let fold = self.facing_open(o, r + 1, invest.clone());
        let mut call_inv = invest.clone();
        call_inv[r] = open;
        let call = self.terminal(Kind::Flop { a: o, b: r, pot_type: PotType::Srp }, call_inv);
        let mult = if r == self.cfg.bb() && o == self.cfg.sb() {
            self.cfg.threebet_bb_vs_sb
        } else if r >= self.cfg.sb() {
            self.cfg.threebet_blind
        } else {
            self.cfg.threebet_ip
        };
        let size = (open * mult).min(self.cfg.stack);
        let mut tb_inv = invest.clone();
        tb_inv[r] = size;
        let threebet = self.facing_3bet(o, r, tb_inv);
        let mut actions = vec![("fold".into(), fold), ("call".into(), call), ("3bet".into(), threebet)];
        if self.cfg.jam_over_open {
            let mut jam_inv = invest.clone();
            jam_inv[r] = self.cfg.stack;
            let jam = self.facing_jam(o, r, jam_inv);
            actions.push(("jam".into(), jam));
        }
        self.decision(r, actions)
    }

    fn facing_3bet(&mut self, o: usize, t: usize, invest: Vec<f64>) -> usize {
        let fold = self.terminal(Kind::Fold { winner: t }, invest.clone());
        let mut call_inv = invest.clone();
        call_inv[o] = invest[t];
        let call = self.terminal(Kind::Flop { a: o, b: t, pot_type: PotType::ThreeBet }, call_inv);
        if self.cfg.small_4bet {
            let mut fb_inv = invest.clone();
            fb_inv[o] = (invest[t] * 2.25).min(self.cfg.stack);
            let fourbet = self.facing_4bet(t, o, fb_inv);
            return self.decision(o, vec![("fold".into(), fold), ("call".into(), call), ("4bet".into(), fourbet)]);
        }
        let mut jam_inv = invest.clone();
        jam_inv[o] = self.cfg.stack;
        let jam = self.facing_jam(t, o, jam_inv);
        self.decision(o, vec![("fold".into(), fold), ("call".into(), call), ("jam".into(), jam)])
    }

    /// The 3-bettor `t` faces a small 4-bet from `f`: fold, call (low-SPR pot), or jam.
    fn facing_4bet(&mut self, t: usize, f: usize, invest: Vec<f64>) -> usize {
        let fold = self.terminal(Kind::Fold { winner: f }, invest.clone());
        let mut call_inv = invest.clone();
        call_inv[t] = invest[f];
        let call = self.terminal(Kind::Flop { a: f, b: t, pot_type: PotType::FourBet }, call_inv);
        let mut jam_inv = invest.clone();
        jam_inv[t] = self.cfg.stack;
        let jam = self.facing_jam(f, t, jam_inv);
        self.decision(t, vec![("fold".into(), fold), ("call".into(), call), ("jam".into(), jam)])
    }

    /// `c` faces a jam from `j`.
    fn facing_jam(&mut self, c: usize, j: usize, invest: Vec<f64>) -> usize {
        let fold = self.terminal(Kind::Fold { winner: j }, invest.clone());
        let mut call_inv = invest.clone();
        call_inv[c] = self.cfg.stack;
        let call = self.terminal(Kind::AllIn { a: c, b: j }, call_inv);
        self.decision(c, vec![("fold".into(), fold), ("call".into(), call)])
    }

    fn root(&self) -> usize {
        self.nodes.len() - 1
    }

    fn scal(&self, reach: &[f64]) -> f64 {
        reach.iter().zip(&self.prior).map(|(r, p)| r * p).sum()
    }

    /// Returns counterfactual values, players x 169, flattened.
    fn cfr(&mut self, node: usize, reach: &[f64], t: f64) -> Vec<f64> {
        let n = self.cfg.players;
        let mut out = vec![0.0; n * N];
        match &self.nodes[node] {
            Node::T(term) => {
                let scals: Vec<f64> = (0..n).map(|p| self.scal(&reach[p * N..(p + 1) * N])).collect();
                match term.kind {
                    Kind::Fold { winner } => {
                        let pot: f64 = term.invest.iter().sum();
                        for p in 0..n {
                            let net = if p == winner { pot - term.invest[p] } else { -term.invest[p] };
                            let others: f64 = (0..n).filter(|&q| q != p).map(|q| scals[q]).product();
                            out[p * N..(p + 1) * N].iter_mut().for_each(|x| *x = net * others);
                        }
                    }
                    Kind::Flop { a, b, .. } | Kind::AllIn { a, b } => {
                        let folded: f64 = (0..n).filter(|&q| q != a && q != b).map(|q| scals[q]).product();
                        let (ra, rb) = (&reach[a * N..(a + 1) * N], &reach[b * N..(b + 1) * N]);
                        let mut pab = 0.0;
                        for i in 0..N {
                            let (mut va, mut vb, mut ci) = (0.0, 0.0, 0.0);
                            for j in 0..N {
                                va += term.ma[i * N + j] * rb[j];
                                vb += term.mb[i * N + j] * ra[j];
                                ci += term.cond[i * N + j] * rb[j];
                            }
                            out[a * N + i] = va;
                            out[b * N + i] = vb;
                            pab += self.prior[i] * ra[i] * ci;
                        }
                        for p in 0..n {
                            if p == a || p == b {
                                let others: f64 = (0..n).filter(|&q| q != a && q != b).map(|q| scals[q]).product();
                                out[p * N..(p + 1) * N].iter_mut().for_each(|x| *x *= others);
                            } else {
                                let others: f64 = (0..n).filter(|&q| q != a && q != b && q != p).map(|q| scals[q]).product();
                                let v = -term.invest[p] * pab * others;
                                out[p * N..(p + 1) * N].iter_mut().for_each(|x| *x = v);
                            }
                        }
                        let _ = folded;
                    }
                }
                out
            }
            Node::D(d) => {
                let p = d.player;
                let k = d.actions.len();
                let children: Vec<usize> = d.actions.iter().map(|a| a.1).collect();
                let sigma = Self::current_strategy(&d.regret, k);
                let mut child_vals = Vec::with_capacity(k);
                for (a, &child) in children.iter().enumerate() {
                    let mut r = reach.to_vec();
                    for h in 0..N {
                        r[p * N + h] *= sigma[a * N + h];
                    }
                    child_vals.push(self.cfr(child, &r, t));
                }
                for (a, cv) in child_vals.iter().enumerate() {
                    for q in 0..n {
                        for h in 0..N {
                            let v = cv[q * N + h];
                            out[q * N + h] += if q == p { sigma[a * N + h] * v } else { v };
                        }
                    }
                }
                if let Node::D(d) = &mut self.nodes[node] {
                    for a in 0..k {
                        for h in 0..N {
                            let gain = child_vals[a][p * N + h] - out[p * N + h];
                            let r = &mut d.regret[a * N + h];
                            *r = (*r + gain).max(0.0);
                            d.strat_sum[a * N + h] += t * reach[p * N + h] * sigma[a * N + h];
                        }
                    }
                }
                out
            }
        }
    }

    fn current_strategy(regret: &[f64], k: usize) -> Vec<f64> {
        let mut s = vec![0.0; k * N];
        for h in 0..N {
            let total: f64 = (0..k).map(|a| regret[a * N + h]).sum();
            for a in 0..k {
                s[a * N + h] = if total > 0.0 { regret[a * N + h] / total } else { 1.0 / k as f64 };
            }
        }
        s
    }

    pub fn solve(&mut self, iterations: u32) {
        let reach = vec![1.0; self.cfg.players * N];
        let root = self.root();
        for _ in 0..iterations {
            self.iterations += 1;
            let t = self.iterations as f64;
            self.cfr(root, &reach, t);
        }
    }

    fn average(&self, node: usize) -> Vec<f64> {
        let Node::D(d) = &self.nodes[node] else { panic!("not a decision") };
        let k = d.actions.len();
        let mut s = vec![0.0; k * N];
        for h in 0..N {
            let total: f64 = (0..k).map(|a| d.strat_sum[a * N + h]).sum();
            for a in 0..k {
                s[a * N + h] = if total > 0.0 { d.strat_sum[a * N + h] / total } else { 1.0 / k as f64 };
            }
        }
        s
    }

    /// Walks from the root following `path` (action names, e.g. ["fold","fold","open","call"])
    /// and returns every player's reach vector under the average strategy, plus the final node.
    pub fn walk(&self, path: &[&str]) -> Result<(usize, Vec<Vec<f64>>), String> {
        let mut node = self.root();
        let mut reach = vec![vec![1.0; N]; self.cfg.players];
        for step in path {
            let Node::D(d) = &self.nodes[node] else { return Err(format!("path too long at {step}")) };
            let a = d.actions.iter().position(|x| x.0 == *step).ok_or(format!("no action {step}"))?;
            let avg = self.average(node);
            for h in 0..N {
                reach[d.player][h] *= avg[a * N + h];
            }
            node = d.actions[a].1;
        }
        Ok((node, reach))
    }

    /// Action path for "opener opens, everyone between folds, responder does `resp`".
    pub fn path(&self, opener: &str, responder: &str, resp: &[&str]) -> Result<Vec<&'static str>, String> {
        let pos = self.cfg.positions();
        let o = pos.iter().position(|p| *p == opener).ok_or("bad opener")?;
        let r = pos.iter().position(|p| *p == responder).ok_or("bad responder")?;
        if r <= o {
            return Err("responder must act after opener".into());
        }
        let mut path: Vec<&'static str> = vec!["fold"; o];
        path.push("open");
        path.extend(std::iter::repeat("fold").take(r - o - 1));
        for s in resp {
            path.push(match *s {
                "call" => "call",
                "3bet" => "3bet",
                "jam" => "jam",
                "fold" => "fold",
                x => return Err(format!("bad action {x}")),
            });
        }
        Ok(path)
    }

    /// Strategy at a decision node for its player: (action names, 169 x k frequencies).
    pub fn strategy_at(&self, path: &[&str]) -> Result<(usize, Vec<String>, Vec<f64>), String> {
        let (node, _) = self.walk(path)?;
        let Node::D(d) = &self.nodes[node] else { return Err("not a decision".into()) };
        Ok((d.player, d.actions.iter().map(|a| a.0.clone()).collect(), self.average(node)))
    }

    /// Combo-weighted frequency of each action at a node.
    pub fn action_freqs(&self, path: &[&str]) -> Result<Vec<(String, f64)>, String> {
        let (_, names, avg) = self.strategy_at(path)?;
        let k = names.len();
        Ok((0..k)
            .map(|a| (names[a].clone(), (0..N).map(|h| avg[a * N + h] * self.prior[h]).sum()))
            .collect())
    }

    pub fn terminals_zero_sum(&self) -> bool {
        self.nodes.iter().all(|n| match n {
            Node::T(t) => {
                let pot: f64 = t.invest.iter().sum();
                pot > 0.0 && t.invest.iter().all(|&x| x >= 0.0 && x <= self.cfg.stack)
            }
            _ => true,
        })
    }
}

/// postflop-solver range string, e.g. "AA:1,AKs:0.531". Weights below 0.005 are dropped.
pub fn range_string(reach: &[f64]) -> String {
    (0..N)
        .rev()
        .filter(|&h| reach[h] >= 0.005)
        .map(|h| format!("{}:{:.3}", hand169::name(h), reach[h].min(1.0)))
        .collect::<Vec<_>>()
        .join(",")
}

/// 13x13 grid (AA top-left, suited above the diagonal) as text, percent per cell.
pub fn grid(freq: &[f64]) -> String {
    let mut s = String::new();
    for r1 in (0..13).rev() {
        for r2 in (0..13).rev() {
            let i = if r1 == r2 { 13 * r1 + r1 } else if r2 < r1 { 13 * r1 + r2 } else { 13 * r1 + r2 };
            // row r1, column r2: above diagonal (r2 < r1) suited -> index 13*r1+r2; below -> offsuit 13*r2... use class index directly
            let idx = if r2 < r1 { 13 * r1 + r2 } else if r2 > r1 { 13 * r1 + r2 } else { i };
            s.push_str(&format!("{:>4.0}", freq[idx] * 100.0));
        }
        s.push('\n');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use poker_core::hand169::index_of;

    fn solved(players: usize, stack: f64, iters: u32) -> Solver {
        let eq = EquityTable::compute(4000, 3);
        let mut s = Solver::new(Config::new(players, stack), eq);
        s.solve(iters);
        s
    }

    #[test]
    fn sane_three_handed() {
        let s = solved(3, 40.0, 1500);
        assert!(s.terminals_zero_sum());
        let (_, names, avg) = s.strategy_at(&[]).unwrap(); // BTN first in
        let open = names.iter().position(|n| n == "open").unwrap();
        let f = |h: &str| avg[open * N + index_of(h).unwrap()];
        assert!(f("AA") > 0.95);
        assert!(f("AKo") > 0.95);
        assert!(f("72o") < 0.5, "72o open {}", f("72o"));
        let btn_open: f64 = s.action_freqs(&[]).unwrap().iter().find(|x| x.0 == "open").unwrap().1;
        assert!(btn_open > 0.35 && btn_open < 0.95, "BTN open {btn_open}");
    }

    #[test]
    fn six_max_ordering() {
        let s = solved(6, 40.0, 1500);
        let open = |path: &[&str]| s.action_freqs(path).unwrap().iter().find(|x| x.0 == "open").unwrap().1;
        let utg = open(&[]);
        let btn = open(&["fold", "fold", "fold"]);
        assert!(utg < btn, "UTG {utg} should be tighter than BTN {btn}");
        assert!(utg > 0.08 && utg < 0.35, "UTG open {utg}");
    }
}

/// Everything the postflop solver needs for one matchup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostflopSpot {
    pub oop: String,
    pub ip: String,
    pub oop_range: String,
    pub ip_range: String,
    pub pot_bb: f64,
    pub stack_bb: f64,
    /// combo-weighted share of all hands each player arrives with
    pub oop_width: f64,
    pub ip_width: f64,
}

impl Solver {
    /// Ranges for "opener opens, responder calls" (Srp) or "responder 3-bets, opener calls" (ThreeBet).
    pub fn postflop_spot(&self, opener: &str, responder: &str, pot_type: PotType) -> Result<PostflopSpot, String> {
        let resp: &[&str] = match pot_type {
            PotType::Srp => &["call"],
            PotType::ThreeBet => &["3bet", "call"],
            PotType::FourBet => return Err("4-bet pots are not solved postflop".into()),
        };
        let path = self.path(opener, responder, resp)?;
        let (node, reach) = self.walk(&path)?;
        let Node::T(t) = &self.nodes[node] else { return Err("path does not end at a flop".into()) };
        let (a, b) = match t.kind {
            Kind::Flop { a, b, .. } => (a, b),
            _ => return Err("not a flop terminal".into()),
        };
        let pos = self.cfg.positions();
        let (oop, ip) = if self.cfg.in_position(a, b) { (b, a) } else { (a, b) };
        let width = |p: usize| reach[p].iter().zip(&self.prior).map(|(r, w)| r * w).sum::<f64>();
        Ok(PostflopSpot {
            oop: pos[oop].into(),
            ip: pos[ip].into(),
            oop_range: range_string(&reach[oop]),
            ip_range: range_string(&reach[ip]),
            pot_bb: t.invest.iter().sum(),
            stack_bb: self.cfg.stack - t.invest[a],
            oop_width: width(oop),
            ip_width: width(ip),
        })
    }
}

#[cfg(test)]
mod spot_tests {
    use super::*;

    #[test]
    fn btn_vs_bb_spots() {
        let eq = EquityTable::compute(4000, 3);
        let mut s = Solver::new(Config::new(3, 40.0), eq);
        s.solve(1000);
        let srp = s.postflop_spot("BTN", "BB", PotType::Srp).unwrap();
        assert_eq!((srp.oop.as_str(), srp.ip.as_str()), ("BB", "BTN"));
        assert!((srp.pot_bb - 5.5).abs() < 1e-9 && (srp.stack_bb - 37.5).abs() < 1e-9);
        let tb = s.postflop_spot("BTN", "BB", PotType::ThreeBet).unwrap();
        assert!((tb.pot_bb - 20.5).abs() < 1e-9 && (tb.stack_bb - 30.0).abs() < 1e-9);
        assert!(tb.oop_width < srp.oop_width);
        let sb = s.postflop_spot("SB", "BB", PotType::Srp).unwrap();
        assert_eq!((sb.oop.as_str(), sb.ip.as_str()), ("SB", "BB"));
    }
}
