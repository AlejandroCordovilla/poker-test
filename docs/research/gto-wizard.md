# GTO Wizard: feature and UX research

Researched 2026-10-11 from public material only: 15 YouTube transcripts, help.gtowizard.com, blog.gtowizard.com patch notes and press. No login, nothing fetched from app.gtowizard.com, no solutions or ranges copied. **[GTOW]** = stated by GTO Wizard (help center, blog, official channel); **[3P]** = observed in a third-party video or article. IDs (V#, H#, B#, P#) refer to the sources list.

## 1. Feature map

| Feature | What it does, how the user drives it | Evidence |
|---|---|---|
| **Trainer** | Plays hands against the solver. Modes: Full Hand, Street, Spot (one decision repeated). Info panel shows both ranges, hand equity and class, range frequencies. History box: expand streets, "play from here", change the opponent's move, "play as hero" from the other seat. Study button opens the node in the solution browser. | [GTOW] H2; V1 0:48-3:05 |
| Trainer settings | Positions (several allowed), starting node, preflop action, randomized stacks, IP/OOP/alternate; board cards or texture filters; hands by range slider or hand class; input Actions or Frequency; pause after each action or each mistake; Auto New Hand off/3 s/instant; RNG High/Low/Off; timebank 7/15/25 s; difficulty Standard/Grouped/Simple; up to 4 tables; session length; hints, quick results, streak icon; focus mode (F). | [GTOW] H2; V1 3:05-8:36; V2 6:25, 9:36 |
| "Close decisions" | Deals only spots where two actions are close in EV. Threshold not public (coach: "picks an EV threshold I believe"). Board "subsets" (~85 representative flops per the coach) also available. | V2 4:50, 6:25 (GTOW channel); H8 |
| Drills | My Drills vs premade; color tags, All/Any tag filter; sort by hands played or GTOW Score%; Export/Duplicate/Edit/Play/Stats; share by import/export. | [GTOW] H3 |
| Trainer stats | Lifetime and per-session stats filtered by format/date, by pot type; practiced hands sortable by EV loss or frequency difference, with replayer. | [GTOW] H4 |
| **Study** | 13x13 matrix colored by action; actions table (click to filter); hover a hand for its mix; Summary tab (weight, EV, EQ, EQR, blocker scores); Ranges, Breakdown (Range% / Performed% / Represented%), Reports tabs. | [GTOW] H7, H9; V3 1:31-19:48 |
| **Aggregate reports** | Strategy/EV/EQ/EQR across 1,755 flops, probability-weighted; chart or table; "all flops" row above a "Filtered" row; group by high card, suits, pairing, connectedness; above/below-average highlight; each row links to practice or study. | [GTOW] H8; V4 0:47-3:49 |
| **Analyze** | Upload histories; hands table sortable by EV loss, BB W/L, pot; filters (date, position, preflop action, aggression, EV-loss range, GTO score, GTO result, status, postflop line, effective stack); saved reports; notes and color tags; one click opens a hand in Study or Practice. Stats view breaks GTOW score, frequency difference and mistake/blunder counts down by street, preflop action, position, aggression. | [GTOW] H10-H12, B1-B3; V5 1:36-16:46 |
| **GTO Reports** | Preflop stats (VPIP, PFR, RFI, limp, squeeze, 3/4/5-bet) vs GTO: above red, below blue; Absolute/Relative toggle; click a stat for position breakdowns; low-sample flag under 200 hands; Mistakes panel sorted by EV loss. Cash, 100/200bb, rake-adjusted only. | [GTOW] B2, B3 |
| **Range Builder** | Paint a node's whole strategy (brush, sliders, category painting, lock combos), Confirm, get graded. | [GTOW] H5 |
| **Custom solving** | Edit ranges, stacks, tree; sizing Automatic (finds the highest-EV single size), Dynamic or Fixed; save tagged parameter sets; drill the result. | [GTOW] V8 0:47-8:43; H13 |
| **Nodelocking** | Set Strategy (paint), Set Frequency (sliders; hands shift by EV loss), Lock/Unlock per hand or category. Compare Nodes: before/after side by side, switchable to EV. One street at a time. | [GTOW] H14; V7 1:32-8:44 |
| Player profiles | Preset opponent types modeled as incentives (reward betting/calling, penalize folding), "not forward-looking". | [GTOW] B4; [3P] V14 2:18-8:28 |
| Study plans | Ordered lessons mixing articles, videos, solution links and preset drills. | [GTOW] B1; [3P] V13 0:46-6:11 |
| PokerArena | Rated heads-up matches (1500 start in the video), free post-game review exported to Analyze. | [GTOW] B2, B3; [3P] V15 0:45 |

## 2. Scoring and grading (as published)

- **Move classes** [GTOW] H4: *Best* = highest-frequency action, or the action your RNG roll selects. *Correct* = played at some frequency but not best. *Inaccuracy* = played under 3.5% in GTO without losing significant EV. *Wrong* = never played. *Blunder* = never played and loses significant EV. "Significant" is **not defined publicly** (V6 19:09 confirms a threshold exists).
- **GTOW Score** [GTOW] H2, H4, V1 4:40: each move scores -100% to +100%. Best moves (double check mark) get max points; lower-frequency actions get points "according to their percentages"; blunders lose points by EV loss in % of pot. **Formula unknown.** RNG mode changes what counts as best, and so the score (V1 7:49).
- **Accuracy** [GTOW] V1 4:40: share of correct vs incorrect moves.
- **EV metrics** [GTOW] H4, V1 5:06: total EV loss (bb); average per hand, per action, per mistake; also as % of pot "to compare across pot sizes". Analyzer also shows bb/100 (7.65 bb shown as "765 bb/100 error") [GTOW] H11.
- **Frequency difference** [GTOW] H4: frequency of the most common action minus frequency of the action taken, averaged over actions.
- **Analyzer colors** [GTOW] H11, H12: green correct, yellow inaccuracy, red blunder.
- **Score bands** [GTOW] H12: >85% good; <~80% in 6-max needs work; regs 90-95%; 95%+ excellent; 100% isn't the goal.
- **Range Builder** [GTOW] H5: per-combo frequency comparison with GTO; 0-100% summary; separate grades for hand categories, draws, equity buckets; best and worst categories. Formula unknown.
- **Mixed strategies** [GTOW] H2: RNG off, the top-frequency action is best and the others are correct with partial points; RNG on (High: most aggressive action on the highest roll), the rolled action is best. Frequency input lets you enter the mix. Grouped/Simple difficulty auto-picks the top action inside the chosen group.
- **Hand mapping** [GTOW] H10, H11: closest rake and stack; presolved preflop to turn, river solved on the exact board and sizes; closest size with the difference shown in parentheses; status Analyzed / Non-existing GTO spot / Error; multiway skipped; partial analysis after an off-tree action.

## 3. UX patterns worth copying

1. One color per action everywhere; black for combos not in range; editable themes for color-blind users [GTOW] V3 1:31, 13:34.
2. Green/yellow/red verdict plus a double check mark for best; numbers second.
3. Normalized range view; per-combo horizontal view when suits matter [GTOW] V3 14:19.
4. Hand-class filter strip with arrow keys ("two pair and better") and equity buckets that re-filter the frequencies [GTOW] V3 15:52.
5. Display-only action grouping (All/Grouped/Simple, key S) [GTOW] V3 17:25.
6. Hotkeys: F focus, Space bb vs % pot, S grouping, W chart/table, 1-4 tabs, P clear filters [GTOW] H7, H8.
7. "Pause after mistake only" plus instant next hand for volume; Learning mode auto-opens the explanation on pause [GTOW] H2, H15.
8. Every view links to the others: report row, analyzed hand and trainer hand each open in Study or Practice [GTOW] V4 3:49, B3, V1 3:05.
9. All-vs-filtered rows in reports; Compare Nodes side by side [GTOW] H8, H14.

## 4. Gap analysis vs DESIGN.md

| # | Pri | Change | Why it helps our player | Size |
|---|---|---|---|---|
| 1 | High | Five grading classes with green/yellow/red. Keep our <5% line as "inaccuracy" (GTOW: 3.5%); define "blunder" ourselves (frequency ~0 and EV loss above X% of pot) and show the rule in-app. | Separates costly errors from harmless low-frequency picks; leak stats can count blunders only. | S |
| 2 | High | EV loss in bb **and % of pot**; session totals per decision and per mistake; frequency difference. | Short-stack SRPs and 3-bet pots differ in size; % pot compares them. | S |
| 3 | High | RNG mode (1-100 roll) and Frequency input for mixed nodes. | Our 40/60 rule never punishes poor mixing; RNG trains executing it. | S |
| 4 | High | Full Hand / Street / Spot modes; pause after action or mistake; instant next hand. | Full Hand drills river defense in context (a logged leak); Spot gives volume on SB limp and 3-bet decisions. | M |
| 5 | High | Focused filters: hand class, close decisions, textures mapped to our flop clusters, IP/OOP. | Targets the three logged leaks directly. | S-M |
| 6 | High | Import UX from Analyze: per-decision status (Scored / No solved spot / Partial), size-mapping delta in parentheses, sort by EV loss, saved filters, "drill this spot". | Fits Milestone 5; multiway hands become visible instead of silently dropped. | M |
| 7 | Med | Leak page: red above / blue below baseline, Absolute/Relative toggle, click a stat to list its hands, Mistakes list sorted by EV loss. Keep our shrinkage, n≥30 and credible intervals (stronger than a 200-hand flag). | Makes Milestone 3 scannable and links each leak to drills. | M |
| 8 | Med | Range Builder: paint a node's strategy, grade per combo and per category. | Best fit for 3-handed preflop ranges and river call thresholds. | L |
| 9 | Med | Exploit mode: Compare Nodes (GTO vs vs-opponent, with EV) and opponent presets from imported stats. postflop-solver has `lock_current_strategy` (found in the local checkout), so locking is feasible. | Home opponents are far from GTO; side-by-side deltas teach the adjustment. | L |
| 10 | Med | Study view: Strategy+EV, a best-action/regret map, normalized view, Range%/Performed%/Represented% breakdown. | Backs our rule-based reason text with visible numbers. | M |
| 11 | Low | Saved, tagged drills with per-drill score; an ordered plan feeding the daily session, with a pass mark (a 3P video suggests ~90%). | Gives the 20-minute session a curriculum. | S |
| 12 | Low | Hotkeys (F, Space for bb/%/$, S, number keys) and color themes. | Faster reps; color-blind safety. | S |

**Don't copy:** Arena, ratings and matchmaking; upload quotas and cloud processing; rake pickers and 100/200bb 6-max benchmarks (we're no-rake, 40-100bb); ICM, PLO, MTT, ante solutions; 4-table and game-speed settings; GTOW's 85-95% score bands (calibrated on online regs); 1,755-flop reports (we have ~75 clusters: weight by cluster size and label approximate); Table Wizard live overlays (real-time help during play).

