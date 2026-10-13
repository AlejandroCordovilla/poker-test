import { useCallback, useEffect, useRef, useState } from "react";

type Q = { kind: string; text: string; options: string[]; answer: number; explain: string };
const KINDS: Record<string, string> = { odds: "Pot odds", mdf: "How often to defend", bluff: "Bluff needs to work", outs: "Outs to equity", call: "Call or fold?" };
const SIZES = [0.33, 0.5, 0.75, 1, 1.5];
const r1 = (x: number) => Math.round(x * 10) / 10;
const pct = (x: number) => `${Math.round(x * 100)}%`;
const pick = <T,>(a: T[]) => a[Math.floor(Math.random() * a.length)];
const comb2 = (n: number) => (n * (n - 1)) / 2;

/** Four distinct percent options around the right answer, shuffled. */
function options(right: number, wrongs: number[]): [string[], number] {
  const set = new Set<number>([Math.round(right * 100)]);
  for (const w of wrongs) if (set.size < 4 && w > 0 && w < 1) set.add(Math.round(w * 100));
  let k = 5;
  while (set.size < 4) { set.add(Math.min(95, Math.max(3, Math.round(right * 100) + (Math.random() < 0.5 ? -k : k)))); k += 4; }
  const arr = [...set].sort(() => Math.random() - 0.5);
  return [arr.map((x) => `${x}%`), arr.indexOf(Math.round(right * 100))];
}

function make(kinds: string[]): Q {
  const kind = pick(kinds.length ? kinds : Object.keys(KINDS));
  const pot = pick([4, 6, 8, 10, 12, 16, 20, 24, 30, 40]);
  const f = pick(SIZES);
  const bet = r1(pot * f);
  if (kind === "odds") {
    const need = bet / (pot + 2 * bet);
    const [o, a] = options(need, [bet / (pot + bet), bet / pot, need + 0.1]);
    return { kind, text: `Pot is ${pot}bb. Villain bets ${bet}bb (${Math.round(f * 100)}% pot). What equity do you need to call?`, options: o, answer: a, explain: `call ÷ final pot = ${bet} ÷ (${pot} + ${bet} + ${bet}) = ${pct(need)}` };
  }
  if (kind === "mdf") {
    const mdf = pot / (pot + bet);
    const [o, a] = options(mdf, [bet / (pot + bet), 1 - bet / (pot + 2 * bet), mdf - 0.15]);
    return { kind, text: `Pot is ${pot}bb and they bet ${bet}bb. To stop any bluff from auto-profiting, how much of your range must continue?`, options: o, answer: a, explain: `pot ÷ (pot + bet) = ${pot} ÷ ${r1(pot + bet)} = ${pct(mdf)}` };
  }
  if (kind === "bluff") {
    const be = bet / (pot + bet);
    const [o, a] = options(be, [bet / (pot + 2 * bet), pot / (pot + bet), be + 0.12]);
    return { kind, text: `You bluff ${bet}bb into ${pot}bb. How often must they fold for the bluff to break even?`, options: o, answer: a, explain: `bet ÷ (pot + bet) = ${bet} ÷ ${r1(pot + bet)} = ${pct(be)}` };
  }
  if (kind === "outs") {
    const outs = pick([4, 6, 8, 9, 12, 15]);
    const flop = Math.random() < 0.5;
    const eq = flop ? 1 - comb2(47 - outs) / comb2(47) : outs / 46;
    const [o, a] = options(eq, [flop ? outs / 46 : (outs * 4) / 100, flop ? (outs * 2) / 100 : (outs * 4) / 100, eq + 0.1]);
    return {
      kind,
      text: `You have ${outs} outs on the ${flop ? "flop (two cards to come, all-in)" : "turn (one card to come)"}. Chance to hit?`,
      options: o,
      answer: a,
      explain: flop ? `1 - (misses on both cards) = ${pct(eq)}. Shortcut: outs × 4 ≈ ${outs * 4}%` : `outs ÷ 46 = ${pct(eq)}. Shortcut: outs × 2 ≈ ${outs * 2}%`,
    };
  }
  const need = bet / (pot + 2 * bet);
  const eq = Math.max(0.05, Math.min(0.9, need + pick([-0.15, -0.08, -0.04, 0.04, 0.08, 0.15])));
  const right = eq >= need ? 0 : 1;
  return { kind: "call", text: `Pot ${pot}bb, they bet ${bet}bb. You have about ${pct(eq)} equity. Call or fold?`, options: ["Call", "Fold"], answer: right, explain: `you need ${pct(need)}; you have ${pct(eq)}, so ${right === 0 ? "call" : "fold"}` };
}

