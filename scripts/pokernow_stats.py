"""PokerNow log -> per-player tendencies + deal-randomness checks.

Usage: python3 -I pokernow_stats.py <poker_now_log.csv> [hero_name]
Seed of the Milestone 5 PokerNow parser.
"""
import csv, collections, math, re, sys

PLAYER = re.compile(r'"(.+?) @ [^"]+"')
CARD = re.compile(r'(10|[2-9JQKA])([♠♥♦♣])')
RANKS = "23456789TJQKA"


def cards(s):
    return [("T" if r == "10" else r, su) for r, su in CARD.findall(s)]


def chi2_p(obs, exp):
    """Chi-square statistic and approximate p-value (Wilson-Hilferty)."""
    x = sum((o - e) ** 2 / e for o, e in zip(obs, exp))
    k = len(obs) - 1
    z = ((x / k) ** (1 / 3) - (1 - 2 / (9 * k))) / math.sqrt(2 / (9 * k))
    return x, 0.5 * math.erfc(z / math.sqrt(2))


def binom_z(hits, n, p):
    return (hits - n * p) / math.sqrt(n * p * (1 - p))


def parse(path):
    rows = sorted(csv.DictReader(open(path, encoding="utf-8")), key=lambda r: int(r["order"]))
    hands, cur = [], None
    for r in rows:
        e = r["entry"]
        if e.startswith("-- starting hand"):
            cur = {"lines": [], "hero": None, "board": []}
            hands.append(cur)
        elif e.startswith("-- ending hand"):
            cur = None
        elif cur is not None:
            cur["lines"].append(e)
            if e.startswith("Your hand is"):
                cur["hero"] = cards(e)
            elif e.startswith(("Flop:", "Turn:", "River:")):
                cur["board"] = cards(e)
    return hands


def tendencies(hands):
    S = collections.defaultdict(collections.Counter)
    for h in hands:
        street, seated = "pre", set()
        pre_raises, last_aggr, cbet_chance, cbet_made = 0, None, False, False
        vpip, pfr, saw_flop, invested = set(), set(), set(), collections.Counter()
        street_in = collections.Counter()
        folded = set()
        for e in h["lines"]:
            if e.startswith("Player stacks"):
                seated = set(PLAYER.findall(e))
                for p in seated:
                    S[p]["hands"] += 1
                continue
            if e.startswith(("Flop:", "Turn:", "River:")):
                for p in street_in:
                    invested[p] += street_in[p]
                street_in.clear()
                new = e.split(":")[0].lower()
                if new == "flop":
                    saw_flop = seated - folded
                    for p in saw_flop:
                        S[p]["saw_flop"] += 1
                street = new
                continue
            m = PLAYER.match(e)
            if not m:
                continue
            p, act = m.group(1), e[m.end():]
            amt = re.findall(r"\d+(?:\.\d+)?", act)
            amt = float(amt[0]) if amt else 0.0
            if "posts a" in act:
                street_in[p] = amt
            elif act.startswith(" folds"):
                folded.add(p)
                S[p][f"{street}_fold"] += 1
                if street == "flop" and cbet_made and p != last_aggr:
                    S[p]["fold_to_cbet"] += 1
            elif act.startswith(" checks"):
                S[p][f"{street}_check"] += 1
                if street == "flop" and p == last_aggr and not cbet_made and cbet_chance is False:
                    S[p]["cbet_opp"] += 1
                    cbet_chance = True
            elif act.startswith(" calls"):
                street_in[p] = amt
                S[p][f"{street}_call"] += 1
                if street == "pre":
                    vpip.add(p)
                if street == "flop" and cbet_made and p != last_aggr:
                    S[p]["call_cbet"] += 1
            elif act.startswith((" bets", " raises")):
                street_in[p] = amt
                S[p][f"{street}_aggr"] += 1
                if street == "pre":
                    vpip.add(p); pfr.add(p)
                    if pre_raises >= 1:
                        S[p]["threebet"] += 1
                    pre_raises += 1
                    last_aggr = p
                elif street == "flop" and act.startswith(" bets") and p == last_aggr and not cbet_chance:
                    S[p]["cbet_opp"] += 1; S[p]["cbet"] += 1
                    cbet_made, cbet_chance = True, True
            elif act.startswith(" collected"):
                S[p]["won"] += amt
                if "with" in act:
                    S[p]["won_sd"] += 1
            elif act.startswith(" shows"):
                S[p]["showdown"] += 1
            if e.startswith("Uncalled bet"):
                pass
        for e in h["lines"]:
            if e.startswith("Uncalled bet"):
                amt = float(re.findall(r"\d+(?:\.\d+)?", e)[0])
                q = PLAYER.search(e).group(1)
                S[q]["won"] += amt
        for p in street_in:
            invested[p] += street_in[p]
        for p in invested:
            S[p]["invested"] += invested[p]
        for p in vpip:
            S[p]["vpip"] += 1
        for p in pfr:
            S[p]["pfr"] += 1
    return S