## 5. Sources

**YouTube (15 targeted, 15 transcripts fetched with youtube_transcript_api; yt-dlp not needed)**
- V1 [GTOW] "Best Way to Practice Poker in 2022": https://www.youtube.com/watch?v=Zw6crM3NzD0
- V2 [GTOW channel, coach Jerman] "GTO Trainer Secrets: Use THESE Drills to Improve Faster!": https://www.youtube.com/watch?v=dJoqBaRrZxg
- V3 [GTOW] "The Ultimate Guide to Studying GTO Poker Solutions": https://www.youtube.com/watch?v=6XdV_0tePy4
- V4 [GTOW] "How to Study Poker with Aggregated Reports": https://www.youtube.com/watch?v=D1vzP_nj77I
- V5 [GTOW] "How to Spot Your Biggest Leaks with GTO Wizard Analyzer 2.0": https://www.youtube.com/watch?v=2GghU5jzQys
- V6 [GTOW] "How a Pro Analyzes Poker Hands with GTO Wizard": https://www.youtube.com/watch?v=Ujg90wntIQU
- V7 [GTOW] "Discover the Power of Nodelocking!": https://www.youtube.com/watch?v=Xe-2CtTs3kg
- V8 [GTOW] "Introducing GTO Wizard AI": https://www.youtube.com/watch?v=mi5F1-69QOw
- V9 [GTOW] "How to Study Effectively With GTO Wizard": https://www.youtube.com/watch?v=RsDVNjSmqeI
- V10 [3P] PokerListings, "GTO Wizard Features Explained in One Video!": https://www.youtube.com/watch?v=KUFFA6Gukw8
- V11 [3P] GTO Poker Charts, "How to Upload Hands to GTO Wizard and Analyse Your Mistakes": https://www.youtube.com/watch?v=WvtSfQbFXTA
- V12 [3P] Felt Experience Poker, "How I Actually Use GTO Wizard to Improve": https://www.youtube.com/watch?v=l-sEiR_HGXQ
- V13 [3P] GTO Poker Charts, "Become a GTO Wizard with GTO Wizard Study Plans": https://www.youtube.com/watch?v=69fL98ws_GA
- V14 [3P] 2CC Vic, "I Tried Out GTO Wizard's New Exploit Feature": https://www.youtube.com/watch?v=Wnvg8mEiu2o
- V15 [3P] GTOWarrior, "Trying Out GTO Wizard's New Poker Arena Feature": https://www.youtube.com/watch?v=2CJ08-cMuNU