const SECONDS = 12;

/** Fast mental-math reps: pot odds, defense frequency, bluff break-even, outs. */
export function MathDrill() {
  const [kinds, setKinds] = useState<string[]>([]);
  const [q, setQ] = useState<Q>(() => make([]));
  const [picked, setPicked] = useState<number | null>(null);
  const [left, setLeft] = useState(SECONDS);
  const [stats, setStats] = useState({ n: 0, right: 0, streak: 0 });
  const tick = useRef<number | null>(null);

  const next = useCallback(() => { setQ(make(kinds)); setPicked(null); setLeft(SECONDS); }, [kinds]);
  useEffect(() => { next(); }, [kinds]); // eslint-disable-line react-hooks/exhaustive-deps

  const choose = useCallback((i: number) => {
    if (picked !== null) return;
    setPicked(i);
    const ok = i === q.answer;
    setStats((s) => ({ n: s.n + 1, right: s.right + (ok ? 1 : 0), streak: ok ? s.streak + 1 : 0 }));
    if (ok) window.setTimeout(next, 900);
  }, [picked, q, next]);

  useEffect(() => {
    if (picked !== null) return;
    tick.current = window.setInterval(() => setLeft((l) => (l <= 0.1 ? 0 : l - 0.1)), 100);
    return () => { if (tick.current) clearInterval(tick.current); };
  }, [q, picked]);
  useEffect(() => { if (left === 0 && picked === null) choose(-1); }, [left, picked, choose]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (picked !== null && (e.key === "Enter" || e.key === " ")) { e.preventDefault(); next(); return; }
      const k = parseInt(e.key, 10);
      if (picked === null && k >= 1 && k <= q.options.length) choose(k - 1);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [picked, q, next, choose]);

  return (
    <main className="main quickfire">
      <div className="leak-controls">
        <div className="chips">
          {Object.entries(KINDS).map(([k, l]) => (
            <button key={k} className={`chip ${kinds.includes(k) ? "on" : ""}`} onClick={() => setKinds((ks) => (ks.includes(k) ? ks.filter((x) => x !== k) : [...ks, k]))}>{l}</button>
          ))}
        </div>
        <span className="hint inline">{stats.n} answered · {stats.n ? Math.round((100 * stats.right) / stats.n) : 0}% right · streak {stats.streak}</span>
      </div>
      <div className={`qf-card ${picked === null ? "" : picked === q.answer ? "best" : "wrong"}`}>
        <div className="muted">{KINDS[q.kind]}</div>
        <div className="qf-situation math-q">{q.text}</div>
        <div className="bar wide"><div style={{ width: `${(left / SECONDS) * 100}%` }} /></div>
        <div className="actions qf-actions">
          {q.options.map((o, i) => (
            <button key={i} className={`action ${picked !== null ? (i === q.answer ? "best" : i === picked ? "chosen" : "") : ""}`} disabled={picked !== null} onClick={() => choose(i)}>
              <span className="key">{i + 1}</span><span>{o}</span>
            </button>
          ))}
        </div>
        {picked !== null && (
          <div className="qf-result">
            <b className={picked === q.answer ? "vtext best" : "vtext wrong"}>{picked === q.answer ? "Right" : picked === -1 ? "Time's up" : "Not quite"}</b>
            <div className="muted">{q.explain}</div>
            {picked !== q.answer && <button className="primary" onClick={next}>Next ↵</button>}
          </div>
        )}
      </div>
      <p className="hint">Keys 1-4. {SECONDS} seconds each, about the time you have at the table.</p>
    </main>
  );
}