def pct(a, b):
    return f"{100 * a / b:5.1f}% ({a}/{b})" if b else "   n/a"


def report_players(S):
    for p, s in sorted(S.items(), key=lambda kv: -kv[1]["hands"]):
        post = ["flop", "turn", "river"]
        aggr = sum(s[f"{st}_aggr"] for st in post)
        calls = sum(s[f"{st}_call"] for st in post)
        acts = aggr + calls + sum(s[f"{st}_check"] + s[f"{st}_fold"] for st in post)
        print(f"\n{p}  ({s['hands']} hands, net {s['won'] - s['invested']:+.2f})")
        print(f"  VPIP            {pct(s['vpip'], s['hands'])}")
        print(f"  PFR             {pct(s['pfr'], s['hands'])}")
        print(f"  3-bet (count)   {s['threebet']}")
        print(f"  Post-flop aggression freq {pct(aggr, acts)}   AF (bets+raises / calls) {aggr / calls if calls else float('inf'):.2f}")
        print(f"  Flop c-bet      {pct(s['cbet'], s['cbet_opp'])}")
        faced = s['fold_to_cbet'] + s['call_cbet']
        print(f"  Fold to c-bet   {pct(s['fold_to_cbet'], faced)}  (raises vs c-bet not split out)")
        print(f"  Went to showdown {pct(s['showdown'], s['saw_flop'])} of flops seen")


def report_deal(hands):
    hero = [h["hero"] for h in hands if h["hero"] and len(h["hero"]) == 2]
    n = len(hero)
    pairs = sum(a[0] == b[0] for a, b in hero)
    suited = sum(a[1] == b[1] for a, b in hero)
    print(f"\nDEAL CHECKS")
    print(f"  Your hole cards: {n} hands")
    print(f"    pocket pairs {pairs} (expect {n/17:.1f}, z={binom_z(pairs, n, 1/17):+.2f})")
    print(f"    suited       {suited} (expect {n*4/17:.1f}, z={binom_z(suited, n, 4/17):+.2f})")
    rc = collections.Counter(c[0] for hc in hero for c in hc)
    x, p = chi2_p([rc[r] for r in RANKS], [2 * n / 13] * 13)
    print(f"    rank spread  chi2={x:.1f} p={p:.2f}")
    flops = [h["board"][:3] for h in hands if len(h["board"]) >= 3]
    f = len(flops)
    paired = sum(len({c[0] for c in fl}) < 3 for fl in flops)
    mono = sum(len({c[1] for c in fl}) == 1 for fl in flops)
    two = sum(len({c[1] for c in fl}) == 2 for fl in flops)
    print(f"  Flops: {f}")
    print(f"    paired    {paired} (expect {f*0.1718:.1f}, z={binom_z(paired, f, 0.1718):+.2f})")
    print(f"    monotone  {mono} (expect {f*0.0518:.1f}, z={binom_z(mono, f, 0.0518):+.2f})")
    print(f"    two-tone  {two} (expect {f*0.5506:.1f}, z={binom_z(two, f, 0.5506):+.2f})")
    board = [c for h in hands for c in h["board"]]
    rc = collections.Counter(c[0] for c in board)
    sc = collections.Counter(c[1] for c in board)
    x, p = chi2_p([rc[r] for r in RANKS], [len(board) / 13] * 13)
    print(f"  Board cards: {len(board)}  rank chi2={x:.1f} p={p:.2f}", end="")
    x, p = chi2_p([sc[s] for s in "♠♥♦♣"], [len(board) / 4] * 4)
    print(f"  suit chi2={x:.1f} p={p:.2f}")


