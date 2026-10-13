// Deals a few "Big bets" drills (dev tool): bigbets facing|making
use trainer::drill::Mode;
use trainer::{Engine, Filters};

fn main() -> anyhow::Result<()> {
    let kind = std::env::args().nth(1).unwrap_or("facing".into());
    let mut e = Engine::new(std::path::Path::new("data"), 5);
    let f = Filters { big_bets: kind.clone(), ..Default::default() };
    for _ in 0..4 {
        let v = e.new_drill(&f, Mode::Spot)?;
        println!("{} {} | {} | {} | pot {:.1} call {:.1} | {}", v.stack, v.matchup, v.board.join(""), v.line.join(", "), v.pot_bb, v.to_call_bb, v.actions.iter().map(|a| a.label.clone()).collect::<Vec<_>>().join(" / "));
        let fb = e.answer(0, None)?;
        println!("   solver: {}", fb.actions.iter().map(|a| format!("{} {:.0}%", a.label, a.freq_hand * 100.0)).collect::<Vec<_>>().join(", "));
    }
    Ok(())
}
