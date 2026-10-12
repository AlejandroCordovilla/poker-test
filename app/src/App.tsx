import { useCallback, useEffect, useState } from "react";
import { api, DrillView, Feedback, Filters, Meta, Summary } from "./api";
import { Card } from "./Card";
import { convertText, loadUnits, money, saveUnits, Units } from "./units";

type Mode = "random" | "focused" | "leaks";
const EMPTY: Filters = { stacks: [], matchups: [], pot_types: [], streets: [], textures: [] };
const POT_NAME: Record<string, string> = { srp: "single-raised pot", "3bp": "3-bet pot" };
const VERDICT_CLASS: Record<string, string> = { Best: "best", Fine: "fine", Inaccuracy: "inacc", Mistake: "mistake", Blunder: "blunder" };

function toggle<T>(list: T[], v: T): T[] {
  return list.includes(v) ? list.filter((x) => x !== v) : [...list, v];
}

function Chips<T extends string | number>({ label, options, value, onChange, fmt }: {
  label: string; options: T[]; value: T[]; onChange: (v: T[]) => void; fmt?: (v: T) => string;
}) {
  return (
    <div className="filter">
      <div className="filter-label">{label}</div>
      <div className="chips">
        {options.map((o) => (
          <button key={String(o)} className={`chip ${value.includes(o) ? "on" : ""}`} onClick={() => onChange(toggle(value, o))}>
            {fmt ? fmt(o) : String(o)}
          </button>
        ))}
      </div>
    </div>
  );
}

