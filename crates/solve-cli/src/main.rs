//! Batch solver CLI for Milestone 1.
//!
//!   solve ranges   -> preflop solves, data/ranges/*.json + *.txt
//!   solve flops    -> representative flops, data/flops.json
//!   solve plan     -> job list, data/plan.json
//!   solve estimate -> memory per job + timed pilots, projected hours and disk
//!   solve run      -> resumable, memory-budgeted parallel batch into data/solutions/
//!   solve status   -> progress

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use poker_core::equity::EquityTable;
use poker_core::flops::{cluster, Cluster};
use preflop::{PotType, PostflopSpot};
use serde::{Deserialize, Serialize};
use solver_api::{BetSizes, Solver, SpotConfig};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

mod runner;

#[derive(Parser)]
#[command(about = "Poker trainer batch solver")]
struct Cli {
    /// project data directory
    #[arg(long, default_value = "data")]
    data: PathBuf,
    /// batch config
    #[arg(long, default_value = "configs/m1.toml")]
    config: PathBuf,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    Ranges,
    Flops,
    Plan,
    Estimate {
        /// pilot solves per pot type, used to measure seconds per GB
        #[arg(long, default_value_t = 1)]
        pilot: usize,
    },
    Run {
        /// stop after this many jobs
        #[arg(long)]
        limit: Option<usize>,
        /// total solver memory allowed at once, GB
        #[arg(long, default_value_t = 9.0)]
        mem_budget: f64,
        /// only jobs whose id contains this text
        #[arg(long)]
        only: Option<String>,
        /// dashboard port (http://localhost:PORT)
        #[arg(long, default_value_t = 8790)]
        port: u16,
    },
    Status,
    /// CI: solve one shard of the remaining spots (balanced by memory)
    Shard {
        #[arg(long)]
        index: usize,
        #[arg(long)]
        of: usize,
        #[arg(long, default_value_t = 320.0)]
        max_minutes: f64,
        /// ids already solved elsewhere, one per line
        #[arg(long)]
        skip_file: Option<PathBuf>,
        #[arg(long, default_value_t = 12.0)]
        mem_budget: f64,
    },
}

#[derive(Deserialize, Clone)]
struct Config {
    stacks: Vec<u32>,
    pot_types: Vec<String>,
    flops: usize,
    /// fewer representative flops for deep stacks (>= 80bb), whose trees are much bigger
    #[serde(default)]
    deep_flops: Option<usize>,
    preflop_iterations: u32,
    equity_samples: u32,
    target_exploitability_pct: f64,
    max_iterations: u32,
    matchup: Vec<Matchup>,
}

#[derive(Deserialize, Serialize, Clone)]
struct Matchup {
    table: usize,
    opener: String,
    responder: String,
}

#[derive(Serialize, Deserialize, Clone)]
struct Job {
    spot: SpotConfig,
    matchup: String,
    pot_type: String,
    stack: u32,
    flop_label: String,
    flop_weight: f64,
    #[serde(default)]
    memory_bytes: Option<u64>,
}

fn pot_type(s: &str) -> Result<PotType> {
    Ok(match s {
        "srp" => PotType::Srp,
        "3bp" => PotType::ThreeBet,
        x => bail!("unknown pot type {x}"),
    })
}

fn ranges_path(data: &Path, table: usize, stack: u32) -> PathBuf {
    data.join("ranges").join(format!("{table}max-{stack}bb.json"))
}

fn read_json<T: for<'de> Deserialize<'de>>(p: &Path) -> Result<T> {
    serde_json::from_reader(std::io::BufReader::new(std::fs::File::open(p).with_context(|| format!("open {}", p.display()))?))
        .with_context(|| format!("parse {}", p.display()))
}

fn write_json<T: Serialize>(p: &Path, v: &T) -> Result<()> {
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d)?;
    }
    let tmp = p.with_extension("tmp");
    serde_json::to_writer_pretty(std::io::BufWriter::new(std::fs::File::create(&tmp)?), v)?;
    std::fs::rename(tmp, p)?;
    Ok(())
}

fn gb(b: u64) -> f64 {
    b as f64 / 1024f64.powi(3)
}

