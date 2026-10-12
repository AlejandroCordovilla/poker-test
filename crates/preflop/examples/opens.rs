// Prints each seat's first-in opening range (>50%) for 6-max at a stack depth (dev tool).
use poker_core::{equity::EquityTable, hand169};
use preflop::{Config, Solver};

fn main() {
    let stack: f64 = std::env::args().nth(1).map(|s| s.parse().unwrap()).unwrap_or(40.0);
    let eq = EquityTable::load_or_compute(std::path::Path::new("data/equity169.json"), 20_000).unwrap();
    let mut s = Solver::new(Config::new(6, stack), eq);
    s.solve(3000);
    for (i, pos) in ["UTG", "HJ", "CO", "BTN", "SB"].iter().enumerate() {
        let path = vec!["fold"; i];
        let (_, _, avg) = s.strategy_at(&path).unwrap();
        let mut hs: Vec<usize> = (0..hand169::N).filter(|&h| avg[hand169::N + h] > 0.5).collect();
        hs.sort_by_key(|&h| std::cmp::Reverse(h));
        println!("{pos}: {}", hs.iter().map(|&h| hand169::name(h)).collect::<Vec<_>>().join(" "));
    }
    let (_, names, avg) = s.strategy_at(&["fold", "fold", "fold", "open", "fold"]).unwrap();
    for (a, n) in names.iter().enumerate().skip(1) {
        let hs: Vec<String> = (0..hand169::N).rev().filter(|&h| avg[a * hand169::N + h] > 0.5).map(hand169::name).collect();
        println!("BB vs BTN open, {n}: {}", hs.join(" "));
    }
}
