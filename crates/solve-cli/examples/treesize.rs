// Memory of candidate bet-size trees for a few representative spots (dev tool).
use solver_api::{BetSizes, SpotConfig};
use std::collections::BTreeMap;

fn main() -> anyhow::Result<()> {
    let variants: Vec<(&str, BetSizes)> = vec![
        ("current", BetSizes::slim()),
        ("turn +125%", BetSizes { turn_bet: "33%, 75%, 125%, a".into(), ..BetSizes::slim() }),
        ("flop +125%, turn +125%", BetSizes { flop_bet: "33%, 75%, 125%, a".into(), turn_bet: "33%, 75%, 125%, a".into(), ..BetSizes::slim() }),
        ("turn 33/75/100/150", BetSizes { turn_bet: "33%, 75%, 100%, 150%, a".into(), ..BetSizes::slim() }),
    ];
    for stack in [50u32, 100] {
        let ranges: BTreeMap<String, preflop::PostflopSpot> = serde_json::from_str(&std::fs::read_to_string(format!("data/ranges/3max-{stack}bb.json"))?)?;
        for (key, flop) in [("BTNvBB-srp", "Ks7d2c"), ("BTNvBB-srp", "9h8h6c"), ("BTNvBB-3bp", "Ks7d2c")] {
            let r = &ranges[key];
            for (name, sizes) in &variants {
                let mut sizes = sizes.clone();
                if key.ends_with("3bp") {
                    sizes.flop_raise = "3x, a".into();
                    sizes.turn_raise = "3x, a".into();
                    sizes.river_raise = "3x, a".into();
                } else if stack >= 80 {
                    sizes.flop_raise = "3x".into();
                    sizes.turn_raise = "3x".into();
                }
                let spot = SpotConfig { id: "t".into(), oop_range: r.oop_range.clone(), ip_range: r.ip_range.clone(), flop: flop.into(), pot_bb: r.pot_bb, stack_bb: r.stack_bb, sizes, store_river: false, target_exploitability_pct: 0.5, max_iterations: 1 };
                let g = solver_pfs::build(&spot)?;
                println!("{stack}bb {key} {flop} | {name:52} {:6.2} GB", g.memory_usage().1 as f64 / 1024f64.powi(3));
            }
        }
    }
    Ok(())
}
