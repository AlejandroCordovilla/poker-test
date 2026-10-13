//! Solver-independent types. The trainer and CLI only talk to `Solver`, so the backend
//! (postflop-solver today, TexasSolver as a cross-check) can be swapped.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// River sizes for live river solves (drills and imported hands): small, big, pot, overbet.
pub const RIVER_LIVE_BET: &str = "33%, 75%, 100%, 150%, a";
pub const RIVER_LIVE_RAISE: &str = "3x, a";

/// Bet sizes per street in postflop-solver notation ("33%, 75%, a"; raises "3x, a").
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BetSizes {
    pub flop_bet: String,
    pub flop_raise: String,
    pub turn_bet: String,
    pub turn_raise: String,
    pub river_bet: String,
    pub river_raise: String,
}

impl BetSizes {
    /// Single-raised pots: three bet sizes, raises all-in only, two river sizes.
    pub fn slim() -> Self {
        BetSizes {
            flop_bet: "33%, 75%, a".into(),
            flop_raise: "a".into(),
            turn_bet: "33%, 75%, a".into(),
            turn_raise: "a".into(),
            river_bet: "75%, a".into(),
            river_raise: "a".into(),
        }
    }

    /// Tree version 2 (2026-10-12): adds a 125% overbet on the flop and turn (flop overbet
    /// skipped at 100bb, where it doesn't fit in 16 GB). The river keeps 75% / all-in in the
    /// stored solves; drills and imported hands re-solve the river live with `RIVER_LIVE_*`.
    pub fn for_spot(single_raised: bool, spr: f64) -> Self {
        let over = "33%, 75%, 125%, a".to_string();
        match (single_raised, spr > 10.0) {
            (true, true) => BetSizes {
                flop_bet: "33%, 75%, a".into(),
                flop_raise: "3x".into(),
                turn_bet: over,
                turn_raise: "3x".into(),
                river_bet: "75%, a".into(),
                river_raise: "a".into(),
            },
            (true, false) => BetSizes { flop_bet: over.clone(), turn_bet: over, ..Self::slim() },
            (false, _) => BetSizes { flop_bet: over.clone(), turn_bet: over, river_bet: "33%, 75%, a".into(), ..Self::full() },
        }
    }

    /// Version 1 sizes (kept for reading older solves).
    pub fn for_spot_v1(single_raised: bool, spr: f64) -> Self {
        match (single_raised, spr > 10.0) {
            (true, true) => BetSizes { flop_raise: "3x".into(), turn_raise: "3x".into(), ..Self::slim() },
            (true, false) => Self::slim(),
            (false, _) => Self::full(),
        }
    }

    /// 3-bet pots are small (low SPR), so they keep the full tree.
    pub fn full() -> Self {
        let b = "33%, 75%, a".to_string();
        let r = "3x, a".to_string();
        BetSizes { flop_bet: b.clone(), flop_raise: r.clone(), turn_bet: b.clone(), turn_raise: r.clone(), river_bet: b, river_raise: r }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SpotConfig {
    pub id: String,
    pub oop_range: String,
    pub ip_range: String,
    /// e.g. "Ks7d2c"
    pub flop: String,
    pub pot_bb: f64,
    pub stack_bb: f64,
    pub sizes: BetSizes,
    /// keep river strategies on disk (large); otherwise flop + turn only
    pub store_river: bool,
    /// stop when exploitability <= this % of the starting pot
    pub target_exploitability_pct: f64,
    pub max_iterations: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Estimate {
    pub memory_bytes: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SolveReport {
    pub solver: String,
    pub seconds: f64,
    /// time spent actually computing (excludes pauses and throttling sleeps)
    #[serde(default)]
    pub compute_seconds: f64,
    pub exploitability_pct: f64,
    pub memory_bytes: u64,
    pub file_bytes: u64,
}

/// Lets a caller pause, throttle, abort and watch a running solve.
pub trait Control: Sync {
    /// Called between iterations with the measured compute time of the last step.
    /// May sleep (pause, throttle). Returns false to abort the solve.
    fn checkpoint(&self, iteration: u32, exploitability_pct: f64, step_seconds: f64) -> bool;
}

/// Runs flat out, never pauses.
pub struct NoControl;

impl Control for NoControl {
    fn checkpoint(&self, _: u32, _: f64, _: f64) -> bool {
        true
    }
}

pub trait Solver: Send + Sync {
    fn name(&self) -> &'static str;
    fn estimate(&self, spot: &SpotConfig) -> anyhow::Result<Estimate>;
    /// Solves and writes the result to `out`. Aborting returns an error and writes nothing.
    fn solve(&self, spot: &SpotConfig, out: &Path, ctl: &dyn Control) -> anyhow::Result<SolveReport>;
}