fn cmd_ranges(cfg: &Config, data: &Path) -> Result<()> {
    let eq = EquityTable::load_or_compute(&data.join("equity169.json"), cfg.equity_samples)?;
    let tables: std::collections::BTreeSet<usize> = cfg.matchup.iter().map(|m| m.table).collect();
    for &table in &tables {
        for &stack in &cfg.stacks {
            let t0 = Instant::now();
            let eq2 = EquityTable { samples: eq.samples, eq: eq.eq.clone(), w: eq.w.clone() };
            let mut s = preflop::Solver::new(preflop::Config::new(table, stack as f64), eq2);
            s.solve(cfg.preflop_iterations);
            let mut out: BTreeMap<String, PostflopSpot> = BTreeMap::new();
            let mut txt = format!("# Preflop ranges, {table}-handed, {stack}bb, no rake\n# Model and limits: docs/PREFLOP.md\n\n");
            let pos = s.cfg.positions();
            for (i, p) in pos.iter().enumerate().take(pos.len() - 1) {
                let f = s.action_freqs(&vec!["fold"; i]).map_err(anyhow::Error::msg)?;
                txt += &format!("{p} first in: open {:.1}%\n", f[1].1 * 100.0);
            }
            for m in cfg.matchup.iter().filter(|m| m.table == table) {
                for pt in &cfg.pot_types {
                    let spot = s.postflop_spot(&m.opener, &m.responder, pot_type(pt)?).map_err(anyhow::Error::msg)?;
                    txt += &format!(
                        "\n## {} vs {} {pt}: pot {}bb, {}bb behind\n{} (OOP, {:.1}% of hands): {}\n{} (IP, {:.1}% of hands): {}\n",
                        m.opener, m.responder, spot.pot_bb, spot.stack_bb, spot.oop, spot.oop_width * 100.0, spot.oop_range, spot.ip, spot.ip_width * 100.0, spot.ip_range
                    );
                    out.insert(format!("{}v{}-{pt}", m.opener, m.responder), spot);
                }
            }
            let p = ranges_path(data, table, stack);
            write_json(&p, &out)?;
            std::fs::write(p.with_extension("txt"), txt)?;
            println!("{table}-handed {stack}bb: {} spots in {:.1}s -> {}", out.len(), t0.elapsed().as_secs_f64(), p.display());
        }
    }
    Ok(())
}

fn flops_file(data: &Path, k: usize) -> PathBuf {
    data.join(format!("flops-{k}.json"))
}

fn flops_for(cfg: &Config, stack: u32) -> usize {
    if stack >= 80 { cfg.deep_flops.unwrap_or(cfg.flops) } else { cfg.flops }
}

fn cmd_flops(cfg: &Config, data: &Path) -> Result<()> {
    if let Some(k) = cfg.deep_flops {
        write_json(&flops_file(data, k), &cluster(k))?;
        println!("{k} deep-stack flops -> {}", flops_file(data, k).display());
    }
    let cl = cluster(cfg.flops);
    write_json(&flops_file(data, cfg.flops), &cl)?;
    let mut by: BTreeMap<String, (usize, f64)> = BTreeMap::new();
    for c in &cl {
        let e = by.entry(format!("{:?}", c.stratum)).or_default();
        e.0 += 1;
        e.1 += c.weight;
    }
    println!("{} representative flops covering all 1,755 (weights = share of real flops):", cl.len());
    for (k, (n, w)) in by {
        println!("  {k:14} {n:3} flops  {:5.1}% of deals", w * 100.0);
    }
    Ok(())
}

