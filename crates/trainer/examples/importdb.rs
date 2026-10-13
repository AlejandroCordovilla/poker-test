// End-to-end import into a throwaway database (dev tool):
// importdb <log> <hero> <db path> [player=display ...]
use std::collections::HashMap;
use trainer::db::Db;
use trainer::import::*;

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let db = Db::open(std::path::Path::new(&a[3]))?;
    let hands = parse(&std::fs::read_to_string(&a[1])?)?;
    for pair in a.iter().skip(4) {
        if let Some((name, display)) = pair.split_once('=') {
            set_alias(&db, name, display)?;
        }
    }
    let names = aliases(&db)?;
    let mut an = Analyzer::new(std::path::Path::new("data"));
    let mut jobs = Vec::new();
    for h in &hands {
        if hand_exists(&db, &h.id)? { continue; }
        let (g, st, note, job) = an.preflop(h, &a[2], &names)?;
        for x in &g { save_graded(&db, x, None)?; }
        let villain = job.as_ref().map(|j| j.villain_display.clone()).unwrap_or_default();
        save_hand(&db, "log.csv", h, &a[2], &position_of(h, &a[2]), &villain, "", &st, &note, &g)?;
        if let Some(j) = job { save_job(&db, &j)?; jobs.push(j); }
    }
    println!("pending jobs in db: {}", pending_jobs(&db)?.len());
    let j = &jobs[1];
    let (g, note) = analyze_postflop(j, 4.0)?;
    for x in &g { save_graded(&db, x, Some(&j.villain_display))?; }
    finish_hand(&db, &j.hand_id, if note.is_empty() { "scored" } else { "partial" }, &note, &g)?;
    println!("pending after one: {}", pending_jobs(&db)?.len());
    let rows = list_hands(&db)?;
    let scored = rows.iter().find(|r| r.hand_id == j.hand_id).unwrap();
    println!("hand {} vs {} status {} decisions {} loss {:.2} net {:.1}", scored.hand_no, scored.villain, scored.status, scored.decisions, scored.ev_loss, scored.net_bb);
    for d in hand_decisions(&db, &j.hand_id)? { println!("   {} {} {} -> {} ({}) {:.2}", d.street, d.situation, d.board, d.chosen, d.verdict, d.ev_loss_bb); }
    let rep = db.leaks(Some("import"), None)?;
    println!("leak stats over {} imported decisions:", rep.decisions);
    for s in rep.stats { println!("   {:45} n={:3} you {:.0}% solver {:.0}% -> {}", s.name, s.n, s.you * 100.0, s.solver * 100.0, s.verdict); }
    println!("re-import skips existing: {}", hands.iter().filter(|h| hand_exists(&db, &h.id).unwrap()).count());
    Ok(())
}
