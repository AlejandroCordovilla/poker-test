// How often the solver uses each bet size at the flop and turn (dev tool).
use postflop_solver::{load_data_from_file, Action, PostFlopGame};
use std::collections::BTreeMap;
use trainer::scoring::range_freqs;

fn main() -> anyhow::Result<()> {
    let spots = trainer::spots::load(std::path::Path::new("data"));
    let mut agg: BTreeMap<String, (f64, usize)> = BTreeMap::new();
    let mut by_tex: BTreeMap<String, (f64, f64, usize)> = BTreeMap::new();
    for s in spots.iter() {
        let (mut g, _): (PostFlopGame, String) = load_data_from_file(&s.file, None).map_err(|e| anyhow::anyhow!(e))?;
        // node A: OOP first to act on the flop; node B: IP after OOP checks
        for (who, hist) in [("OOP flop", vec![]), ("IP flop vs check", vec![0usize])] {
            g.apply_history(&hist);
            g.cache_normalized_weights();
            let p = g.current_player();
            let acts = g.available_actions();
            let f = range_freqs(&g.strategy(), g.normalized_weights(p), acts.len());
            let pot = g.tree_config().starting_pot as f64;
            for (a, x) in acts.iter().zip(&f) {
                let label = match a {
                    Action::Check => "check".to_string(),
                    Action::Bet(v) => format!("bet {:.0}%", 100.0 * *v as f64 / pot),
                    Action::AllIn(_) => "all-in".to_string(),
                    _ => continue,
                };
                let e = agg.entry(format!("{} {} {}", s.pot_type, who, label)).or_default();
                e.0 += x;
                e.1 += 1;
                if who == "IP flop vs check" && s.pot_type == "srp" {
                    let tex = s.texture.split_whitespace().next().unwrap_or("").to_string();
                    let t = by_tex.entry(tex).or_default();
                    if label.starts_with("bet 7") { t.1 += x; }
                    if label.starts_with("bet 3") { t.0 += x; }
                    if label == "check" { t.2 += 1; }
                }
            }
        }
    }
    println!("{} solved spots", spots.len());
    for (k, (sum, n)) in agg { println!("  {k:40} {:>5.1}% avg over {n}", 100.0 * sum / n as f64); }
    println!("IP vs check, single-raised, by high card: (33% bet, 75% bet)");
    for (k, (a, b, n)) in by_tex { if n > 0 { println!("  {k:10} 33%: {:>5.1}%  75%: {:>5.1}%  ({n} flops)", 100.0 * a / n as f64, 100.0 * b / n as f64); } }
    Ok(())
}
