// Plays a full hand, then rewinds and replays it the same way (dev tool).
use trainer::drill::Mode;
use trainer::{Engine, Filters};

fn play(e: &mut Engine) -> anyhow::Result<Vec<String>> {
    let mut log = Vec::new();
    loop {
        let v = e.current().unwrap().clone();
        log.push(format!("{} #{} board {} | {}", v.street, v.decision, v.board.join(""), v.line.join(", ")));
        let pick = v.actions.iter().position(|a| a.kind == "call" || a.kind == "check").unwrap_or(0);
        let fb = e.answer(pick, None)?;
        log.push(format!("   took {} -> {} | {}", v.actions[pick].label, fb.verdict, fb.lesson.as_ref().map(|l| l.concept.clone()).unwrap_or_default()));
        let st = e.continue_hand()?;
        if let Some(r) = st.result {
            log.push(format!("   result: {}", r.summary));
            return Ok(log);
        }
    }
}

fn main() -> anyhow::Result<()> {
    let mut e = Engine::new(std::path::Path::new("data"), 21);
    e.new_drill(&Filters::default(), Mode::Hand)?;
    let first = play(&mut e)?;
    println!("FIRST PLAY\n{}", first.join("\n"));
    println!("review has {} decisions", e.review().len());
    e.rewind(0)?;
    let second = play(&mut e)?;
    println!("REPLAY identical: {}", first == second);
    if first != second { println!("{}", second.join("\n")); }
    // go back to the last decision and check it restores
    let n = e.review().len();
    let v = e.rewind(n - 1)?;
    println!("rewound to decision {} on the {} ({}), live river: {}", v.decision, v.street, v.board.join(""), v.live_solved);
    let fb = e.answer(0, None)?;
    println!("re-answered: {} | lesson: {}", fb.verdict, fb.lesson.map(|l| format!("{} / {} / RULE: {}", l.verdict, l.concept, l.rule)).unwrap_or_default());
    Ok(())
}
