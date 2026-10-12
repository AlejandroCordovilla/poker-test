# Preflop model

Source of every preflop range in the app. Code: `crates/preflop`. Output: `data/ranges/*.txt` (readable) and `*.json` (used by the batch).

## Why we built it

There are no owned charts, and no free, documented 40-50bb no-rake 6-max or 3-handed ranges turned up (searched 2026-10-11). Paid packs found were 100bb or raked. A purchased pack can replace these files later; the postflop pipeline only reads the range strings.

## Game modeled

- No rake, blinds 0.5/1, stacks 40bb, 50bb or 100bb, 3-handed (BTN, SB, BB) or 6-max (UTG, HJ, CO, BTN, SB, BB).
- First in: fold or open to 2.5bb (SB opens to 3bb). No limps.
- Facing an open: fold, call, or 3-bet (3x the open in position, 4x from the blinds, 3x for BB vs SB). A direct jam over an open is offered only below 35bb.
- Facing a 3-bet: fold, call, or jam (below 60bb); fold, call, or 4-bet to 2.25x (60bb and deeper), and the 3-bettor then folds, calls or jams. Facing a jam: fold or call.
- No multiway pots: once someone calls an open or 3-bets, later players fold. Squeezes and cold 4-bets are not modeled.

## How pots are valued

- All-in: raw equity against the opponent's exact range (with card removal between the two players).
- Pots that see a flop: each player's share of the pot is equity times a realization factor, normalized so the two shares sum to the pot.
  - Position: in position 1.0; out of position 0.90 in single-raised pots, 0.95 in 3-bet pots (0.80 and 0.88 at 80bb+, where position is worth more).
  - Initiative: the last preflop aggressor gets +8%.
  - Hand playability: realization rises with the hand's equity against a random hand (`0.75 + 0.9 x (equity - 0.5)`, clamped to 0.55-1.12), with +6% for suited and +3% for connected hands. This is why 72o realizes far less than its raw equity and suited connectors realize more.
- Equity between the 169 hand classes: 20,000 Monte Carlo runouts per matchup, fixed seed (`data/equity169.json`), checked against known values (AA vs KK about 82%, AKs vs QQ about 46%).
- Solved with vector CFR+ for 3,000 iterations. With 3 or 6 players CFR has no formal convergence guarantee; in practice the ranges are stable between runs.

## Calibration

Opening frequencies against widely published 100bb 6-max charts (roughly UTG 18%, HJ 22%, CO 29%, BTN 44%):

| Seat | 40bb | 50bb |
|---|---|---|
| UTG | 19.9% | 19.9% |
| HJ | 25.2% | 24.7% |
| CO | 30.3% | 30.5% |
| BTN | 40.7% | 40.6% |
| SB (raise or fold) | 58.9% | 58.7% |

At 100bb: UTG 18%, HJ 22%, CO 28%, BTN 40%, SB 47%.

BB vs a BTN open (3-handed, 40bb): fold 35%, call 53%, 3-bet 12%. BTN vs that 3-bet: fold 41%, call 24%, jam 35%.

## Known limits

- The button opens a few points tighter than 100bb charts, and the button flats too often against a CO open, because squeezes are not modeled.
- At 100bb the button 4-bets about 20% of the hands that face a 3-bet, more than solver charts (closer to 10-15%). 4-bet pots aren't drilled postflop, so this only shifts preflop advice.
- No limps. Real SB strategies at these depths often limp; the trainer's SB first-in advice is raise or fold.
- The realization factors are a standard approximation, not a solved postflop game. Preflop is the least precise layer of the pipeline; drills say so where preflop ranges drive the result.
- Exact frequencies matter less than the shape. Against typical home-game opponents (for example a 94/74 raiser or a 65/12 caller), opponent notes in exploit mode matter more than the third decimal here.
