//! Index of solved spots, read from the batch's `data/solutions/*.json` reports.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpotInfo {
    pub id: String,
    pub file: PathBuf,
    pub stack: u32,
    pub matchup: String,
    pub pot_type: String,
    pub flop: String,
    pub texture: String,
    pub flop_weight: f64,
    pub oop: String,
    pub ip: String,
    pub preflop_aggressor: String,
    pub pot_bb: f64,
    pub stack_bb: f64,
    pub river_bet: String,
    pub river_raise: String,
}

/// Postflop order SB, BB, then the rest.
fn order(p: &str) -> usize {
    ["SB", "BB", "UTG", "HJ", "CO", "BTN"].iter().position(|x| *x == p).unwrap_or(9)
}

pub fn load(data: &Path) -> Vec<SpotInfo> {
    let dir = data.join("solutions");
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(&dir) else { return out };
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().is_none_or(|x| x != "json") {
            continue;
        }
        let bin = p.with_extension("bin");
        if !bin.exists() {
            continue;
        }
        let Ok(v) = std::fs::read_to_string(&p).map_err(|_| ()).and_then(|s| serde_json::from_str::<serde_json::Value>(&s).map_err(|_| ())) else { continue };
        let j = &v["job"];
        let matchup = j["matchup"].as_str().unwrap_or("").to_string();
        let mut it = matchup.split(" vs ");
        let (opener, responder) = (it.next().unwrap_or("").to_string(), it.next().unwrap_or("").to_string());
        let (oop, ip) = if order(&opener) < order(&responder) { (opener.clone(), responder.clone()) } else { (responder.clone(), opener.clone()) };
        let pot_type = j["pot_type"].as_str().unwrap_or("").to_string();
        out.push(SpotInfo {
            id: j["spot"]["id"].as_str().unwrap_or("").to_string(),
            file: bin,
            stack: j["stack"].as_u64().unwrap_or(0) as u32,
            preflop_aggressor: if pot_type == "srp" { opener } else { responder },
            matchup,
            pot_type,
            flop: j["spot"]["flop"].as_str().unwrap_or("").to_string(),
            texture: j["flop_label"].as_str().unwrap_or("").to_string(),
            flop_weight: j["flop_weight"].as_f64().unwrap_or(0.0),
            oop,
            ip,
            pot_bb: j["spot"]["pot_bb"].as_f64().unwrap_or(0.0),
            stack_bb: j["spot"]["stack_bb"].as_f64().unwrap_or(0.0),
            river_bet: j["spot"]["sizes"]["river_bet"].as_str().unwrap_or("75%, a").to_string(),
            river_raise: j["spot"]["sizes"]["river_raise"].as_str().unwrap_or("a").to_string(),
        });
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}
