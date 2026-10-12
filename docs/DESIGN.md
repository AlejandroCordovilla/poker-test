# Poker Trainer: Design

Status: Phase 0 approved 2026-10-11. Milestone 1 pipeline built (ranges, flops, batch CLI, cost estimate); overnight batch waiting on review.

## Goal

A native-feeling Mac app that trains a player for 40-100bb no-rake NLHE home games (PokerNow today, $0.20/$0.40 and $0.25/$0.50 blinds, $20-25 buy-ins, often 3-handed) with solver-graded drills, and learns his tendencies and leaks over time from both drills and real hands.

## What real hand histories showed

The first imported home-game log (249 hands, mostly 3-handed, 50bb) shaped three choices. Player names and per-player numbers live in a private, uncommitted notes file.

1. **Games are mostly 3-5 handed**, so BTN/SB/BB spots matter far more than early-position 6-max spots. Spot priority follows the logs, not a 6-max default.
2. **Home-game opponents are far from GTO** (one raised 74% of hands preflop, another called 65% and rarely raised). GTO grading finds fundamental leaks, but the money is in exploits, so exploit mode is Milestone 3b, not a someday feature.
3. **The user's own leaks were specific and measurable:** limp-calling out of the small blind, almost never 3-betting, and folding too often to river bets after checking (including made hands). These became tracked stats and drill families:
   - Theory is the target, opponents are the filter: every leak is graded against the solver, and the leak report can be filtered by opponent.
   - River defense and river barrelling get their own stats and drills (fold to river bet after checking, by hand strength and bet size; river barrel frequency after betting flop and turn).
   - Preflop hand-selection drills for 3-handed and 6-max ranges at 40-100bb.

Deal checks on the same log (pocket pairs, suitedness, flop textures, rank and suit spread) were all within about 1 standard deviation of expected.

## Stack

- **Tauri 2** shell, **React + TypeScript** UI, **Rust** core. Tauri gives a small native Mac app (WebKit, no bundled Chromium) and the Rust core can call the solver library directly, without a subprocess.
- **SQLite** via `rusqlite` for all app data (spots, attempts, player model, imports). Solver outputs live in compact binary files on disk, indexed from SQLite.
- Everything offline. No accounts, no network calls at runtime.

