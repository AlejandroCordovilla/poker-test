// Prints first-in and facing-raise strategies for a table size and stack (dev tool).
use poker_core::{equity::EquityTable, hand169};
use preflop::{Config, Solver};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let players: usize = args.get(1).map(|s| s.parse().unwrap()).unwrap_or(3);
    let stack: f64 = args.get(2).map(|s| s.parse().unwrap()).unwrap_or(40.0);
    let eq = EquityTable::load_or_compute(std::path::Path::new("data/equity169.json"), 20_000).unwrap();
    let mut cfg = Config::new(players, stack);
    if let Ok(v) = std::env::var("TB") { cfg.threebet_blind = v.parse().unwrap(); cfg.threebet_ip = v.parse::<f64>().unwrap() * 0.85; }
    if let Ok(v) = std::env::var("INIT") { cfg.initiative = v.parse().unwrap(); }
    let mut s = Solver::new(cfg, eq);
    s.solve(3000);
    let show = |label: &str, path: &[&str], action: &str| {
        let (_, names, avg) = s.strategy_at(path).unwrap();
        let a = names.iter().position(|n| n == action).unwrap();
        let freqs = s.action_freqs(path).unwrap();
        let mut hands: Vec<(usize, f64)> = (0..hand169::N).map(|h| (h, avg[a * hand169::N + h])).filter(|x| x.1 > 0.5).collect();
        hands.sort_by(|x, y| y.1.partial_cmp(&x.1).unwrap());
        println!("\n{label}: {}", freqs.iter().map(|(n, f)| format!("{n} {:.0}%", f * 100.0)).collect::<Vec<_>>().join(", "));
        println!("  {action} (>50%): {}", hands.iter().map(|(h, _)| hand169::name(*h)).collect::<Vec<_>>().join(" "));
    };
    if players == 6 {
        for (i, pos) in ["UTG", "HJ", "CO", "BTN", "SB"].iter().enumerate() {
            let path = vec!["fold"; i];
            let f = s.action_freqs(&path).unwrap();
            println!("{pos} first in: open {:.0}%", f[1].1 * 100.0);
        }
        return;
    }
    show("BTN first in", &[], "open");
    show("SB first in", &["fold"], "open");
    show("BB vs BTN open", &["open", "fold"], "call");
    show("BB vs BTN open", &["open", "fold"], "3bet");

    show("BTN vs BB 3-bet", &["open", "fold", "3bet"], "call");
    show("BTN vs BB 3-bet", &["open", "fold", "3bet"], "jam");
}
