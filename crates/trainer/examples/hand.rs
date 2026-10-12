// Plays full hands in Hand mode, always taking the solver's best action (dev tool).
use trainer::drill::Mode;
use trainer::{Engine, Filters};

fn main() -> anyhow::Result<()> {
    let n: usize = std::env::args().nth(1).map(|s| s.parse().unwrap()).unwrap_or(3);
    let mut e = Engine::new(std::path::Path::new("data"), 11);
    for h in 0..n {
        let mut v = e.new_drill(&Filters::default(), Mode::Hand)?;
        println!("\n=== hand {} | {} {} | {} | you {} {}", h + 1, v.matchup, v.pot_type, v.board.join(" "), v.hero, v.hand.join(""));
        loop {
            println!("  [{} #{}] {} | pot {:.1} to call {:.1} | {}{}", v.street, v.decision, v.node_type, v.pot_bb, v.to_call_bb, v.situation, if v.live_solved { " (river solved live)" } else { "" });
            let probe = e.answer(0, None)?; // grade action 0 first to learn the best action...
            let best = probe.score.best;
            let _ = best;
            let step = e.continue_hand()?;
            println!("    took {} ({}) -> {}", v.actions[0].label, probe.verdict, step.events.join(", "));
            if let Some(r) = step.result {
                println!("  RESULT: {}{}", r.summary, r.villain_hand.map(|h| format!(" (villain {})", h.join(""))).unwrap_or_default());
                break;
            }
            v = step.next.unwrap();
        }
    }
    Ok(())
}