# ---- Hero line analysis (river give-ups, preflop hand selection) ----
RV = {r: i for i, r in enumerate(RANKS, 2)}


def made_hand(hole, board):
    """Rough strength: 'nothing', 'pair+' (pair using a hole card, pocket pair, flush or straight)."""
    allc = hole + board
    if hole[0][0] == hole[1][0] or any(h[0] == b[0] for h in hole for b in board):
        return "pair+"
    suits = collections.Counter(c[1] for c in allc)
    if any(v >= 5 for v in suits.values()):
        return "pair+"
    vals = {RV[c[0]] for c in allc} | ({1} if any(c[0] == "A" for c in allc) else set())
    if any(all(v + i in vals for i in range(5)) for v in vals):
        return "pair+"
    return "nothing"


def hand_class(h):
    a, b = sorted(h, key=lambda c: -RV[c[0]])
    gap = RV[a[0]] - RV[b[0]]
    s = a[1] == b[1]
    if gap == 0:
        return "pocket pair"
    if s and gap == 1:
        return "suited connector"
    if s:
        return "other suited"
    if RV[b[0]] >= 10:
        return "offsuit broadway"
    if a[0] == "A":
        return "offsuit ace"
    return "offsuit other"


def report_hero(hands, hero):
    cls_dealt, cls_played = collections.Counter(), collections.Counter()
    lines = collections.Counter()
    examples = []
    for n, h in enumerate(hands, 1):
        if not h["hero"]:
            continue
        acts = {"pre": [], "flop": [], "turn": [], "river": []}
        river_seq, street, others_at_river = [], "pre", set()
        for e in h["lines"]:
            if e.startswith(("Flop:", "Turn:", "River:")):
                street = e.split(":")[0].lower()
                continue
            m = PLAYER.match(e)
            if not m:
                continue
            p, act = m.group(1), e[m.end():].split()[0]
            if p == hero:
                acts[street].append(act)
            if street == "river":
                river_seq.append((p, act))
                if p != hero:
                    others_at_river.add(p)
        c = hand_class(h["hero"])
        cls_dealt[c] += 1
        if any(a in ("calls", "bets", "raises") for a in acts["pre"]):
            cls_played[c] += 1
        aggr = lambda st: any(a in ("bets", "raises") for a in acts[st])
        if aggr("flop") and aggr("turn") and acts["river"]:
            strength = made_hand(h["hero"], h["board"])
            if aggr("river"):
                lines[f"barrel river ({strength})"] += 1
            elif acts["river"][0] == "checks":
                after = [(p, a) for p, a in river_seq[river_seq.index((hero, "checks")) + 1:]]
                opp_bet = next((p for p, a in after if a in ("bets", "raises") and p != hero), None)
                hero_resp = acts["river"][1] if len(acts["river"]) > 1 else None
                if opp_bet and hero_resp == "folds":
                    lines[f"check river, {opp_bet} bets, you FOLD ({strength})"] += 1
                    examples.append((n, h["hero"], h["board"], opp_bet))
                elif opp_bet:
                    lines[f"check river, {opp_bet} bets, you {hero_resp} ({strength})"] += 1
                else:
                    lines[f"check river, checked through ({strength})"] += 1
    print(f"\nPREFLOP: what you play (VPIP by hand class)")
    for c, d in cls_dealt.most_common():
        print(f"  {c:18} {pct(cls_played[c], d)}")
    print(f"\nBET FLOP + TURN, THEN RIVER:")
    for k, v in lines.most_common():
        print(f"  {v:3}  {k}")
    for n, hole, board, opp in examples:
        fmt = lambda cs: " ".join(r + s for r, s in cs)
        print(f"    hand #{n}: {fmt(hole)} on {fmt(board)} -> folded to {opp}")


if __name__ == "__main__":
    hands = parse(sys.argv[1])
    print(f"{len(hands)} hands")
    report_players(tendencies(hands))
    report_deal(hands)
    if len(sys.argv) > 2:
        report_hero(hands, sys.argv[2])