**Help center [GTOW]**
- H2 https://help.gtowizard.com/how-to-use-the-trainer/
- H3 https://help.gtowizard.com/manage-training-drills/
- H4 https://help.gtowizard.com/measure-performance/
- H5 https://help.gtowizard.com/how-to-use-the-range-builder/
- H6, H8 https://help.gtowizard.com/aggregate-reports-guide/
- H7 https://help.gtowizard.com/study-mode/ and https://help.gtowizard.com/ranges-tab/
- H9 https://help.gtowizard.com/breakdown-tab/
- H10 https://help.gtowizard.com/analyze-mode-guide/
- H11 https://help.gtowizard.com/how-to-use-the-hand-history-analyzer/
- H12 https://help.gtowizard.com/tips-and-tricks-to-study-analyzed-hands/
- H13 https://help.gtowizard.com/how-to-train-against-custom-solutions/
- H14 https://help.gtowizard.com/how-to-use-nodelocking/
- H15 https://help.gtowizard.com/training-ideas/
- Also read: https://help.gtowizard.com/practice-mode-overview/ (no specifics)

**Blog and site [GTOW]**
- B1 https://blog.gtowizard.com/redefining-precision-introducing-hh-analyzer-2-0-and-much-more/ (2024-11-27)
- B2 https://blog.gtowizard.com/major-upgrade-gto-reports-have-arrived-plus-tons-of-new-features/ (2025-03-21)
- B3 https://blog.gtowizard.com/redesigned_analyzer_and_upgraded_gto_reports/ (2025-09-24)
- B4 https://blog.gtowizard.com/exploiting-profiles-episode-iii-the-maniac/
- B5 https://blog.gtowizard.com/hand-history-analyzer-is-here-patch-notes/ (2020-12-11)
- https://gtowizard.com/ ; https://www.dailydoseofgto.com/

**Press [3P]**
- P1 https://www.pokernews.com/news/2025/03/gto-wizard-game-changing-feature-48276.htm
- P2 https://www.pokernews.com/news/promotions/2024/12/gto-wizard-unveils-upgraded-hand-history-analyzer-47632.htm
- P3 https://www.deucescracked.com/blog/gto-wizard-2026-overhaul-poker-study-guide (2026-08-11; vague, not relied on)
