// Poker Trainer desktop app: Tauri commands over the trainer crate.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::Serialize;
use std::path::PathBuf;
use std::sync::Mutex;
use trainer::db::{AttemptRow, Context, Db, LeakReport, Summary};
use trainer::drill::{Mode, Step};
use trainer::{DrillView, Engine, Feedback, Filters};

struct AppState {
    engine: Mutex<Engine>,
    db: Mutex<Db>,
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

fn main() {
    let data = data_dir();
    let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(7);
    let db = Db::open(&data.join("trainer.sqlite")).expect("open database");
    tauri::Builder::default()
        .manage(AppState { engine: Mutex::new(Engine::new(&data, seed)), db: Mutex::new(db) })
        .invoke_handler(tauri::generate_handler![meta, new_drill, answer, continue_hand, summary, leaks, attempts, redrill])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
