//! Background batch runner with a local dashboard (http://localhost:8790):
//! progress, hours left, pause/resume, speed (100/50/25%), stop.

use crate::{gb, read_json, sol_dir, write_json, Job};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use solver_api::Control;
use std::collections::{BTreeMap, VecDeque};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Full-speed seconds per GB of solver memory, used until real solves are measured.
const DEFAULT_RATE: f64 = 95.0;

#[derive(Serialize, Deserialize, Clone)]
pub struct Controls {
    pub paused: bool,
    pub speed: f64,
}

#[derive(Serialize, Clone)]
struct Running {
    id: String,
    group: String,
    mem_gb: f64,
    iteration: u32,
    exploitability_pct: f64,
    target_pct: f64,
    progress: f64,
    #[serde(skip)]
    first_expl: Option<f64>,
}

#[derive(Serialize)]
struct State {
    controls: Controls,
    stopping: bool,
    finished: bool,
    started_unix: u64,
    total_jobs: usize,
    done_jobs: usize,
    failed_jobs: usize,
    percent: f64,
    hours_left: f64,
    running: BTreeMap<String, Running>,
    groups: Vec<(String, usize, usize)>,
    log: VecDeque<String>,
    #[serde(skip)]
    remaining: BTreeMap<String, f64>,
    #[serde(skip)]
    rate: BTreeMap<String, (f64, f64)>,
    #[serde(skip)]
    total_work: f64,
}

fn rate_key(j: &Job) -> String {
    format!("{}bb-{}", j.stack, j.pot_type)
}

fn now_unix() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

impl State {
    fn rate(&self, key: &str) -> f64 {
        if let Some(&(s, g)) = self.rate.get(key) {
            if g > 0.0 {
                return s / g;
            }
        }
        let (s, g) = self.rate.values().fold((0.0, 0.0), |a, b| (a.0 + b.0, a.1 + b.1));
        if g > 0.0 { s / g } else { DEFAULT_RATE }
    }

    fn recompute(&mut self) {
        let mut left: f64 = self.remaining.iter().map(|(k, gbs)| gbs * self.rate(k)).sum();
        for r in self.running.values() {
            left -= r.progress * r.mem_gb * self.rate(&r.group);
        }
        let left = left.max(0.0);
        self.hours_left = left / self.controls.speed.max(0.05) / 3600.0;
        self.percent = if self.total_work > 0.0 { 100.0 * (1.0 - left / self.total_work) } else { 100.0 };
    }

    fn note(&mut self, s: String) {
        let t = chrono_like_time();
        self.log.push_front(format!("{t}  {s}"));
        self.log.truncate(60);
    }
}

fn chrono_like_time() -> String {
    let out = std::process::Command::new("date").arg("+%H:%M").output();
    out.map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default()
}

struct JobCtl {
    id: String,
    state: Arc<Mutex<State>>,
}

impl Control for JobCtl {
    fn checkpoint(&self, iteration: u32, expl: f64, step: f64) -> bool {
        {
            let mut st = self.state.lock().unwrap();
            if let Some(r) = st.running.get_mut(&self.id) {
                r.iteration = iteration;
                r.exploitability_pct = expl;
                let first = *r.first_expl.get_or_insert(expl.max(r.target_pct * 1.01));
                // exploitability falls roughly geometrically: progress on a log scale
                let p = ((first / expl.max(1e-9)).ln() / (first / r.target_pct).ln()).clamp(0.0, 0.99);
                r.progress = r.progress.max(p);
            }
            st.recompute();
        }
        // throttle, then wait while paused
        let mut sleep = {
            let st = self.state.lock().unwrap();
            if st.controls.speed < 0.99 { step * (1.0 / st.controls.speed - 1.0) } else { 0.0 }
        };
        loop {
            {
                let st = self.state.lock().unwrap();
                if st.stopping {
                    return false;
                }
                if !st.controls.paused && sleep <= 0.0 {
                    return true;
                }
            }
            let chunk = if sleep > 0.0 { sleep.min(0.25) } else { 0.25 };
            std::thread::sleep(Duration::from_secs_f64(chunk));
            sleep -= chunk;
        }
    }
}

fn controls_path(data: &Path) -> PathBuf {
    data.join("run-control.json")
}

fn save_controls(data: &Path, c: &Controls) {
    let _ = write_json(&controls_path(data), c);
}

pub struct Scope {
    pub only: Option<String>,
    pub ids: Option<std::collections::HashSet<String>>,
    /// stop starting new spots after this many minutes; abort running ones 25 minutes later
    pub max_minutes: Option<f64>,
    /// exit as soon as everything is done (CI), instead of keeping the dashboard up
    pub exit_when_done: bool,
}

