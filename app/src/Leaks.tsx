import { useEffect, useState } from "react";
import { api, AttemptRow, LeakReport, LeakStat } from "./api";
import { prettyCards } from "./units";

const VERDICT_CLASS: Record<string, string> = { Best: "best", Correct: "correct", Inaccuracy: "inacc", Wrong: "wrong", Blunder: "blunder" };

function pct(x: number) {
  return `${Math.round(x * 100)}%`;
}

/** Leak page: every stat against the solver, with sample size and confidence. */
export function Leaks({ m, t, onDrill }: { m: (x: number, d?: number) => string; t: (s: string) => string; onDrill: (id: number) => void }) {
  const [source, setSource] = useState<string | null>(null);
  const [opponent, setOpponent] = useState<string | null>(null);
  const [relative, setRelative] = useState(false);
  const [report, setReport] = useState<LeakReport | null>(null);
  const [stat, setStat] = useState<LeakStat | null>(null);
  const [rows, setRows] = useState<AttemptRow[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api.leaks(source, opponent).then(setReport).catch((e) => setError(String(e)));
  }, [source, opponent]);

  useEffect(() => {
    api.attempts(stat?.key ?? null, source, opponent).then(setRows).catch((e) => setError(String(e)));
  }, [stat, source, opponent]);

  const gap = (s: LeakStat) => (relative ? (s.solver > 0 ? s.estimate / s.solver - 1 : 0) : s.estimate - s.solver);
  const fmtGap = (s: LeakStat) => {
    const g = gap(s);
    return `${g > 0 ? "+" : ""}${Math.round(g * 100)}${relative ? "%" : " pts"}`;
  };
  const cls = (s: LeakStat) => (s.n < 30 ? "muted" : s.estimate - s.solver > 0.05 ? "above" : s.solver - s.estimate > 0.05 ? "below" : "");

  return (
    <main className="main leaks">
      {error && <div className="error">{error}</div>}
      <div className="leak-controls">
        <div className="seg small">
          {[[null, "All"], ["drill", "Drills"], ["import", "Real hands"]].map(([v, l]) => (
            <button key={l} className={source === v ? "on" : ""} onClick={() => setSource(v)}>{l}</button>
          ))}
        </div>
        {report && report.opponents.length > 0 && (
          <select value={opponent ?? ""} onChange={(e) => setOpponent(e.target.value || null)}>
            <option value="">All opponents</option>
            {report.opponents.map((o) => <option key={o} value={o}>vs {o}</option>)}
          </select>
        )}
        <div className="seg small">
          <button className={!relative ? "on" : ""} onClick={() => setRelative(false)}>Absolute</button>
          <button className={relative ? "on" : ""} onClick={() => setRelative(true)}>Relative</button>
        </div>
        <span className="hint inline">{report?.decisions ?? 0} decisions</span>
      </div>

      {report && report.top.length > 0 && (
        <div className="top-leaks">
          <h3>Top leaks costing you the most</h3>
          <div className="cards3">
            {report.top.map((s) => (
              <button key={s.key} className="leak-card" onClick={() => setStat(s)}>
                <div className="lc-name">{s.name}</div>
                <div className="lc-num">{m(s.bb_per_100 / 100, 2)} <span>per decision</span></div>
                <div className="lc-sub">You {pct(s.estimate)} vs solver {pct(s.solver)} · n={s.n} · {Math.round(s.confidence * 100)}% confident</div>
              </button>
            ))}
          </div>
        </div>
      )}
      {report && report.top.length === 0 && (
        <p className="hint">No confirmed leaks yet. A stat needs at least 30 decisions and a gap the data is sure about before it's called a leak.</p>
      )}

      <table className="leak-table">
        <thead>
          <tr><th>Stat</th><th className="num">n</th><th className="num">You</th><th className="num">Solver</th><th className="num">Gap</th><th>90% range</th><th className="num">Avg loss</th><th>Verdict</th></tr>
        </thead>
        <tbody>
          {report?.stats.map((s) => (
            <tr key={s.key} className={`clickable ${stat?.key === s.key ? "sel" : ""}`} onClick={() => setStat(stat?.key === s.key ? null : s)}>
              <td>{s.name}</td>
              <td className="num">{s.n}</td>
              <td className="num">{pct(s.you)}</td>
              <td className="num">{pct(s.solver)}</td>
              <td className={`num gap ${cls(s)}`}>{fmtGap(s)}</td>
              <td>
                <div className="ci">
                  <div className="ci-range" style={{ left: `${s.ci_low * 100}%`, width: `${(s.ci_high - s.ci_low) * 100}%` }} />
                  <div className="ci-solver" style={{ left: `${s.solver * 100}%` }} />
                </div>
              </td>
              <td className="num">{m(s.bb_per_100 / 100, 2)}</td>
              <td className={s.flagged ? "flag" : "muted"}>{s.verdict}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <p className="hint">Gap uses a shrunk estimate: your rate is pulled toward the solver until there's enough data (20-decision prior), so a few hands can't fake a leak. Red: you do it more than the solver; blue: less. The bar shows the 90% range of your true rate; the tick is the solver.</p>

      <h3>{stat ? `Decisions: ${stat.name}` : "Costliest decisions"}</h3>
      <table className="cats">
        <thead><tr><th>Hand</th><th>Board</th><th>You</th><th>Solver</th><th className="num">Lost</th><th>Grade</th><th></th></tr></thead>
        <tbody>
          {rows.map((r) => (
            <tr key={r.id}>
              <td>{prettyCards(r.hand)}</td>
              <td>{prettyCards(r.board)}</td>
              <td>{t(r.chosen)}</td>
              <td>{t(r.best)}</td>
              <td className="num">{m(r.ev_loss_bb, 2)} <span className="muted">({r.ev_loss_pct.toFixed(1)}%)</span></td>
              <td className={`vtext ${VERDICT_CLASS[r.verdict] ?? ""}`}>{r.verdict}</td>
              <td>{r.can_redrill && <button className="mini" onClick={() => onDrill(r.id)}>Drill this</button>}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </main>
  );
}
