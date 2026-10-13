import { useCallback, useEffect, useRef, useState } from "react";
import { api, DrillView, Feedback, Filters, GameMode, HandResult, Meta, ReviewItem } from "./api";
import { LessonView } from "./LessonView";
import { TableView } from "./TableView";
import { Card } from "./Card";

type Mode = "random" | "focused" | "leaks";
export const EMPTY: Filters = { stacks: [], matchups: [], pot_types: [], streets: [], textures: [], hand_buckets: [], close_only: false, positions: [], big_bets: "" };
const POT_NAME: Record<string, string> = { srp: "single-raised pot", "3bp": "3-bet pot" };
const VERDICT_CLASS: Record<string, string> = { Best: "best", Correct: "correct", Inaccuracy: "inacc", Wrong: "wrong", Blunder: "blunder" };
const GOOD = ["Best", "Correct"];

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

function load<T>(k: string, d: T): T {
  try { const v = localStorage.getItem(k); return v ? (JSON.parse(v) as T) : d; } catch { return d; }
}
function save(k: string, v: unknown) {
  try { localStorage.setItem(k, JSON.stringify(v)); } catch {}
}

export type Session = { n: number; loss: number; good: number; mistakes: number; mistakeLoss: number };

export function Drill({ meta, m, t, redrill, onRedrillDone, onAnswered }: {
  meta: Meta | null;
  m: (x: number, d?: number) => string;
  t: (s: string) => string;
  redrill: number | null;
  onRedrillDone: () => void;
  onAnswered: (fb: Feedback) => void;
}) {
  const [mode, setMode] = useState<Mode>("random");
  const [gameMode, setGameMode] = useState<GameMode>(() => load("gameMode", "spot" as GameMode));
  const [rngOn, setRngOn] = useState<boolean>(() => load("rngOn", false));
  const [pauseAll, setPauseAll] = useState<boolean>(() => load("pauseAll", true));
  const [filters, setFilters] = useState<Filters>(() => ({ ...EMPTY, ...load("filters", {}) }));
  const [drill, setDrill] = useState<DrillView | null>(null);
  const [fb, setFb] = useState<Feedback | null>(null);
  const [roll, setRoll] = useState<number | null>(null);
  const [events, setEvents] = useState<string[]>([]);
  const [result, setResult] = useState<HandResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [timeline, setTimeline] = useState<ReviewItem[]>([]);
  const [viewing, setViewing] = useState<ReviewItem | null>(null);
  const refreshTimeline = () => api.review().then(setTimeline).catch(() => setTimeline([]));

  useEffect(() => save("gameMode", gameMode), [gameMode]);
  useEffect(() => save("rngOn", rngOn), [rngOn]);
  useEffect(() => save("pauseAll", pauseAll), [pauseAll]);
  useEffect(() => save("filters", filters), [filters]);

  const show = (v: DrillView) => {
    setDrill(v);
    setFb(null);
    setViewing(null);
    refreshTimeline();
    setRoll(rngOn ? 1 + Math.floor(Math.random() * 100) : null);
  };

  const next = useCallback(async () => {
    setError(null);
    setEvents([]);
    setResult(null);
    try {
      show(await api.newDrill(mode === "focused" ? filters : EMPTY, mode, gameMode));
    } catch (e) {
      setDrill(null);
      setError(String(e));
    }
  }, [mode, filters, gameMode, rngOn]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => { if (meta && meta.spots > 0 && redrill === null) next(); }, [meta, mode, gameMode]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    if (redrill === null) return;
    api.redrill(redrill).then((v) => { setEvents([]); setResult(null); setError(null); show(v); }).catch((e) => setError(String(e))).finally(onRedrillDone);
  }, [redrill]); // eslint-disable-line react-hooks/exhaustive-deps

  const advance = useCallback(async () => {
    setBusy(true);
    try {
      const step = await api.continueHand();
      setEvents(step.events);
      if (step.result) { setResult(step.result); setFb(fb); }
      else if (step.next) show(step.next);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }, [fb, rngOn]); // eslint-disable-line react-hooks/exhaustive-deps

  const replayFrom = async (i: number) => {
    try {
      const v = await api.rewind(i);
      setEvents([]);
      setResult(null);
      show(v);
    } catch (e) { setError(String(e)); }
  };

  const advanceRef = useRef(advance);
  advanceRef.current = advance;

  const choose = useCallback(async (i: number) => {
    if (!drill || fb || busy) return;
    try {
      const f = await api.answer(i, roll);
      setFb(f);
      onAnswered(f);
      refreshTimeline();
      if (f.continues && drill.actions[i].kind === "fold") {
        // folding ends the hand: resolve it right away instead of showing "Continue"
        const step = await api.continueHand();
        setEvents(step.events);
        if (step.result) setResult(step.result);
        return;
      }
      if (f.continues && !pauseAll && GOOD.includes(f.verdict)) {
        // "pause only after mistakes": keep playing
        setTimeout(() => advanceRef.current?.(), 350);
      }
    } catch (e) {
      setError(String(e));
    }
  }, [drill, fb, busy, roll, pauseAll, onAnswered]);


  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.target as HTMLElement)?.tagName === "INPUT") return;
      if (fb && (e.key === "Enter" || e.key === " ")) {
        e.preventDefault();
        if (fb.continues && !result) advance(); else next();
        return;
      }
      const k = parseInt(e.key, 10);
      if (!fb && drill && k >= 1 && k <= drill.actions.length) choose(k - 1);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [fb, drill, result, next, choose, advance]);

  const handOver = !!result;

  return (
    <div className="drill-layout">
      <aside className="side">
        <div className="filter-label">Game</div>
        <div className="seg">
          {(["spot", "street", "hand"] as GameMode[]).map((g) => (
            <button key={g} className={gameMode === g ? "on" : ""} onClick={() => setGameMode(g)}>
              {g === "spot" ? "Spot" : g === "street" ? "Street" : "Full hand"}
            </button>
          ))}
        </div>
        {gameMode !== "spot" && (
          <label className="toggle"><input type="checkbox" checked={!pauseAll} onChange={(e) => setPauseAll(!e.target.checked)} /> Pause only after mistakes</label>
        )}
        <label className="toggle"><input type="checkbox" checked={rngOn} onChange={(e) => setRngOn(e.target.checked)} /> RNG mode (roll decides the mix)</label>

        <div className="filter-label" style={{ marginTop: 14 }}>Deal</div>
        <div className="seg">
          {(["random", "focused", "leaks"] as Mode[]).map((x) => (
            <button key={x} className={mode === x ? "on" : ""} onClick={() => setMode(x)}>
              {x === "random" ? "Random" : x === "focused" ? "Focused" : "My leaks"}
            </button>
          ))}
        </div>
        {mode === "focused" && meta && (
          <div className="filters">
            {gameMode === "spot" && (
              <div className="filter">
                <div className="filter-label">Big bets</div>
                <div className="chips">
                  {[["", "Any"], ["facing", "Facing a big bet"], ["making", "When to bet big"]].map(([v, l]) => (
                    <button key={v} className={`chip ${filters.big_bets === v ? "on" : ""}`} onClick={() => setFilters({ ...filters, big_bets: v })}>{l}</button>
                  ))}
                </div>
              </div>
            )}
            <Chips label="Stack" options={meta.stacks} value={filters.stacks} onChange={(v) => setFilters({ ...filters, stacks: v })} fmt={(s) => `${s}bb`} />
            <Chips label="Matchup" options={meta.matchups} value={filters.matchups} onChange={(v) => setFilters({ ...filters, matchups: v })} />
            <Chips label="Pot" options={meta.pot_types} value={filters.pot_types} onChange={(v) => setFilters({ ...filters, pot_types: v })} fmt={(p) => (p === "srp" ? "Single-raised" : "3-bet")} />
            <Chips label="Position" options={["IP", "OOP"]} value={filters.positions} onChange={(v) => setFilters({ ...filters, positions: v })} fmt={(p) => (p === "IP" ? "In position" : "Out of position")} />
            {gameMode !== "hand" && <Chips label="Street" options={["flop", "turn"]} value={filters.streets} onChange={(v) => setFilters({ ...filters, streets: v })} fmt={(s) => s[0].toUpperCase() + s.slice(1)} />}
            {gameMode === "spot" && <Chips label="Your hand" options={["Nuts", "Strong", "Medium", "Weak", "Draw", "Air"]} value={filters.hand_buckets} onChange={(v) => setFilters({ ...filters, hand_buckets: v })} />}
            <Chips label="Board" options={meta.textures} value={filters.textures} onChange={(v) => setFilters({ ...filters, textures: v })} />
            {gameMode === "spot" && <label className="toggle"><input type="checkbox" checked={filters.close_only} onChange={(e) => setFilters({ ...filters, close_only: e.target.checked })} /> Close decisions only</label>}
            <button className="primary wide" onClick={next}>Deal</button>
          </div>
        )}
        {mode === "leaks" && <p className="hint">Deals spots from the category where you lose the most EV (needs 5+ drills in a category).</p>}
        {mode === "random" && <p className="hint">Any solved spot, weighted by how often its flop type comes up in real games.</p>}
        <p className="hint keys">Keys: 1-{drill?.actions.length ?? 4} pick an action, Enter to continue or deal.</p>
      </aside>

      <main className="main">
        {error && <div className="error">{error}</div>}
        {drill && (
          <>
            <div className="spot-head">
              <div className="spot-title">{t(`${drill.stack}bb`)} stacks · {drill.matchup} · {POT_NAME[drill.pot_type]}</div>
              <div className="spot-sub">
                Preflop: {drill.preflop}. {drill.texture} board.
                {drill.mode !== "spot" && <> Decision {drill.decision}.</>}
                {drill.live_solved && <span className="tag">river solved live</span>}
              </div>
            </div>

            <TableView drill={viewing ? viewing.view : drill} result={viewing ? null : result} m={m} t={t} />

            {drill.mode !== "spot" && timeline.length > 0 && (
              <div className="timeline">
                {timeline.map((it) => (
                  <button key={it.index} className={`tl ${it.feedback ? VERDICT_CLASS[it.feedback.verdict] : "open"} ${viewing?.index === it.index ? "sel" : ""}`}
                    onClick={() => setViewing(viewing?.index === it.index || !it.feedback ? null : it)} title={it.feedback ? `${it.feedback.verdict}` : "current decision"}>
                    {it.index + 1}. {it.view.street}{it.feedback ? ` · ${it.feedback.verdict}` : " · now"}
                  </button>
                ))}
                <span className="tl-hint">click a decision to look back</span>
              </div>
            )}

            {viewing && viewing.feedback && (
              <div className="feedback viewing">
                <div className="viewing-head">Looking back at decision {viewing.index + 1} ({viewing.view.street}): you chose <b>{t(viewing.view.actions[viewing.feedback.chosen].label)}</b></div>
                <div className={`verdict ${VERDICT_CLASS[viewing.feedback.verdict]}`}><span className="v">{viewing.feedback.verdict}</span><span>{viewing.feedback.score.ev_loss_bb < 0.005 ? "no EV lost" : `lost ${m(viewing.feedback.score.ev_loss_bb, 2)} (${viewing.feedback.score.ev_loss_pct.toFixed(1)}% of pot)`}</span></div>
                <LessonView fb={viewing.feedback} t={t} />
                <div className="row-btns">
                  <button className="primary" onClick={() => replayFrom(viewing.index)}>Replay from here</button>
                  <button className="mini" onClick={() => setViewing(null)}>Back to the hand</button>
                </div>
              </div>
            )}

            {events.length > 0 && <div className="events">{t(events.join(" → "))}</div>}

            {!handOver && !viewing && (
              <>
                <div className="prompt">
                  {drill.node_type === "facing a bet" || drill.node_type === "facing a raise" ? `Facing ${m(drill.to_call_bb)} to call.` : drill.node_type === "checked to" ? "Checked to you." : "You're first to act."} What do you do?
                  {roll !== null && <span className="rng">RNG {roll}</span>}
                </div>
                <div className="actions">
                  {drill.actions.map((a) => {
                    const r = fb?.actions[a.index];
                    const cls = fb ? (a.index === fb.score.target ? "best" : a.index === fb.chosen ? "chosen" : "") : "";
                    return (
                      <button key={a.index} className={`action ${a.kind} ${cls}`} disabled={!!fb || busy} onClick={() => choose(a.index)}>
                        <span className="key">{a.index + 1}</span>
                        <span>{t(a.label)}</span>
                        {r && <span className="freq">{Math.round(r.freq_hand * 100)}%</span>}
                      </button>
                    );
                  })}
                </div>
              </>
            )}

            {fb && !viewing && (
              <div className="feedback">
                <div className={`verdict ${VERDICT_CLASS[fb.verdict]}`}>
                  <span className="v">{fb.verdict}</span>
                  <span>{fb.score.ev_loss_bb < 0.005 ? "no EV lost" : `lost ${m(fb.score.ev_loss_bb, 2)} (${fb.score.ev_loss_pct.toFixed(1)}% of pot)`}</span>
                  {fb.score.freq_diff >= 0.05 && <span className="tag">frequency gap {Math.round(fb.score.freq_diff * 100)} pts</span>}
                  {fb.score.rare && <span className="tag">solver almost never does this</span>}
                  {fb.score.mixed && <span className="tag">mixed spot</span>}
                </div>
                <div className="handline">You have {fb.hand_desc.made}{fb.hand_desc.draw ? ` + ${fb.hand_desc.draw}` : ""} · about {Math.round(fb.equity * 100)}% equity vs their range</div>
                <table className="ev">
                  <thead><tr><th>Action</th><th className="num">EV</th><th>This hand</th><th>Whole range</th></tr></thead>
                  <tbody>
                    {fb.actions.map((a, i) => (
                      <tr key={i} className={i === fb.score.target ? "best" : i === fb.chosen ? "chosen" : ""}>
                        <td>{t(a.label)}{i === fb.chosen ? " (you)" : ""}</td>
                        <td className="num">{m(a.ev_bb, 2)}</td>
                        <td><div className="bar"><div style={{ width: `${a.freq_hand * 100}%` }} /></div><span className="pct">{Math.round(a.freq_hand * 100)}%</span></td>
                        <td><div className="bar range"><div style={{ width: `${a.freq_range * 100}%` }} /></div><span className="pct">{Math.round(a.freq_range * 100)}%</span></td>
                      </tr>
                    ))}
                  </tbody>
                </table>
                <LessonView fb={fb} t={t} />
                {result && <div className={`result ${result.hero_won === true ? "won" : result.hero_won === false ? "lost" : ""}`}>{t(result.summary)}</div>}
                {result && timeline.length > 0 && (
                  <div className="hand-review">
                    <h3>Hand review</h3>
                    <table className="cats">
                      <thead><tr><th>#</th><th>Street</th><th>Board</th><th>You</th><th>Best</th><th className="num">Lost</th><th>Grade</th><th></th></tr></thead>
                      <tbody>
                        {timeline.filter((it) => it.feedback).map((it) => (
                          <tr key={it.index}>
                            <td>{it.index + 1}</td>
                            <td>{it.view.street}</td>
                            <td>{it.view.board.join(" ")}</td>
                            <td>{t(it.view.actions[it.feedback!.chosen].label)}</td>
                            <td>{t(it.view.actions[it.feedback!.score.target].label)}</td>
                            <td className="num">{m(it.feedback!.score.ev_loss_bb, 2)}</td>
                            <td className={`vtext ${VERDICT_CLASS[it.feedback!.verdict]}`}>{it.feedback!.verdict}</td>
                            <td><button className="mini" onClick={() => setViewing(it)}>Review</button></td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                    <div className="hint">Total lost this hand: {m(timeline.reduce((s, it) => s + (it.feedback?.score.ev_loss_bb ?? 0), 0), 2)}. Take your time; nothing moves until you press a button.</div>
                  </div>
                )}
                <div className="row-btns">
                  {fb.continues && !result
                    ? <button className="primary" disabled={busy} onClick={advance}>{busy ? "Playing…" : "Continue ↵"}</button>
                    : <button className="primary" onClick={next}>Next hand ↵</button>}
                  {(result || drill.mode === "spot") && <button className="mini" onClick={() => replayFrom(0)}>{drill.mode === "spot" ? "Try this spot again" : "Replay hand"}</button>}
                  {drill.mode !== "spot" && !result && timeline.length > 1 && <button className="mini" onClick={() => replayFrom(timeline.length - 2)}>← Back one decision</button>}
                </div>
              </div>
            )}
          </>
        )}
        {!drill && !error && meta && meta.spots === 0 && <div className="empty">No solved spots yet. They appear here as the solver finishes them.</div>}
      </main>
    </div>
  );
}
