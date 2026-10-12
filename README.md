# Poker Trainer

A personal Mac app for 40-50bb no-rake NLHE home games: solver-graded drills plus a model of your own leaks. Design: [docs/DESIGN.md](docs/DESIGN.md). Preflop model: [docs/PREFLOP.md](docs/PREFLOP.md).

## Status

- Phase 0 (design): done.
- Milestone 1 (solver pipeline): preflop solver, flop clustering and the batch CLI are built; the full overnight batch hasn't been run yet.

## Setup

```sh
brew install rustup && rustup default stable
export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH"
cargo build --release -p solve-cli
```

`postflop-solver` is pinned to `bincode 2.0.0-rc.3` in `crates/solver-pfs/Cargo.toml`; newer bincode breaks it.

## Background solver (easiest)

Double-click `Solver.command` (or `scripts/solver.sh start`). It solves in the background and opens a dashboard at http://localhost:8790 with % done, hours left, Pause/Resume, speed (Full, Half, Quarter) and Stop. The Mac stays awake only while it's solving. Finished spots are kept; Stop or a restart resumes where it left off. `scripts/solver.sh open|stop|status|log` for the rest.

Batch: 50bb, 100bb and 40bb (in that order) x BTN vs BB, SB vs BB, CO vs BTN x single-raised and 3-bet pots; 75 flops (40 at 100bb). 1,140 spots, about 45 hours of full-speed compute, about 11 GB.

## Batch solver (manual)

Run from the project root; the config is `configs/m1.toml`.

```sh
./target/release/solve ranges             # preflop solves -> data/ranges/*.txt|json
./target/release/solve flops              # representative flops -> data/flops-75.json, flops-40.json
./target/release/solve plan               # 1,140 jobs -> data/plan.json
./target/release/solve estimate --pilot 1 # memory per job + timed pilots -> hours and disk
./target/release/solve run                # resumable, dashboard on :8790; Ctrl-C and rerun to continue
./target/release/solve run --only 40bb-BTNvBB-srp --limit 10
./target/release/solve status
```

`run` keeps total solver memory under `--mem-budget` GB (default 9), so small 3-bet pot solves run several at a time while big single-raised pots run alone. A job is done when `data/solutions/<id>.json` exists; interrupted `.partial` files are removed on the next run. Keep the Mac awake for long batches (`caffeinate -i ./target/release/solve run`).

## Scripts

`scripts/pokernow_stats.py <log.csv> "<your name>"`: per-player stats, deal randomness checks, and your river and preflop habits from a PokerNow log.

## Tests

```sh
cargo test --release
```

## License note

`postflop-solver` is AGPL-3.0. Fine for personal offline use; distributing the app or running it as a service would require releasing its source under AGPL too.

## License

AGPL-3.0 (see LICENSE), because it links postflop-solver, which is AGPL-3.0.