export default function App() {
  const [tab, setTab] = useState<"drill" | "progress">("drill");
  const [meta, setMeta] = useState<Meta | null>(null);
  const [mode, setMode] = useState<Mode>("random");
  const [filters, setFilters] = useState<Filters>(EMPTY);
  const [drill, setDrill] = useState<DrillView | null>(null);
  const [fb, setFb] = useState<Feedback | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [session, setSession] = useState({ n: 0, loss: 0, good: 0 });
  const [summary, setSummary] = useState<Summary | null>(null);
  const [units, setUnitsState] = useState<Units>(loadUnits);
  const setUnits = (u: Units) => { setUnitsState(u); saveUnits(u); };
  const m = (x: number, d = 1) => money(x, units, d);
  const t = (s: string) => convertText(s, units);

  useEffect(() => { api.meta().then(setMeta).catch((e) => setError(String(e))); }, []);

  const next = useCallback(async () => {
    setError(null);
    setFb(null);
    try {
      setDrill(await api.newDrill(mode === "focused" ? filters : EMPTY, mode));
    } catch (e) {
      setDrill(null);
      setError(String(e));
    }
  }, [mode, filters]);

  useEffect(() => { if (meta && meta.spots > 0) next(); }, [meta, mode]); // eslint-disable-line react-hooks/exhaustive-deps

  const choose = useCallback(async (i: number) => {
    if (!drill || fb) return;
    try {
      const f = await api.answer(i);
      setFb(f);
      setSession((s) => ({ n: s.n + 1, loss: s.loss + f.score.ev_loss_bb, good: s.good + (f.verdict === "Best" || f.verdict === "Fine" ? 1 : 0) }));
    } catch (e) {
      setError(String(e));
    }
  }, [drill, fb]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (tab !== "drill") return;
      if (fb && (e.key === "Enter" || e.key === " ")) { e.preventDefault(); next(); return; }
      const k = parseInt(e.key, 10);
      if (!fb && drill && k >= 1 && k <= drill.actions.length) choose(k - 1);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [tab, fb, drill, next, choose]);

  useEffect(() => { if (tab === "progress") api.summary().then(setSummary).catch((e) => setError(String(e))); }, [tab]);

  return (
    <div className="app">
      <header className="top">
        <div className="brand">♠ Poker Trainer</div>
        <nav>
          <button className={tab === "drill" ? "on" : ""} onClick={() => setTab("drill")}>Drill</button>
          <button className={tab === "progress" ? "on" : ""} onClick={() => setTab("progress")}>Progress</button>
        </nav>
        <div className="session">
          {session.n > 0
            ? <>{session.n} this session · avg loss {m(session.loss / session.n, 2)} · {Math.round((100 * session.good) / session.n)}% good</>
            : meta ? <>{meta.spots} solved spots</> : null}
        </div>
        <div className="units">
          <div className="seg small">
            <button className={units.mode === "bb" ? "on" : ""} onClick={() => setUnits({ ...units, mode: "bb" })}>bb</button>
            <button className={units.mode === "$" ? "on" : ""} onClick={() => setUnits({ ...units, mode: "$" })}>$</button>
          </div>
          {units.mode === "$" && (
            <label className="bbval">
              BB = $
              <input type="number" min="0.01" step="0.05" value={units.bb}
                onChange={(e) => { const v = parseFloat(e.target.value); if (v > 0) setUnits({ ...units, bb: v }); }} />
            </label>
          )}
        </div>
      </header>

      {tab === "drill" ? (
        <div className="drill-layout">
          <aside className="side">
            <div className="seg">
              {(["random", "focused", "leaks"] as Mode[]).map((m) => (
                <button key={m} className={mode === m ? "on" : ""} onClick={() => setMode(m)}>
                  {m === "random" ? "Random" : m === "focused" ? "Focused" : "My leaks"}
                </button>
              ))}
            </div>
            {mode === "focused" && meta && (
              <div className="filters">
                <Chips label="Stack" options={meta.stacks} value={filters.stacks} onChange={(v) => setFilters({ ...filters, stacks: v })} fmt={(s) => `${s}bb`} />
                <Chips label="Matchup" options={meta.matchups} value={filters.matchups} onChange={(v) => setFilters({ ...filters, matchups: v })} />
                <Chips label="Pot" options={meta.pot_types} value={filters.pot_types} onChange={(v) => setFilters({ ...filters, pot_types: v })} fmt={(p) => (p === "srp" ? "Single-raised" : "3-bet")} />
                <Chips label="Street" options={["flop", "turn"]} value={filters.streets} onChange={(v) => setFilters({ ...filters, streets: v })} fmt={(s) => s[0].toUpperCase() + s.slice(1)} />
                <Chips label="Board" options={meta.textures} value={filters.textures} onChange={(v) => setFilters({ ...filters, textures: v })} />
                <button className="primary wide" onClick={next}>Deal</button>
              </div>
            )}
            {mode === "leaks" && <p className="hint">Deals spots from the category where you lose the most EV (needs 5+ drills in a category).</p>}
            {mode === "random" && <p className="hint">Any solved spot, weighted by how often its flop type comes up in real games.</p>}
            <p className="hint keys">Keys: 1-{drill?.actions.length ?? 4} pick an action, Enter for the next hand.</p>
          </aside>

          <main className="main">
            {error && <div className="error">{error}</div>}
            {drill && (
              <>
                <div className="spot-head">
                  <div className="spot-title">{units.mode === "$" ? `$${(drill.stack * units.bb).toFixed(0)} stacks` : `${drill.stack}bb`} · {drill.matchup} · {POT_NAME[drill.pot_type]}</div>
                  <div className="spot-sub">Preflop: {drill.preflop}. {drill.texture} board.</div>
                </div>

                <div className="table">
                  <div className="seat villain">{drill.villain} · {m(drill.villain_stack_bb)}</div>
                  <div className="pot">Pot {m(drill.pot_bb)}</div>
                  <div className="board">{drill.board.map((c) => <Card key={c} card={c} size="lg" />)}</div>
                  <div className="line">{drill.line.length ? t(drill.line.join(" → ")) : `${drill.street === "flop" ? "Flop" : "Turn"}: you act first`}</div>
                  <div className="hero">
                    <div className="hand">{drill.hand.map((c) => <Card key={c} card={c} size="lg" />)}</div>
                    <div className="seat">You: {drill.hero} ({drill.hero_in_position ? "in position" : "out of position"}) · {m(drill.hero_stack_bb)}</div>
                  </div>
                </div>

                <div className="prompt">{drill.node_type === "facing a bet" || drill.node_type === "facing a raise" ? `Facing ${m(drill.to_call_bb)} to call.` : drill.node_type === "checked to" ? "Checked to you." : "You're first to act."} What do you do?</div>
                <div className="actions">
                  {drill.actions.map((a) => {
                    const r = fb?.actions[a.index];
                    const cls = fb ? (a.index === fb.score.best ? "best" : a.index === fb.chosen ? "chosen" : "") : "";
                    return (
                      <button key={a.index} className={`action ${a.kind} ${cls}`} disabled={!!fb} onClick={() => choose(a.index)}>
                        <span className="key">{a.index + 1}</span>
                        <span>{t(a.label)}</span>
                        {r && <span className="freq">{Math.round(r.freq_hand * 100)}%</span>}
                      </button>
                    );
                  })}
                </div>

                {fb && (
                  <div className="feedback">
                    <div className={`verdict ${VERDICT_CLASS[fb.verdict]}`}>
                      <span className="v">{fb.verdict}</span>
                      <span>{fb.score.ev_loss_bb < 0.005 ? "no EV lost" : `lost ${m(fb.score.ev_loss_bb, 2)}`}</span>
                      {fb.score.rare && <span className="tag">solver almost never does this</span>}
                      {fb.score.mixed && <span className="tag">mixed spot</span>}
                    </div>
                    <div className="handline">You have {fb.hand_desc.made}{fb.hand_desc.draw ? ` + ${fb.hand_desc.draw}` : ""} · about {Math.round(fb.equity * 100)}% equity vs their range</div>
                    <table className="ev">
                      <thead><tr><th>Action</th><th>EV</th><th>This hand</th><th>Whole range</th></tr></thead>
                      <tbody>
                        {fb.actions.map((a, i) => (
                          <tr key={i} className={i === fb.score.best ? "best" : i === fb.chosen ? "chosen" : ""}>
                            <td>{t(a.label)}{i === fb.chosen ? " (you)" : ""}</td>
                            <td className="num">{m(a.ev_bb, 2)}</td>
                            <td><div className="bar"><div style={{ width: `${a.freq_hand * 100}%` }} /></div><span className="pct">{Math.round(a.freq_hand * 100)}%</span></td>
                            <td><div className="bar range"><div style={{ width: `${a.freq_range * 100}%` }} /></div><span className="pct">{Math.round(a.freq_range * 100)}%</span></td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                    <p className="reason">{t(fb.reason)}</p>
                    <button className="primary" onClick={next}>Next hand ↵</button>
                  </div>
                )}
              </>
            )}
            {!drill && !error && meta && meta.spots === 0 && <div className="empty">No solved spots yet. They appear here as the solver finishes them.</div>}
          </main>
        </div>
      ) : (
        <main className="main progress">
          {summary && (
            <>
              <div className="stats">
                <div><div className="v">{summary.attempts}</div><div className="l">decisions graded</div></div>
                <div><div className="v">{m(summary.avg_loss_bb, 2)}</div><div className="l">average EV loss</div></div>
                <div><div className="v">{Math.round(summary.best_or_fine_pct)}%</div><div className="l">best or fine</div></div>
              </div>
              <h3>Where you lose the most</h3>
              <table className="cats">
                <thead><tr><th>Spot type</th><th className="num">Hands</th><th className="num">Avg loss</th></tr></thead>
                <tbody>
                  {summary.by_category.map((c) => (
                    <tr key={c.category}><td>{c.category}</td><td className="num">{c.n}</td><td className="num">{m(c.avg_loss_bb, 2)}</td></tr>
                  ))}
                </tbody>
              </table>
              <p className="hint">Leak statistics with sample sizes and confidence come in Milestone 3.</p>
            </>
          )}
        </main>
      )}
    </div>
  );
}