pub fn run(data: &Path, limit: Option<usize>, mem_budget: f64, scope: Scope, port: u16) -> Result<()> {
    let only = scope.only.clone();
    let t_start = Instant::now();
    let soft = scope.max_minutes.map(|m| Duration::from_secs_f64(m * 60.0));
    let jobs: Vec<Job> = read_json(&data.join("plan.json"))?;
    let dir = sol_dir(data);
    std::fs::create_dir_all(&dir)?;
    for e in std::fs::read_dir(&dir)?.flatten() {
        if e.path().extension().is_some_and(|x| x == "partial") {
            let _ = std::fs::remove_file(e.path());
        }
    }
    let done = |j: &Job| dir.join(format!("{}.json", j.spot.id)).exists();
    let gbm = |j: &Job| gb(j.memory_bytes.unwrap_or(1 << 30));

    // measured rates from finished solves
    let mut rate: BTreeMap<String, (f64, f64)> = BTreeMap::new();
    let mut groups: Vec<(String, usize, usize)> = Vec::new();
    let mut done_work_gb: BTreeMap<String, f64> = BTreeMap::new();
    for j in &jobs {
        let name = format!("{}bb {} {}", j.stack, j.matchup, j.pot_type);
        if groups.last().map_or(true, |g| g.0 != name) {
            groups.push((name, 0, 0));
        }
        let g = groups.last_mut().unwrap();
        g.2 += 1;
        if done(j) {
            g.1 += 1;
            *done_work_gb.entry(rate_key(j)).or_default() += gbm(j);
            if let Ok(v) = read_json::<serde_json::Value>(&dir.join(format!("{}.json", j.spot.id))) {
                let r = &v["report"];
                let secs = r["compute_seconds"].as_f64().filter(|&x| x > 0.0).or(r["seconds"].as_f64()).unwrap_or(0.0);
                let mem = r["memory_bytes"].as_u64().map(gb).unwrap_or(0.0);
                let e = rate.entry(rate_key(j)).or_default();
                e.0 += secs;
                e.1 += mem;
            }
        }
    }
    let mut pending: Vec<Job> = jobs
        .iter()
        .filter(|j| !done(j))
        .filter(|j| only.as_ref().map_or(true, |o| j.spot.id.contains(o.as_str())))
        .filter(|j| scope.ids.as_ref().map_or(true, |ids| ids.contains(&j.spot.id)))
        .cloned()
        .collect();
    if let Some(l) = limit {
        pending.truncate(l);
    }
    let mut remaining: BTreeMap<String, f64> = BTreeMap::new();
    for j in &pending {
        *remaining.entry(rate_key(j)).or_default() += gbm(j);
    }
    let controls: Controls = read_json(&controls_path(data)).unwrap_or(Controls { paused: false, speed: 1.0 });
    let mut st = State {
        controls,
        stopping: false,
        finished: false,
        started_unix: now_unix(),
        total_jobs: jobs.len(),
        done_jobs: jobs.iter().filter(|j| done(j)).count(),
        failed_jobs: 0,
        percent: 0.0,
        hours_left: 0.0,
        running: BTreeMap::new(),
        groups,
        log: VecDeque::new(),
        remaining,
        rate,
        total_work: 0.0,
    };
    let pending_work: f64 = st.remaining.iter().map(|(k, g)| g * st.rate(k)).sum();
    st.total_work = pending_work + done_work_gb.iter().map(|(k, g)| g * st.rate(k)).sum::<f64>();
    st.recompute();
    st.note(format!("started: {} spots to solve, about {:.1} h at full speed", pending.len(), pending_work / 3600.0));
    let state = Arc::new(Mutex::new(st));

    serve(state.clone(), data.to_path_buf(), port);
    println!("dashboard: http://localhost:{port}");
    let caffeinate = Arc::new(Mutex::new(None::<std::process::Child>));
    if let Some(soft) = soft {
        let state = state.clone();
        std::thread::spawn(move || {
            std::thread::sleep(soft + Duration::from_secs(25 * 60));
            let mut st = state.lock().unwrap();
            st.stopping = true;
            st.note("hard time limit: aborting running spots".into());
        });
    }
    {
        let (state, caffeinate) = (state.clone(), caffeinate.clone());
        std::thread::spawn(move || loop {
            keep_awake(&state, &caffeinate);
            std::thread::sleep(Duration::from_secs(1));
        });
    }

    let budget = mem_budget;
    let mut in_use = 0.0f64;
    let in_use_shared = Arc::new(Mutex::new(0.0f64));
    let mut handles = Vec::new();
    for j in pending {
        if soft.is_some_and(|d| t_start.elapsed() > d) {
            state.lock().unwrap().note("time limit reached: not starting more spots".into());
            break;
        }
        let need = gbm(&j);
        loop {
            {
                let st = state.lock().unwrap();
                if st.stopping {
                    break;
                }
                in_use = *in_use_shared.lock().unwrap();
                if !st.controls.paused && (in_use == 0.0 || in_use + need <= budget) {
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        if state.lock().unwrap().stopping {
            break;
        }
        *in_use_shared.lock().unwrap() += need;
        let group = rate_key(&j);
        state.lock().unwrap().running.insert(
            j.spot.id.clone(),
            Running { id: j.spot.id.clone(), group, mem_gb: need, iteration: 0, exploitability_pct: 0.0, target_pct: j.spot.target_exploitability_pct, progress: 0.0, first_expl: None },
        );
        let (state2, used, data2) = (state.clone(), in_use_shared.clone(), data.to_path_buf());
        handles.push(std::thread::spawn(move || {
            let ctl = JobCtl { id: j.spot.id.clone(), state: state2.clone() };
            let res = crate::run_one(&data2, &j, &ctl);
            *used.lock().unwrap() -= need;
            let mut st = state2.lock().unwrap();
            st.running.remove(&j.spot.id);
            match res {
                Ok(r) => {
                    let key = rate_key(&j);
                    *st.remaining.entry(key.clone()).or_default() -= need;
                    let e = st.rate.entry(key).or_default();
                    e.0 += r.compute_seconds;
                    e.1 += gb(r.memory_bytes);
                    st.done_jobs += 1;
                    let g = format!("{}bb {} {}", j.stack, j.matchup, j.pot_type);
                    if let Some(x) = st.groups.iter_mut().find(|x| x.0 == g) {
                        x.1 += 1;
                    }
                    st.note(format!("solved {} in {:.0}s ({:.2}% expl)", j.spot.id, r.seconds, r.exploitability_pct));
                }
                Err(e) if e.to_string() == "aborted" => {}
                Err(e) => {
                    st.failed_jobs += 1;
                    st.note(format!("FAILED {}: {e:#}", j.spot.id));
                }
            }
            st.recompute();
        }));
        handles.retain(|h| !h.is_finished());
    }
    let _ = in_use;
    for h in handles {
        let _ = h.join();
    }
    {
        let mut st = state.lock().unwrap();
        if !st.stopping {
            st.finished = true;
            st.note("all jobs finished".into());
        }
        st.recompute();
    }
    state.lock().unwrap().controls.paused = true; // lets the watcher release the Mac
    std::thread::sleep(Duration::from_millis(1500));
    // keep the dashboard up for a while after finishing so the result can be seen
    let until = Instant::now() + Duration::from_secs(if scope.exit_when_done { 0 } else { 1800 });
    while Instant::now() < until && !state.lock().unwrap().stopping {
        std::thread::sleep(Duration::from_secs(1));
    }
    Ok(())
}

/// Mac stays awake while solving, may sleep while paused.
fn keep_awake(state: &Arc<Mutex<State>>, child: &Arc<Mutex<Option<std::process::Child>>>) {
    let want = {
        let st = state.lock().unwrap();
        !st.controls.paused && !st.stopping
    };
    let mut c = child.lock().unwrap();
    match (want, c.is_some()) {
        (true, false) => {
            *c = std::process::Command::new("/usr/bin/caffeinate")
                .args(["-i", "-w", &std::process::id().to_string()])
                .spawn()
                .ok();
        }
        (false, true) => {
            if let Some(mut ch) = c.take() {
                let _ = ch.kill();
            }
        }
        _ => {}
    }
}

const PAGE: &str = include_str!("dashboard.html");

fn serve(state: Arc<Mutex<State>>, data: PathBuf, port: u16) {
    std::thread::spawn(move || {
        let Ok(listener) = std::net::TcpListener::bind(("127.0.0.1", port)) else {
            eprintln!("dashboard: port {port} busy; solving continues without it");
            return;
        };
        for stream in listener.incoming().flatten() {
            let mut s = stream;
            let _ = s.set_read_timeout(Some(Duration::from_secs(2)));
            let mut buf = [0u8; 4096];
            let n = s.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]);
            let mut parts = req.split_whitespace();
            let (method, path) = (parts.next().unwrap_or(""), parts.next().unwrap_or("/"));
            let (ctype, body) = match (method, path) {
                ("GET", "/") => ("text/html; charset=utf-8", PAGE.to_string()),
                ("GET", "/api/status") => {
                    let st = state.lock().unwrap();
                    ("application/json", serde_json::to_string(&*st).unwrap_or_default())
                }
                ("POST", p) => {
                    let mut st = state.lock().unwrap();
                    match p {
                        "/api/pause" => {
                            st.controls.paused = true;
                            st.note("paused".into());
                        }
                        "/api/resume" => {
                            st.controls.paused = false;
                            st.note("resumed".into());
                        }
                        "/api/stop" => {
                            st.stopping = true;
                            st.note("stopping: running jobs discarded, finished jobs kept".into());
                        }
                        p if p.starts_with("/api/speed/") => {
                            if let Ok(v) = p.trim_start_matches("/api/speed/").parse::<f64>() {
                                st.controls.speed = (v / 100.0).clamp(0.1, 1.0);
                                let pct = (st.controls.speed * 100.0).round();
                                st.note(format!("speed {pct}%"));
                            }
                        }
                        _ => {}
                    }
                    save_controls(&data, &st.controls);
                    st.recompute();
                    ("application/json", "{}".to_string())
                }
                _ => ("text/plain", "not found".to_string()),
            };
            let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}", body.len());
        }
    });
}