Alternatives considered: Electron (heavier, no gain); pure SwiftUI (more native feel, but the solver is Rust and the analytics are easier to test in Rust, so we'd write FFI glue for no benefit).

## Solver choice

Verified on GitHub 2026-10-11:

| Solver | License | Status | Fit |
|---|---|---|---|
| **b-inary/postflop-solver** | AGPL-3.0-or-later | Development suspended Oct 2023 (author went commercial); last commit Jul 2024 | Rust library. Isomorphism, 16-bit compression, rake, `memory_usage()` before allocation, per-node `strategy()` / `expected_values()` / `equity()`, save/load with zstd, storage modes that drop river data |
| bupticybee/TexasSolver | AGPL-3.0; author sells commercial licenses for embedding the code, calling the release binary is allowed | Repo active (pushed 2026-10-08), last release Nov 2021 (x86 Mac build) | C++ console binary, JSON strategy dump, benchmarked against PioSolver |
| noambrown/poker_solver | MIT | Active | River only, not enough |
| GTOpen, DEEPFOLD and several new "GTO trainer" repos | None stated / SEO-style | | Not usable (no license) or untrusted |

**Recommendation: postflop-solver**, behind a `Solver` trait. It compiles on this Mac (Rust 1.99) after pinning `bincode = 2.0.0-rc.3` (the newer `2.0.1` breaks it, as expected for frozen code); the pin lives in `Cargo.lock`, and we vendor a fork if anything else drifts. TexasSolver's console binary is the second adapter, used to cross-check a sample of solves.

AGPL note: fine for a personal, offline app. If the app is ever distributed or offered as a service, its source must be released under AGPL too.

## Measured cost (M5 MacBook Air, 10 cores, 16 GB, fanless)

40bb, BTN open 2.5bb vs BB call (pot 5.5bb, 37.5bb behind), target exploitability 0.5% of pot, compressed memory:

| Tree | Flop | Peak memory | Solve time | Saved file |
|---|---|---|---|---|
| Full: bets 33/75/all-in, raises 3x + all-in, all streets | K♠7♦2♣ | 8.2 GB | 18.7 min | 1.63 GB |
| Full | 9♥8♥6♣ | 4.9 GB | 6.7 min | 1.19 GB |
| 3-bet pot (22.5bb pot, 29bb behind), full sizes | K♠7♦2♣ | 0.1 GB | 2.4 s | 28 MB |
| Slim: raises all-in only, river 75%/all-in, flop+turn saved | K♠7♦2♣ | 2.4 GB | 3.3 min | 15 MB |
| Slim | 9♥8♥6♣ | 1.4 GB | 1.6 min | 8 MB |

The full tree for the whole spec (about 900 solves) would be roughly 280 hours and 1.4 TB, which doesn't fit. The slim tree is about 6x faster and about 100x smaller on disk, at the same accuracy target.

Projected Milestone 1 batch (12 configs x 75 flops = 900 solves): 450 single-raised solves at about 2.5 min = about 19 hours and about 5 GB; 450 3-bet solves at a few seconds each = under 1 hour and under 13 GB (less with flop+turn storage). Total: about 20 hours, so 2-3 overnight runs, about 15-20 GB of disk. 50bb trees run somewhat larger than the 40bb pilot; the CLI re-estimates before each batch.

The plan uses the slim tree for single-raised pots and the full tree for 3-bet pots (cheap at this SPR). River decisions in drills are re-solved on demand from the stored turn node (seconds, since a river subgame is tiny).

The Air is fanless, so long batches throttle. Batches run overnight with a resumable queue, and the CLI prints the estimated total from `memory_usage()` and a timed pilot solve before starting.

## Preflop ranges

No charts are owned, and no free, documented 40-50bb no-rake ranges turned up (searched 2026-10-11; the closest are a $199 unraked 100bb pack and freemium apps with locked exports). Plan:

1. **Milestone 1a: a small preflop solver** (CFR over the 6-max and 3-handed preflop trees at 40 and 50bb, open sizes 2.5bb / 3-bet 9-11bb / all-in). Postflop value is approximated by equity times a realization factor by position (IP vs OOP, SRP vs 3BP). This is the standard simplification, and it's documented in `docs/PREFLOP.md` as an approximation.
2. Sanity-check the output against widely published 100bb opening frequencies (opening ranges should be tighter from early position, wider on the BTN, etc.).
3. Ranges are plain text files in `data/ranges/`, so a purchased chart pack can replace them later with no code change.

Preflop is the weakest link in the pipeline; drills will say so wherever preflop ranges drive the result.

## Architecture

```
poker-trainer/
  crates/
    poker-core/      cards, ranges, hand eval, board isomorphism, flop clustering
    solver-api/      Solver trait + types (SpotConfig, NodeStrategy, NodeEv)
    solver-pfs/      postflop-solver adapter
    solver-texas/    TexasSolver console adapter (cross-check)
    solve-cli/       batch CLI: plan, estimate, run (resumable, parallel), verify
    trainer/         drill engine, scoring, player model, leak stats, SRS, session generator
    importers/       PokerNow CSV parser (+ manual entry), node matching
  app/               Tauri 2 + React + TypeScript
  data/
    ranges/          preflop ranges (text)
    solutions/       solved spots (binary, zstd)
  docs/
  scripts/           one-off analysis (pokernow_stats.py)
```

### Solver interface

```rust
trait Solver {
    fn estimate(&self, spot: &SpotConfig) -> Estimate;           // memory, rough time
    fn solve(&self, spot: &SpotConfig, out: &Path) -> Result<SolveReport>;
    fn open(&self, path: &Path) -> Result<Box<dyn SolvedSpot>>;
}
trait SolvedSpot {
    fn node(&mut self, history: &[Action], board: &[Card]) -> Result<NodeView>;
}
struct NodeView { actions: Vec<Action>, freq: Vec<[f32; N_ACTIONS]> /* per combo */, ev: Vec<f32> /* per combo per action */ }
```

The trainer only sees `SolvedSpot`/`NodeView`, so the solver can be swapped.

### Spot coverage (Milestone 1)

- Stacks: 40bb and 50bb.
- Matchups: BTN vs BB, SB vs BB, CO vs BTN (6-max), in single-raised and 3-bet pots. That's 6 matchups x 2 stacks = 12 configs. BTN vs BB and SB vs BB solve first because the logs are 3-handed.
- Flops: about 75 representative flops chosen from the 1,755 strategically distinct flops (suit isomorphism), clustered by texture: paired, monotone, two-tone, rainbow; A-high, K/Q-high, middle, low; connected vs disconnected. Each real flop maps to its nearest representative by a texture feature vector (high card, pairing, suits, connectivity, range-advantage proxy).
- Bet sizes configurable per street in the spot config; defaults 33% / 75% / all-in.

### Data model (SQLite)

- `spot(id, matchup, pot_type, stack_bb, flop_cluster, flop, file, solver, exploitability)`
- `node(id, spot_id, history, street)`: indexed decision points used by drills
- `attempt(id, node_id, combo, chosen_action, chosen_size, ev_loss_bb, solver_freq, source[drill|import|manual], session_id, ts)`
- `session(id, kind, started, ended, summary)`
- `srs_item(node_id, combo_class, ease, interval, due, lapses)`
- `stat_event(attempt_id, category, opportunity, took_action)`: one row per stat opportunity (c-bet, fold to c-bet, check-raise, probe, etc.) for leak math
- `opponent(id, name, alias)` and `opponent_event(...)`: per-opponent stats from logs (Milestone 3b)
- `import(id, file, site, hands, imported_at)`

## Scoring (Milestone 2)

- EV loss = EV(best action) - EV(chosen action), in bb, for the exact combo at that node.
- Mixed strategies: an action the solver plays under 5% for that combo counts as a mistake even if its EV is close; any other action is graded by EV loss alone. So a 40/60 mix gives zero loss for either action.
- Bet sizes not in the tree are mapped to the nearest size by pot fraction, with a note in the feedback.
- Feedback: solver frequencies, EV per action, and a short plain-English reason generated from rules over the node's features (range advantage, nut advantage, equity bucket, blockers), not from an LLM, so it works offline and is testable.

## Player model and leaks (Milestone 3)

- Stats per category (street x position x pot type x texture): aggression frequency, c-bet frequency and sizing, check-raise, probe, fold to c-bet, fold to raise, call frequency, and average EV loss.
- Each stat is compared with the solver baseline aggregated over the same opportunities.
- Shrinkage: a Beta prior centered on the solver frequency with weight k = 20 pseudo-opportunities, so 6 hands barely move the estimate. A leak is flagged only when the 90% credible interval excludes the baseline +/- 5 points and n >= 30. Every leak shows n and confidence.
- bb/100 cost of a leak = summed EV loss in that category / hands x 100.
- Dashboard: trends over time, and the top 3 leaks ranked by bb/100 cost.

### 3b: Opponent profiles (exploit mode)

Drills and grading stay GTO-based. Opponent data is used two ways: as a filter on the player's leak report (pitfalls against each opponent), and as notes alongside the theory answer. From imported logs, build per-friend stats (VPIP, PFR, AF, fold to c-bet, showdown frequency) with the same shrinkage. Drills against a known friend show the GTO answer and an exploit note ("this opponent raises 74% preflop: defend the BB wider"). Full node locking (re-solving against a friend's estimated range) is a later step; postflop-solver supports custom ranges per spot, so it's feasible.

