// One-off benchmark: time and size a real 40bb solve before planning Milestone 1.
use postflop_solver::*;
use std::time::Instant;

const BB: i32 = 100; // chip units per big blind

fn run(name: &str, oop: &str, ip: &str, flop: &str, pot_bb: f64, eff_bb: f64, slim: bool) {
    let card_config = CardConfig {
        range: [oop.parse().unwrap(), ip.parse().unwrap()],
        flop: flop_from_str(flop).unwrap(),
        turn: NOT_DEALT,
        river: NOT_DEALT,
    };
    let sizes = if slim {
        BetSizeOptions::try_from(("33%, 75%, a", "a")).unwrap()
    } else {
        BetSizeOptions::try_from(("33%, 75%, a", "3x, a")).unwrap()
    };
    let river = if slim { BetSizeOptions::try_from(("75%, a", "a")).unwrap() } else { sizes.clone() };
    let tree_config = TreeConfig {
        initial_state: BoardState::Flop,
        starting_pot: (pot_bb * BB as f64) as i32,
        effective_stack: (eff_bb * BB as f64) as i32,
        rake_rate: 0.0,
        rake_cap: 0.0,
        flop_bet_sizes: [sizes.clone(), sizes.clone()],
        turn_bet_sizes: [sizes.clone(), sizes.clone()],
        river_bet_sizes: [river.clone(), river],
        turn_donk_sizes: None,
        river_donk_sizes: None,
        add_allin_threshold: 1.5,
        force_allin_threshold: 0.15,
        merging_threshold: 0.1,
    };
    let tree = ActionTree::new(tree_config).unwrap();
    let mut game = PostFlopGame::with_config(card_config, tree).unwrap();
    let (raw, comp) = game.memory_usage();
    let gb = |b: u64| b as f64 / 1024f64.powi(3);
    let t = Instant::now();
    game.allocate_memory(true);
    let target = game.tree_config().starting_pot as f32 * 0.005;
    let expl = solve(&mut game, 1000, target, false);
    let secs = t.elapsed().as_secs_f64();
    if slim {
        game.set_target_storage_mode(BoardState::Turn).unwrap();
    }
    let path = format!("/tmp/bench-{name}.bin");
    let _ = std::fs::remove_file(&path);
    save_data_to_file(&game, "bench", &path, Some(19)).unwrap();
    let disk = std::fs::metadata(&path).unwrap().len();
    println!(
        "{name}: flop {flop} | mem raw {:.2} GB, compressed {:.2} GB | solve {:.1}s | expl {:.3}% pot | file (zstd) {:.1} MB",
        gb(raw), gb(comp), secs, 100.0 * expl / game.tree_config().starting_pot as f32, disk as f64 / 1e6
    );
}

fn main() {
    // Placeholder ranges, only for sizing. Real ranges come from the preflop step.
    let bb_defend = "22+,A2s+,A2o+,K2s+,K5o+,Q2s+,Q8o+,J4s+,J8o+,T6s+,T8o+,96s+,98o,85s+,87o,74s+,64s+,53s+,43s";
    let btn_open = "22+,A2s+,A2o+,K2s+,K8o+,Q4s+,Q9o+,J6s+,J9o+,T6s+,T9o,96s+,85s+,75s+,64s+,54s";
    let bb_3bet = "TT+,AQs+,AKo,A5s-A4s,K9s,QJs,T9s,65s";
    let btn_call3 = "99-22,AJs-A6s,AQo,KTs+,KQo,QTs+,JTs,T9s,98s,87s";
    let which = std::env::args().nth(1).unwrap_or_default();
    if which.is_empty() {
        run("srp-dry", bb_defend, btn_open, "Ks7d2c", 5.5, 37.5, false);
        run("srp-wet", bb_defend, btn_open, "9h8h6c", 5.5, 37.5, false);
    }
    if which == "slim" {
        run("slim-dry", bb_defend, btn_open, "Ks7d2c", 5.5, 37.5, true);
        run("slim-wet", bb_defend, btn_open, "9h8h6c", 5.5, 37.5, true);
        return;
    }
    run("3bp", bb_3bet, btn_call3, "Ks7d2c", 22.5, 29.0, false);
}
