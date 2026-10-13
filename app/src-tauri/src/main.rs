// Poker Trainer desktop app: Tauri commands over the trainer crate.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::Serialize;
use std::path::PathBuf;
use std::sync::Mutex;
use trainer::db::{AttemptRow, Context, Db, LeakReport, Summary};
use trainer::drill::{Mode, Step};
use trainer::import;
use trainer::ranges::{self, RangeTask};
use trainer::{DrillView, Engine, Feedback, Filters};

struct AppState {
    engine: Mutex<Engine>,
    db: Mutex<Db>,
    analyzer: Mutex<import::Analyzer>,
    jobs: std::sync::mpsc::Sender<import::PostJob>,
    import_progress: std::sync::Arc<Mutex<ImportProgress>>,
}

#[derive(Serialize, Clone, Default)]
struct ImportProgress {
    queued: usize,
    done: usize,
    failed: usize,
    current: Option<String>,
    last_seconds: f64,
}

fn data_dir() -> PathBuf {
    std::env::var_os("POKER_TRAINER_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data")))
}

#[derive(Serialize)]
struct Meta {
    spots: usize,
    stacks: Vec<u32>,
    matchups: Vec<String>,
    pot_types: Vec<String>,
    textures: Vec<String>,
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[tauri::command]
fn meta(state: tauri::State<AppState>) -> Meta {
    let mut e = state.engine.lock().unwrap();
    e.refresh_spots();
    eprintln!("[ui] page loaded, {} solved spots", e.spots().len());
    let s = e.spots();
    let mut m = Meta { spots: s.len(), stacks: vec![], matchups: vec![], pot_types: vec![], textures: vec![] };
    for x in s {
        if !m.stacks.contains(&x.stack) { m.stacks.push(x.stack); }
        if !m.matchups.contains(&x.matchup) { m.matchups.push(x.matchup.clone()); }
        if !m.pot_types.contains(&x.pot_type) { m.pot_types.push(x.pot_type.clone()); }
    }
    m.stacks.sort();
    m.textures = ["A-high", "K/Q-high", "middle", "low", "paired", "monotone", "two-tone", "rainbow", "connected", "dry"].iter().map(|s| s.to_string()).collect();
    m
}

#[tauri::command]
fn new_drill(state: tauri::State<AppState>, filters: Filters, mode: String, game_mode: Option<Mode>) -> Result<DrillView, String> {
    let mut f = filters;
    if mode == "leaks" {
        let db = state.db.lock().unwrap();
        match db.worst_category(5).map_err(err)? {
            Some((street, matchup, pot_type, _node)) => {
                f = Filters { streets: vec![street], matchups: vec![matchup], pot_types: vec![pot_type], ..f };
            }
            None => return Err("Not enough history yet: do at least 5 drills in a spot type first.".into()),
        }
    }
    let r = state.engine.lock().unwrap().new_drill(&f, game_mode.unwrap_or_default()).map_err(err);
    match &r {
        Ok(v) => eprintln!("[ui] drill dealt: {} {} board {}", v.spot_id, v.hand.join(""), v.board.join("")),
        Err(e) => eprintln!("[ui] drill error: {e}"),
    }
    r
}

#[tauri::command]
fn answer(state: tauri::State<AppState>, index: usize, rng: Option<u32>) -> Result<Feedback, String> {
    let mut e = state.engine.lock().unwrap();
    let view = e.current().cloned().ok_or("no drill in progress")?;
    let (history, hero_cards) = e.current_context().ok_or("no drill in progress")?;
    let fb = e.answer(index, rng).map_err(err)?;
    let ctx = Context { source: "drill".into(), opponent: None, history, hero_cards: Some(hero_cards) };
    state.db.lock().unwrap().record(&view, &fb, &ctx).map_err(err)?;
    Ok(fb)
}

#[tauri::command]
fn continue_hand(state: tauri::State<AppState>) -> Result<Step, String> {
    state.engine.lock().unwrap().continue_hand().map_err(err)
}

#[tauri::command]
fn summary(state: tauri::State<AppState>) -> Result<Summary, String> {
    state.db.lock().unwrap().summary().map_err(err)
}

#[tauri::command]
fn leaks(state: tauri::State<AppState>, source: Option<String>, opponent: Option<String>) -> Result<LeakReport, String> {
    state.db.lock().unwrap().leaks(source.as_deref(), opponent.as_deref()).map_err(err)
}

#[tauri::command]
fn attempts(state: tauri::State<AppState>, stat: Option<String>, source: Option<String>, opponent: Option<String>) -> Result<Vec<AttemptRow>, String> {
    state.db.lock().unwrap().attempts(stat.as_deref(), source.as_deref(), opponent.as_deref(), 200).map_err(err)
}

#[tauri::command]
fn redrill(state: tauri::State<AppState>, id: i64) -> Result<DrillView, String> {
    let info = state.db.lock().unwrap().redrill_info(id).map_err(err)?;
    let (spot, hist, cards) = info.ok_or("this decision can't be re-drilled (river spots are solved live)")?;
    state.engine.lock().unwrap().redrill(&spot, &hist, &cards).map_err(err)
}

#[derive(Serialize)]
struct Preview {
    hands: usize,
    with_cards: usize,
    players: Vec<(String, usize)>,
    hero: Option<String>,
    aliases: std::collections::HashMap<String, String>,
    already: usize,
}

#[tauri::command]
fn import_preview(state: tauri::State<AppState>, text: String) -> Result<Preview, String> {
    let hands = import::parse(&text).map_err(err)?;
    let db = state.db.lock().unwrap();
    let already = hands.iter().filter(|h| import::hand_exists(&db, &h.id).unwrap_or(false)).count();
    let hero: Option<String> = db.conn().query_row("SELECT hero FROM import_hand ORDER BY imported_at DESC LIMIT 1", [], |r| r.get(0)).ok();
    Ok(Preview {
        hands: hands.len(),
        with_cards: hands.iter().filter(|h| h.hero_cards.is_some()).count(),
        players: import::players(&hands),
        hero,
        aliases: import::aliases(&db).map_err(err)?,
        already,
    })
}

#[derive(Serialize)]
struct ImportResult {
    imported: usize,
    skipped_existing: usize,
    preflop_decisions: usize,
    postflop_queued: usize,
}

#[tauri::command]
fn import_run(state: tauri::State<AppState>, text: String, file: String, hero: String, aliases: std::collections::HashMap<String, String>) -> Result<ImportResult, String> {
    let hands = import::parse(&text).map_err(err)?;
    let db = state.db.lock().unwrap();
    for (k, v) in &aliases {
        import::set_alias(&db, k, v).map_err(err)?;
    }
    let names = import::aliases(&db).map_err(err)?;
    let mut an = state.analyzer.lock().unwrap();
    let mut res = ImportResult { imported: 0, skipped_existing: 0, preflop_decisions: 0, postflop_queued: 0 };
    for h in &hands {
        if import::hand_exists(&db, &h.id).map_err(err)? {
            res.skipped_existing += 1;
            continue;
        }
        let (graded, status, note, job) = an.preflop(h, &hero, &names).map_err(err)?;
        let villain = job.as_ref().map(|j| j.villain_display.clone()).unwrap_or_default();
        let pot_type = job.as_ref().map(|j| j.info.pot_type.clone()).unwrap_or_default();
        for g in &graded {
            import::save_graded(&db, g, None).map_err(err)?;
        }
        import::save_hand(&db, &file, h, &hero, &import::position_of(h, &hero), &villain, &pot_type, &status, &note, &graded).map_err(err)?;
        res.imported += 1;
        res.preflop_decisions += graded.len();
        if let Some(j) = job {
            import::save_job(&db, &j).map_err(err)?;
            state.import_progress.lock().unwrap().queued += 1;
            let _ = state.jobs.send(j);
            res.postflop_queued += 1;
        }
    }
    Ok(res)
}

#[tauri::command]
fn import_status(state: tauri::State<AppState>) -> ImportProgress {
    state.import_progress.lock().unwrap().clone()
}

#[tauri::command]
fn import_hands(state: tauri::State<AppState>) -> Result<Vec<import::HandRow>, String> {
    import::list_hands(&state.db.lock().unwrap()).map_err(err)
}

#[tauri::command]
fn import_hand_detail(state: tauri::State<AppState>, hand_id: String) -> Result<Vec<import::DecisionRow>, String> {
    import::hand_decisions(&state.db.lock().unwrap(), &hand_id).map_err(err)
}

#[tauri::command]
fn range_task(
    state: tauri::State<AppState>,
    kind: String,
    table: Option<usize>,
    stack: u32,
    scenario: Option<String>,
    matchup: Option<String>,
    pot_type: Option<String>,
    who: Option<String>,
) -> Result<RangeTask, String> {
    if kind == "preflop" {
        let mut an = state.analyzer.lock().unwrap();
        return ranges::preflop_task(&mut an, table.unwrap_or(3), stack, scenario.as_deref().unwrap_or("open:BTN")).map_err(err);
    }
    let mut e = state.engine.lock().unwrap();
    let pool: Vec<String> = e
        .spots()
        .iter()
        .filter(|s| s.stack == stack && matchup.as_ref().is_none_or(|m| &s.matchup == m) && pot_type.as_ref().is_none_or(|p| &s.pot_type == p))
        .map(|s| s.id.clone())
        .collect();
    if pool.is_empty() {
        return Err("no solved flops for that choice yet".into());
    }
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0) as usize;
    let id = pool[nanos % pool.len()].clone();
    ranges::flop_task(&mut e, &id, who.as_deref().unwrap_or("oop")).map_err(err)
}

