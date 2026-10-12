// Terminal drill (dev tool): prints a spot, answers with the action given as an argument
// (or the first action), prints the grade and the reason.
use trainer::{Engine, Filters};

fn main() -> anyhow::Result<()> {
    let seed: u64 = std::env::args().nth(1).map(|s| s.parse().unwrap()).unwrap_or(1);
    let pick: Option<usize> = std::env::args().nth(2).map(|s| s.parse().unwrap());
    let mut e = Engine::new(std::path::Path::new("data"), seed);
    println!("{} solved spots", e.spots().len());
    let t = std::time::Instant::now();
    let v = e.new_drill(&Filters::default(), trainer::drill::Mode::Spot)?;
    println!("built in {:.2}s", t.elapsed().as_secs_f64());
    println!("{}bb {} ({}) | {} | board {} [{}]", v.stack, v.matchup, v.pot_type, v.preflop, v.board.join(" "), v.texture);
    println!("line: {}", if v.line.is_empty() { "-".into() } else { v.line.join(", ") });
    println!("you are {} ({}) with {} | pot {:.1}bb, to call {:.1}bb, stack {:.1}bb | {}", v.hero, if v.hero_in_position { "IP" } else { "OOP" }, v.hand.join(""), v.pot_bb, v.to_call_bb, v.hero_stack_bb, v.node_type);
    for a in &v.actions {
        println!("  [{}] {}", a.index, a.label);
    }
    let t = std::time::Instant::now();
    let f = e.answer(pick.unwrap_or(0), None)?;
    println!("graded in {:.2}s", t.elapsed().as_secs_f64());
    println!("-> {} (EV loss {:.2}bb){}", f.verdict, f.score.ev_loss_bb, if f.score.rare { ", rare action" } else { "" });
    for a in &f.actions {
        println!("  {:28} EV {:+.2}bb  this hand {:>4.0}%  range {:>4.0}%", a.label, a.ev_bb, a.freq_hand * 100.0, a.freq_range * 100.0);
    }
    println!("hand: {} | equity {:.0}%", f.hand_desc.made, f.equity * 100.0);
    println!("why: {}", f.reason);
    Ok(())
}