fn cmd_plan(cfg: &Config, data: &Path) -> Result<()> {
    let mut jobs = Vec::new();
    for &stack in &cfg.stacks {
        let flops: Vec<Cluster> = read_json(&flops_file(data, flops_for(cfg, stack)))?;
        for m in &cfg.matchup {
            let ranges: BTreeMap<String, PostflopSpot> = read_json(&ranges_path(data, m.table, stack))?;
            for pt in &cfg.pot_types {
                let key = format!("{}v{}-{pt}", m.opener, m.responder);
                let r = ranges.get(&key).with_context(|| format!("missing {key}; rerun `solve ranges`"))?;
                for f in &flops {
                    let id = format!("{stack}bb-{key}-{}", f.rep);
                    jobs.push(Job {
                        spot: SpotConfig {
                            id,
                            oop_range: r.oop_range.clone(),
                            ip_range: r.ip_range.clone(),
                            flop: f.rep.clone(),
                            pot_bb: r.pot_bb,
                            stack_bb: r.stack_bb,
                            sizes: BetSizes::for_spot(pt == "srp", r.stack_bb / r.pot_bb),
                            store_river: false,
                            target_exploitability_pct: cfg.target_exploitability_pct,
                            max_iterations: cfg.max_iterations,
                        },
                        matchup: format!("{} vs {}", m.opener, m.responder),
                        pot_type: pt.clone(),
                        stack,
                        flop_label: f.label.clone(),
                        flop_weight: f.weight,
                        memory_bytes: None,
                    });
                }
            }
        }
    }
    write_json(&data.join("plan.json"), &jobs)?;
    println!("{} jobs ({} stacks x {} matchups x {} pot types; {} flops, {} at 80bb+) -> data/plan.json", jobs.len(), cfg.stacks.len(), cfg.matchup.len(), cfg.pot_types.len(), cfg.flops, cfg.deep_flops.unwrap_or(cfg.flops));
    Ok(())
}

fn sol_dir(data: &Path) -> PathBuf {
    data.join("solutions")
}

fn is_done(data: &Path, id: &str) -> bool {
    sol_dir(data).join(format!("{id}.json")).exists()
}

fn cmd_estimate(data: &Path, pilot: usize) -> Result<()> {
    let mut jobs: Vec<Job> = read_json(&data.join("plan.json"))?;
    let t0 = Instant::now();
    for j in jobs.iter_mut().filter(|j| j.memory_bytes.is_none()) {
        j.memory_bytes = Some(solver_pfs::Pfs.estimate(&j.spot)?.memory_bytes);
    }
    write_json(&data.join("plan.json"), &jobs)?;
    println!("memory estimated for {} jobs in {:.0}s", jobs.len(), t0.elapsed().as_secs_f64());
    std::fs::create_dir_all(sol_dir(data))?;

    let mut rows = Vec::new();
    let mut keys: Vec<String> = jobs.iter().map(|j| format!("{}bb-{}", j.stack, j.pot_type)).collect();
    let mut seen = std::collections::HashSet::new();
    keys.retain(|k| seen.insert(k.clone()));
    for key in keys {
        let group: Vec<&Job> = jobs.iter().filter(|j| format!("{}bb-{}", j.stack, j.pot_type) == key).collect();
        let mem: u64 = group.iter().map(|j| j.memory_bytes.unwrap()).sum();
        let max = group.iter().map(|j| j.memory_bytes.unwrap()).max().unwrap();
        // pilot: the job closest to the median memory, solved for real (kept as a finished job)
        let mut sorted = group.clone();
        sorted.sort_by_key(|j| j.memory_bytes);
        let mut secs_per_gb = Vec::new();
        let mut file_per_gb = Vec::new();
        for k in 0..pilot.min(sorted.len()) {
            let j = sorted[(sorted.len() / 2 + k).min(sorted.len() - 1)];
            let rep = run_one(data, j, &solver_api::NoControl)?;
            println!("  pilot {}: {:.1}s, {:.2} GB, file {:.1} MB, expl {:.2}%", j.spot.id, rep.seconds, gb(rep.memory_bytes), rep.file_bytes as f64 / 1e6, rep.exploitability_pct);
            secs_per_gb.push(rep.seconds / gb(rep.memory_bytes));
            file_per_gb.push(rep.file_bytes as f64 / gb(rep.memory_bytes));
        }
        // without pilots, fall back to the 40bb benchmark rates (95 s and 6.4 MB per GB)
        let spg = if secs_per_gb.is_empty() { 95.0 } else { secs_per_gb.iter().sum::<f64>() / secs_per_gb.len() as f64 };
        let fpg = if file_per_gb.is_empty() { 6.4e6 } else { file_per_gb.iter().sum::<f64>() / file_per_gb.len() as f64 };
        rows.push((key, group.len(), gb(mem), gb(max), spg * gb(mem) / 3600.0, fpg * gb(mem) / 1e9));
    }
    println!("\n{:<10} {:>5} {:>12} {:>11} {:>10} {:>9}", "group", "jobs", "sum mem GB", "max mem GB", "est hours", "disk GB");
    let (mut h, mut d) = (0.0, 0.0);
    for (pt, n, m, mx, hours, disk) in &rows {
        println!("{pt:<10} {n:>5} {m:>12.0} {mx:>11.2} {hours:>10.1} {disk:>9.1}");
        h += hours;
        d += disk;
    }
    println!("total: about {h:.0} hours of solving (one job at a time), about {d:.1} GB on disk");
    println!("(time scales with memory; fanless Air throttles under long load, so treat hours as a floor)");
    Ok(())
}