/// Background worker: re-solves queued hands one at a time and stores the grades.
fn start_worker(data: PathBuf, rx: std::sync::mpsc::Receiver<import::PostJob>, progress: std::sync::Arc<Mutex<ImportProgress>>) {
    std::thread::spawn(move || {
        let Ok(db) = Db::open(&data.join("trainer.sqlite")) else { return };
        for job in rx {
            progress.lock().unwrap().current = Some(format!("{} vs {}", job.info.matchup, job.villain_display));
            let t = std::time::Instant::now();
            let res = import::analyze_postflop(&job, 4.0);
            let mut p = progress.lock().unwrap();
            p.last_seconds = t.elapsed().as_secs_f64();
            p.current = None;
            match res {
                Ok((graded, note)) => {
                    for g in &graded {
                        let _ = import::save_graded(&db, g, Some(&job.villain_display));
                    }
                    let status = if note.is_empty() { "scored" } else { "partial" };
                    let _ = import::finish_hand(&db, &job.hand_id, status, &note, &graded);
                    p.done += 1;
                }
                Err(e) => {
                    let _ = import::finish_hand(&db, &job.hand_id, "preflop only", &format!("postflop: {e}"), &[]);
                    p.failed += 1;
                }
            }
            eprintln!("[import] hand {} done in {:.0}s", job.hand_id, p.last_seconds);
        }
    });
}

fn main() {
    let data = data_dir();
    let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(7);
    let db = Db::open(&data.join("trainer.sqlite")).expect("open database");
    let (tx, rx) = std::sync::mpsc::channel();
    let progress = std::sync::Arc::new(Mutex::new(ImportProgress::default()));
    start_worker(data.clone(), rx, progress.clone());
    // hands queued in an earlier session that never got their postflop pass
    if let Ok(jobs) = import::pending_jobs(&db) {
        progress.lock().unwrap().queued += jobs.len();
        for j in jobs {
            let _ = tx.send(j);
        }
    }
    tauri::Builder::default()
        .manage(AppState {
            engine: Mutex::new(Engine::new(&data, seed)),
            db: Mutex::new(db),
            analyzer: Mutex::new(import::Analyzer::new(&data)),
            jobs: tx,
            import_progress: progress,
        })
        .invoke_handler(tauri::generate_handler![meta, new_drill, answer, continue_hand, summary, leaks, attempts, redrill, import_preview, import_run, import_status, import_hands, import_hand_detail, range_task])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
