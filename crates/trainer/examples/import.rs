// Runs the PokerNow importer on a log without saving (dev tool).
// Usage: import <log.csv> <hero name> [postflop jobs to solve]
use std::collections::HashMap;
use trainer::import::{analyze_postflop, parse, Analyzer};

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let text = std::fs::read_to_string(&a[1])?;
    let hero = &a[2];
    let solve_n: usize = a.get(3).map(|s| s.parse().unwrap()).unwrap_or(0);
    let hands = parse(&text)?;
    let mut an = Analyzer::new(std::path::Path::new("data"));
    let t = std::time::Instant::now();
    let mut status: HashMap<String, usize> = HashMap::new();
    let mut notes: HashMap<String, usize> = HashMap::new();
    let mut graded = Vec::new();
    let mut jobs = Vec::new();
    for h in &hands {
        let (g, st, note, job) = an.preflop(h, hero, &HashMap::new())?;
        *status.entry(st).or_default() += 1;
        if !note.is_empty() {
            *notes.entry(note).or_default() += 1;
        }
        graded.extend(g);
        jobs.extend(job);
    }
    println!("{} hands, preflop graded in {:.1}s: {} decisions", hands.len(), t.elapsed().as_secs_f64(), graded.len());
    println!("status: {status:?}");
    println!("notes: {notes:?}");
    let mut by: HashMap<String, (usize, f64, usize)> = HashMap::new();
    for g in &graded {
        let e = by.entry(format!("{} {}", g.view.hero, g.view.situation)).or_default();
        e.0 += 1;
        e.1 += g.fb.score.ev_loss_bb;
        e.2 += (!matches!(g.fb.verdict.as_str(), "Best" | "Correct")) as usize;
    }
    let mut v: Vec<_> = by.into_iter().collect();
    v.sort_by(|a, b| b.1 .1.partial_cmp(&a.1 .1).unwrap());
    for (k, (n, loss, bad)) in v {
        println!("  {k:24} n={n:3} mistakes={bad:3} total loss {loss:.1}bb");
    }
    let mut worst: Vec<_> = graded.iter().collect();
    worst.sort_by(|a, b| b.fb.score.ev_loss_bb.partial_cmp(&a.fb.score.ev_loss_bb).unwrap());
    for g in worst.iter().take(5) {
        println!("  worst: {} {} {} -> {} (best {}), lost {:.2}bb [{}]", g.view.hero, g.fb.hand_desc.made, g.view.situation, g.view.actions[g.fb.chosen].label, g.view.actions[g.fb.score.best].label, g.fb.score.ev_loss_bb, g.fb.verdict);
    }
    println!("{} postflop jobs queued", jobs.len());
    for j in jobs.iter().take(solve_n) {
        let t = std::time::Instant::now();
        match analyze_postflop(j, 4.0) {
            Ok((g, note)) => {
                println!("  solved {} ({} vs {}, {}bb pot, {:.0}bb behind) in {:.0}s: {} decisions {}", j.hand_id, j.info.matchup, j.villain, j.spot.pot_bb, j.spot.stack_bb, t.elapsed().as_secs_f64(), g.len(), note);
                for x in g {
                    println!("    {} {} on {} : {} -> {} (best {}) lost {:.2}bb", x.view.street, x.fb.hand_desc.made, x.view.board.join(""), x.view.situation, x.view.actions[x.fb.chosen].label, x.view.actions[x.fb.score.best].label, x.fb.score.ev_loss_bb);
                }
            }
            Err(e) => println!("  {} failed: {e}", j.hand_id),
        }
    }
    Ok(())
}