## Adaptive training (Milestone 4)

- SRS: SM-2 style intervals per (node, hand class). Interval growth is scaled down by EV loss and by the leak severity of the node's category, so expensive misses come back sooner.
- Daily session: 20 minutes, 60% leak work, 30% mixed review, 10% new spots. Time-boxed by measured average seconds per decision.
- Coach summary after each session: template-based from the session's stats ("You under-bet dry A-high boards in position by about 20 points, n=34, 85% confidence").

## Hand history import (Milestone 5)

- PokerNow CSV log parser (format documented by the logs themselves and two community converters). Hero's hole cards come from "Your hand is" lines; others' only at showdown.
- Each hero decision maps to the nearest solved node: matchup and pot type from the action history, stack bucket (40/50bb, nearest), flop to its cluster representative with a "distance" flag, bet sizes to the nearest tree size. Decisions too far from any solved node (multiway, straddles, odd sizes) are logged as unscored.
- Manual quick-entry screen for games on sites without export.
- Real-play results feed the same `attempt` table with `source=import`, so leak reports mix drills and real hands (filterable).

## Honest limits

- Solver output is an approximation: 0.5% exploitability target, a slim bet-size tree, ~75 representative flops standing in for 1,755, and approximate preflop ranges.
- Postflop solves are heads-up. Multiway pots (common in these games: VPIPs of 65-94%) are unscored.
- GTO is the wrong target against specific home-game opponents. Exploit mode (3b) addresses that, but its reads are only as good as the sample.

## Milestones and review gates

1. **Solver pipeline:** 1a preflop solver + ranges, 1b flop clustering, 1c batch CLI with estimate, resume and parallelism, then the overnight solve batch. Stop for review with the cost report before the full batch.
2. **Drill engine** with scoring and feedback. Review.
3. **Player model + leaks**, 3b opponent profiles. Review.
4. **Adaptive training.** Review.
5. **Import** (PokerNow + manual entry). Review.

Tests: scoring (EV loss, the 5% mixed-strategy rule, size mapping), frequency math (aggregation over combos and opportunities), leak statistics (shrinkage, credible intervals, minimum samples), and the PokerNow parser (net results sum to zero, stat counts on a fixture log).
