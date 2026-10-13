// Times a live turn subgame (turn + river) built from a stored flop solution (dev tool):
// turnsub <spot .bin>
use postflop_solver::*;
use std::time::Instant;

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let (mut game, _): (PostFlopGame, String) = load_data_from_file(&a[1], None).map_err(|e| anyhow::anyhow!(e))?;
    // flop: check, check; then the first legal turn card
    game.play(0);
    game.play(0);
    let turn = (0..52).find(|c| game.possible_cards() >> c & 1 == 1).unwrap();
    game.play(turn);
    game.cache_normalized_weights();
    let board = game.current_board();
    let range = |p: usize| {
        let w = game.normalized_weights(p);
        let max = w.iter().cloned().fold(0.0f32, f32::max).max(1e-12);
        Range::from_hands_weights(game.private_cards(p), &w.iter().map(|x| x / max).collect::<Vec<_>>()).unwrap()
    };
    let tc = game.tree_config().clone();
    let cc = CardConfig { range: [range(0), range(1)], flop: [board[0], board[1], board[2]], turn: board[3], river: NOT_DEALT };
    let tree = TreeConfig { initial_state: BoardState::Turn, ..tc.clone() };
    println!("pot {} stack {} turn sizes {:?}", tc.starting_pot, tc.effective_stack, tc.turn_bet_sizes[0]);
    let mut g = PostFlopGame::with_config(cc, ActionTree::new(tree).map_err(|e| anyhow::anyhow!(e))?).map_err(|e| anyhow::anyhow!(e))?;
    let (mem, memc) = g.memory_usage();
    println!("memory {:.0} MB (compressed {:.0} MB)", mem as f64 / 1e6, memc as f64 / 1e6);
    println!("stored turn actions {:?}", game.available_actions());
    println!("sub    turn actions {:?}", g.available_actions());
    g.allocate_memory(true);
    let t = Instant::now();
    let target = g.tree_config().starting_pot as f32 * 0.005;
    let e = solve(&mut g, 300, target, false);
    println!("solved in {:.1}s, exploitability {:.3} ({:.2}% pot)", t.elapsed().as_secs_f64(), e, 100.0 * e / g.tree_config().starting_pot as f32);
    Ok(())
}
