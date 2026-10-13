import { useCallback, useEffect, useRef, useState } from "react";
import { Quiz, quizApi, QuizResult } from "./api";
import { Card } from "./Card";

const KIND_LABEL: Record<string, string> = { pf_first_in: "First in", pf_vs_open: "Facing a raise", pf_vs_3bet: "Facing a 3-bet" };
const VERDICT_CLASS: Record<string, string> = { Best: "best", Correct: "correct", Inaccuracy: "inacc", Wrong: "wrong", Blunder: "blunder" };

/** Preflop quick-fire: one hand, one decision, a second or two each. */
export function Quickfire({ m }: { m: (x: number, d?: number) => string }) {
  const [table, setTable] = useState(3);
  const [stack, setStack] = useState(50);
  const [kinds, setKinds] = useState<string[]>([]);
  const [q, setQ] = useState<Quiz | null>(null);
  const [r, setR] = useState<QuizResult | null>(null);
  const [stats, setStats] = useState({ n: 0, good: 0, loss: 0, streak: 0, best: 0 });
  const [error, setError] = useState<string | null>(null);
  const timer = useRef<number | null>(null);

  const deal = useCallback(async () => {
    if (timer.current) { clearTimeout(timer.current); timer.current = null; }
    setR(null); setError(null);
    try { setQ(await quizApi.deal(table, stack, kinds)); } catch (e) { setError(String(e)); }
  }, [table, stack, kinds]);

  useEffect(() => { deal(); }, [table, stack, kinds]); // eslint-disable-line react-hooks/exhaustive-deps

  const answer = useCallback(async (i: number) => {
    if (!q || r) return;
    try {
      const res = await quizApi.answer(i);
      setR(res);
      const good = res.verdict === "Best" || res.verdict === "Correct";
      setStats((s) => { const streak = good ? s.streak + 1 : 0; return { n: s.n + 1, good: s.good + (good ? 1 : 0), loss: s.loss + res.ev_loss_bb, streak, best: Math.max(s.best, streak) }; });
      if (good) timer.current = window.setTimeout(deal, 650);
    } catch (e) { setError(String(e)); }
  }, [q, r, deal]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.target as HTMLElement)?.tagName === "INPUT" || (e.target as HTMLElement)?.tagName === "SELECT") return;
      if (r && (e.key === "Enter" || e.key === " ")) { e.preventDefault(); deal(); return; }
      const k = parseInt(e.key, 10);
      if (!r && q && k >= 1 && k <= q.actions.length) answer(k - 1);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [q, r, deal, answer]);

  const toggle = (k: string) => setKinds((ks) => (ks.includes(k) ? ks.filter((x) => x !== k) : [...ks, k]));

  return (
    <main className="main quickfire">
      <div className="leak-controls">
        <select value={table} onChange={(e) => setTable(+e.target.value)}><option value={3}>3-handed</option><option value={6}>4-6 handed</option></select>
        <select value={stack} onChange={(e) => setStack(+e.target.value)}>{[40, 50, 100].map((s) => <option key={s} value={s}>{s}bb</option>)}</select>
        <div className="chips">
          {Object.entries(KIND_LABEL).map(([k, l]) => <button key={k} className={`chip ${kinds.includes(k) ? "on" : ""}`} onClick={() => toggle(k)}>{l}</button>)}
        </div>
        <span className="hint inline">{stats.n} hands · {stats.n ? Math.round((100 * stats.good) / stats.n) : 0}% right · streak {stats.streak} (best {stats.best}) · {m(stats.n ? stats.loss / stats.n : 0, 2)} lost per hand</span>
      </div>
      {error && <div className="error">{error}</div>}
      {q && (
        <div className={`qf-card ${r ? VERDICT_CLASS[r.verdict] : ""}`}>
          <div className="qf-situation"><span className="pos">{q.seat}</span> {q.situation} <span className="muted">{q.stack}bb</span></div>
          <div className="qf-hand">{q.hand.map((c) => <Card key={c} card={c} size="lg" />)}</div>
          <div className="actions qf-actions">
            {q.actions.map((a, i) => (
              <button key={i} className={`action ${q.kinds[i]} ${r ? (i === r.best ? "best" : i === r.chosen ? "chosen" : "") : ""}`} disabled={!!r} onClick={() => answer(i)}>
                <span className="key">{i + 1}</span><span>{a}</span>
                {r && <span className="freq">{Math.round(r.freq[i] * 100)}%</span>}
              </button>
            ))}
          </div>
          {r && (
            <div className="qf-result">
              <span className={`vtext ${VERDICT_CLASS[r.verdict]}`}><b>{r.verdict}</b></span>
              {r.ev_loss_bb > 0.005 && <span> · lost {m(r.ev_loss_bb, 2)}</span>}
              <div className="muted">{r.why}</div>
              {!(r.verdict === "Best" || r.verdict === "Correct") && <button className="primary" onClick={deal}>Next ↵</button>}
            </div>
          )}
        </div>
      )}
      <p className="hint">Keys 1-{q?.actions.length ?? 3}. Right answers move on by themselves; mistakes wait for you.</p>
    </main>
  );
}
