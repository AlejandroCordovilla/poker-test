//! postflop-solver (b-inary, AGPL-3.0) adapter.

use anyhow::{anyhow, Result};
use postflop_solver::*;
use solver_api::{Control, Estimate, SolveReport, SpotConfig};
use std::path::Path;
use std::time::Instant;

/// Chip units per big blind inside the solver (integer chips).
pub const BB: f64 = 100.0;

pub struct Pfs;

fn sizes(bet: &str, raise: &str) -> Result<BetSizeOptions> {
    BetSizeOptions::try_from((bet, raise)).map_err(|e| anyhow!(e))
}

pub fn build(spot: &SpotConfig) -> Result<PostFlopGame> {
    let card_config = CardConfig {
        range: [
            spot.oop_range.parse().map_err(|e: String| anyhow!("oop range: {e}"))?,
            spot.ip_range.parse().map_err(|e: String| anyhow!("ip range: {e}"))?,
        ],
        flop: flop_from_str(&spot.flop).map_err(|e| anyhow!(e))?,
        turn: NOT_DEALT,
        river: NOT_DEALT,
    };
    let s = &spot.sizes;
    let (f, t, r) = (sizes(&s.flop_bet, &s.flop_raise)?, sizes(&s.turn_bet, &s.turn_raise)?, sizes(&s.river_bet, &s.river_raise)?);
    let tree_config = TreeConfig {
        initial_state: BoardState::Flop,
        starting_pot: (spot.pot_bb * BB).round() as i32,
        effective_stack: (spot.stack_bb * BB).round() as i32,
        rake_rate: 0.0,
        rake_cap: 0.0,
        flop_bet_sizes: [f.clone(), f],
        turn_bet_sizes: [t.clone(), t],
        river_bet_sizes: [r.clone(), r],
        turn_donk_sizes: None,
        river_donk_sizes: None,
        add_allin_threshold: 1.5,
        force_allin_threshold: 0.15,
        merging_threshold: 0.1,
    };
    let tree = ActionTree::new(tree_config).map_err(|e| anyhow!(e))?;
    PostFlopGame::with_config(card_config, tree).map_err(|e| anyhow!(e))
}

impl solver_api::Solver for Pfs {
    fn name(&self) -> &'static str {
        "postflop-solver"
    }

    fn estimate(&self, spot: &SpotConfig) -> Result<Estimate> {
        let game = build(spot)?;
        Ok(Estimate { memory_bytes: game.memory_usage().1 })
    }

    fn solve(&self, spot: &SpotConfig, out: &Path, ctl: &dyn Control) -> Result<SolveReport> {
        let t0 = Instant::now();
        let mut game = build(spot)?;
        let memory_bytes = game.memory_usage().1;
        game.allocate_memory(true);
        let pot = (spot.pot_bb * BB) as f32;
        let target = pot * (spot.target_exploitability_pct as f32 / 100.0);
        // same loop as postflop_solver::solve, with a checkpoint between iterations
        let mut compute = 0.0;
        let t = Instant::now();
        let mut expl = compute_exploitability(&game);
        compute += t.elapsed().as_secs_f64();
        for i in 0..spot.max_iterations {
            if expl <= target {
                break;
            }
            let t = Instant::now();
            solve_step(&game, i);
            if (i + 1) % 10 == 0 || i + 1 == spot.max_iterations {
                expl = compute_exploitability(&game);
            }
            let step = t.elapsed().as_secs_f64();
            compute += step;
            if !ctl.checkpoint(i + 1, 100.0 * expl as f64 / pot as f64, step) {
                return Err(anyhow!("aborted"));
            }
        }
        finalize(&mut game);
        if !spot.store_river {
            game.set_target_storage_mode(BoardState::Turn).map_err(|e| anyhow!(e))?;
        }
        let tmp = out.with_extension("partial");
        save_data_to_file(&game, &spot.id, &tmp, Some(19)).map_err(|e| anyhow!(e))?;
        std::fs::rename(&tmp, out)?;
        Ok(SolveReport {
            solver: self.name().into(),
            seconds: t0.elapsed().as_secs_f64(),
            compute_seconds: compute,
            exploitability_pct: 100.0 * expl as f64 / (spot.pot_bb * BB),
            memory_bytes,
            file_bytes: std::fs::metadata(out)?.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solver_api::{BetSizes, Solver};

    #[test]
    fn tiny_river_like_spot_solves() {
        // very short stacks keep this fast: checks the adapter end to end
        let spot = SpotConfig {
            id: "test".into(),
            oop_range: "AA,KK,QQ,AK,72o".into(),
            ip_range: "AA,KK,JJ,AQ,T9s".into(),
            flop: "Ks7d2c".into(),
            pot_bb: 20.0,
            stack_bb: 5.0,
            sizes: BetSizes::full(),
            store_river: false,
            target_exploitability_pct: 1.0,
            max_iterations: 300,
        };
        let est = Pfs.estimate(&spot).unwrap();
        assert!(est.memory_bytes > 0);
        let dir = std::env::temp_dir().join("pfs-adapter-test.bin");
        let rep = Pfs.solve(&spot, &dir, &solver_api::NoControl).unwrap();
        assert!(rep.exploitability_pct <= 1.0, "{rep:?}");
        assert!(rep.file_bytes > 0);
        let _ = std::fs::remove_file(dir);
    }
}