fn run_one(data: &Path, j: &Job, ctl: &dyn solver_api::Control) -> Result<solver_api::SolveReport> {
    let dir = sol_dir(data);
    std::fs::create_dir_all(&dir)?;
    let out = dir.join(format!("{}.bin", j.spot.id));
    let rep = solver_pfs::Pfs.solve(&j.spot, &out, ctl)?;
    #[derive(Serialize)]
    struct Done<'a> {
        job: &'a Job,
        report: &'a solver_api::SolveReport,
    }
    write_json(&dir.join(format!("{}.json", j.spot.id)), &Done { job: j, report: &rep })?;
    Ok(rep)
}

/// Deterministic shard assignment: unsolved jobs, biggest first, each to the least-loaded shard.
fn shard_ids(data: &Path, index: usize, of: usize, skip: Option<&Path>) -> Result<std::collections::HashSet<String>> {
    let jobs: Vec<Job> = read_json(&data.join("plan.json"))?;
    let skip: std::collections::HashSet<String> = match skip {
        Some(p) => std::fs::read_to_string(p).unwrap_or_default().lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect(),
        None => Default::default(),
    };
    let mut todo: Vec<&Job> = jobs.iter().filter(|j| !skip.contains(&j.spot.id) && !is_done(data, &j.spot.id)).collect();
    todo.sort_by(|a, b| b.memory_bytes.cmp(&a.memory_bytes).then(a.spot.id.cmp(&b.spot.id)));
    let mut load = vec![0u64; of];
    let mut mine = std::collections::HashSet::new();
    for j in todo {
        let s = (0..of).min_by_key(|&s| (load[s], s)).unwrap();
        load[s] += j.memory_bytes.unwrap_or(1 << 30);
        if s == index {
            mine.insert(j.spot.id.clone());
        }
    }
    Ok(mine)
}

fn cmd_status(data: &Path) -> Result<()> {
    let jobs: Vec<Job> = read_json(&data.join("plan.json"))?;
    let mut by: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for j in &jobs {
        let e = by.entry(format!("{}bb {} {}", j.stack, j.matchup, j.pot_type)).or_default();
        e.1 += 1;
        if is_done(data, &j.spot.id) {
            e.0 += 1;
        }
    }
    for (k, (d, t)) in &by {
        println!("{k:28} {d:3}/{t}");
    }
    let (d, t): (usize, usize) = by.values().fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1));
    println!("total {d}/{t}");
    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let cfg: Config = toml::from_str(&std::fs::read_to_string(&cli.config).with_context(|| format!("read {}", cli.config.display()))?)?;
    match cli.cmd {
        Cmd::Ranges => cmd_ranges(&cfg, &cli.data),
        Cmd::Flops => cmd_flops(&cfg, &cli.data),
        Cmd::Plan => cmd_plan(&cfg, &cli.data),
        Cmd::Estimate { pilot } => cmd_estimate(&cli.data, pilot),
        Cmd::Run { limit, mem_budget, only, port } => {
            runner::run(&cli.data, limit, mem_budget, runner::Scope { only, ids: None, max_minutes: None, exit_when_done: false }, port)
        }
        Cmd::Shard { index, of, max_minutes, skip_file, mem_budget } => {
            let ids = shard_ids(&cli.data, index, of, skip_file.as_deref())?;
            println!("shard {index}/{of}: {} spots", ids.len());
            runner::run(&cli.data, None, mem_budget, runner::Scope { only: None, ids: Some(ids), max_minutes: Some(max_minutes), exit_when_done: true }, 8790)
        }
        Cmd::Status => cmd_status(&cli.data),
    }
}
