// Times a live flop re-solve with the stored flop sizes and slim turn/river sizes (dev tool):
// flopsub <spot .bin>
use postflop_solver::*;
use std::time::Instant;

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let (game, _): (PostFlopGame, String) = load_data_from_file(&a[1], None).map_err(|e| anyhow::anyhow!(e))?;
    let tc = game.tree_config().clone();
    let slim = BetSizeOptions::try_from(("75%, a", "a")).unwrap();
    let tree = TreeConfig { turn_bet_sizes: [slim.clone(), slim.clone()], river_bet_sizes: [slim.clone(), slim], ..tc.clone() };
    println!("flop sizes {:?}", tc.flop_bet_sizes[0]);
    let mut g = PostFlopGame::with_config(game.card_config().clone(), ActionTree::new(tree).map_err(|e| anyhow::anyhow!(e))?).map_err(|e| anyhow::anyhow!(e))?;
    let (mem, memc) = g.memory_usage();
    println!("memory {:.0} MB (compressed {:.0} MB)", mem as f64 / 1e6, memc as f64 / 1e6);
    g.allocate_memory(true);
    let t = Instant::now();
    let target = g.tree_config().starting_pot as f32 * 0.01;
    let e = solve(&mut g, 200, target, false);
    println!("solved in {:.1}s, exploitability {:.2}% pot", t.elapsed().as_secs_f64(), 100.0 * e / g.tree_config().starting_pot as f32);
    Ok(())
}
