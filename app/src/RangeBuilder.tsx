import { useEffect, useMemo, useState } from "react";
import { rangeApi, RangeTask } from "./api";
import { Card } from "./Card";

const RANKS = "AKQJT98765432";
const KIND_COLOR: Record<string, string> = { fold: "#7c8ea3", check: "#5f9e74", call: "#3d8a5a", bet: "#d0662b", raise: "#c0453a", allin: "#8f2430" };

const SCENARIOS: Record<number, [string, string][]> = {
  3: [
    ["open:BTN", "BTN first in"], ["open:SB", "SB first in"],
    ["vs_open:BTN:BB", "BB vs BTN open"], ["vs_open:BTN:SB", "SB vs BTN open"], ["vs_open:SB:BB", "BB vs SB open"],
    ["vs_3bet:BTN:BB", "BTN vs BB 3-bet"], ["vs_3bet:BTN:SB", "BTN vs SB 3-bet"], ["vs_3bet:SB:BB", "SB vs BB 3-bet"],
  ],
  6: [
    ["open:UTG", "UTG first in"], ["open:HJ", "HJ first in"], ["open:CO", "CO first in"], ["open:BTN", "BTN first in"], ["open:SB", "SB first in"],
    ["vs_open:CO:BTN", "BTN vs CO open"], ["vs_open:BTN:BB", "BB vs BTN open"], ["vs_open:CO:BB", "BB vs CO open"], ["vs_open:SB:BB", "BB vs SB open"],
    ["vs_3bet:CO:BTN", "CO vs BTN 3-bet"], ["vs_3bet:BTN:BB", "BTN vs BB 3-bet"],
  ],
};

function shade(kind: string, i: number, kinds: string[]) {
  // two bet sizes of the same kind get slightly different shades
  const same = kinds.slice(0, i).filter((k) => k === kind).length;
  const base = KIND_COLOR[kind] ?? "#999";
  return same === 0 ? base : same === 1 ? base + "bb" : base + "88";
}

/** Paint a full range on the 13x13 grid, then grade it hand by hand against the solver. */
export function RangeBuilder({ m }: { m: (x: number, d?: number) => string }) {
  const [kind, setKind] = useState<"preflop" | "flop">("preflop");
  const [table, setTable] = useState(3);
  const [stack, setStack] = useState(50);
  const [scenario, setScenario] = useState("open:BTN");
  const [matchup, setMatchup] = useState("BTN vs BB");
  const [potType, setPotType] = useState("srp");
  const [who, setWho] = useState("oop");
  const [task, setTask] = useState<RangeTask | null>(null);
  const [paint, setPaint] = useState<(number | null)[]>(Array(169).fill(null));
  const [brush, setBrush] = useState(0);
  const [down, setDown] = useState(false);
  const [checked, setChecked] = useState(false);
  const [showSolver, setShowSolver] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = async () => {
    setError(null); setChecked(false); setShowSolver(false);
    try {
      const t = await rangeApi.task(kind === "preflop" ? { kind, table, stack, scenario } : { kind, stack, matchup, potType, who });
      setTask(t);
      setPaint(Array(169).fill(null));
      setBrush(t.kinds.length > 1 ? 1 : 0);
    } catch (e) { setTask(null); setError(String(e)); }
  };
  useEffect(() => { load(); }, []); // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(() => { const up = () => setDown(false); window.addEventListener("mouseup", up); return () => window.removeEventListener("mouseup", up); }, []);

  const inRange = (c: number) => (task?.classes[c].weight ?? 0) > 1e-5;
  const apply = (c: number) => { if (!checked && inRange(c)) setPaint((p) => { const q = [...p]; q[c] = brush; return q; }); };

  const result = useMemo(() => {
    if (!task || !checked) return null;
    let w = 0, loss = 0, acc = 0, unpainted = 0;
    const per: { loss: number; freq: number }[] = [];
    task.classes.forEach((c, i) => {
      if (c.weight <= 1e-5) { per.push({ loss: 0, freq: 1 }); return; }
      const a = paint[i] ?? 0;
      if (paint[i] === null) unpainted++;
      const best = Math.max(...c.ev);
      const l = Math.max(0, best - c.ev[a]);
      per.push({ loss: l, freq: c.freq[a] });
      w += c.weight; loss += c.weight * l; acc += c.weight * c.freq[a];
    });
    return { per, loss: loss / w, lossPct: (100 * loss / w) / Math.max(task.pot_bb, 0.01), acc: acc / w, unpainted };
  }, [task, checked, paint]);

  const cellStyle = (c: number) => {
    if (!task) return {};
    if (!inRange(c)) return { background: "transparent", color: "var(--muted)", opacity: 0.35 };
    if (showSolver || (checked && paint[c] === null)) {
      let acc = 0;
      const stops = task.classes[c].freq.map((f, i) => { const s = `${shade(task.kinds[i], i, task.kinds)} ${acc * 100}% ${(acc + f) * 100}%`; acc += f; return s; });
      return { background: `linear-gradient(90deg, ${stops.join(", ")})`, color: "#fff" };
    }
    const a = paint[c];
    return a === null ? { background: "var(--bg)" } : { background: shade(task.kinds[a], a, task.kinds), color: "#fff" };
  };

  const mark = (c: number) => {
    if (!result || !task || !inRange(c)) return "";
    const r = result.per[c];
    if (r.freq < 0.05 && r.loss > 0.02 * task.pot_bb) return "bad";
    if (r.loss > 0.01 * task.pot_bb) return "meh";
    return "ok";
  };

  const tip = (c: number) => {
    if (!task) return "";
    const k = task.classes[c];
    if (!inRange(c)) return `${k.name}: not in your range here`;
    const mix = task.actions.map((a, i) => `${a} ${Math.round(k.freq[i] * 100)}% (EV ${k.ev[i].toFixed(2)})`).join(", ");
    return `${k.name}: solver ${mix}${paint[c] !== null ? ` · you: ${task.actions[paint[c]!]}` : ""}`;
  };

  return (
    <main className="main ranges">
      {error && <div className="error">{error}</div>}
      <div className="leak-controls">
        <div className="seg small">
          <button className={kind === "preflop" ? "on" : ""} onClick={() => setKind("preflop")}>Preflop</button>
          <button className={kind === "flop" ? "on" : ""} onClick={() => setKind("flop")}>Flop</button>
        </div>
        <select value={stack} onChange={(e) => setStack(+e.target.value)}>{[40, 50, 100].map((s) => <option key={s} value={s}>{s}bb</option>)}</select>
        {kind === "preflop" ? (
          <>
            <select value={table} onChange={(e) => { setTable(+e.target.value); setScenario(SCENARIOS[+e.target.value][0][0]); }}>
              <option value={3}>3-handed</option><option value={6}>4-6 handed</option>
            </select>
            <select value={scenario} onChange={(e) => setScenario(e.target.value)}>{SCENARIOS[table].map(([v, l]) => <option key={v} value={v}>{l}</option>)}</select>
          </>
        ) : (
          <>
            <select value={matchup} onChange={(e) => setMatchup(e.target.value)}>{["BTN vs BB", "SB vs BB", "BTN vs SB"].map((x) => <option key={x}>{x}</option>)}</select>
            <select value={potType} onChange={(e) => setPotType(e.target.value)}><option value="srp">Single-raised</option><option value="3bp">3-bet pot</option></select>
            <select value={who} onChange={(e) => setWho(e.target.value)}><option value="oop">Out of position, first to act</option><option value="ip">In position, checked to</option></select>
          </>
        )}
        <button className="primary" onClick={load}>{kind === "flop" ? "Deal a flop" : "Load"}</button>
      </div>

      {task && (
        <div className="rb">
          <div className="rb-left">
            <h3>{task.title}</h3>
            <p className="hint">{task.description}</p>
            {task.board.length > 0 && <div className="board small-board">{task.board.map((c) => <Card key={c} card={c} />)}</div>}
            <div className="grid13" onMouseLeave={() => setDown(false)}>
              {Array.from({ length: 13 }, (_, ri) =>
                Array.from({ length: 13 }, (_, ci) => {
                  const r1 = 12 - ri, r2 = 12 - ci;
                  const c = 13 * r1 + r2;
                  const name = ri === ci ? RANKS[ri] + RANKS[ri] : ci > ri ? RANKS[ri] + RANKS[ci] + "s" : RANKS[ci] + RANKS[ri] + "o";
                  return (
                    <div key={c} className={`cell ${mark(c)}`} style={cellStyle(c)} title={tip(c)}
                      onMouseDown={() => { setDown(true); apply(c); }} onMouseEnter={() => { if (down) apply(c); }}>
                      {name}
                    </div>
                  );
                }),
              )}
            </div>
          </div>
          <div className="rb-right">
            <div className="filter-label">Brush</div>
            <div className="palette">
              {task.actions.map((a, i) => (
                <button key={i} className={`swatch ${brush === i ? "on" : ""}`} onClick={() => setBrush(i)} disabled={checked}>
                  <span className="dot" style={{ background: shade(task.kinds[i], i, task.kinds) }} /> {a}
                </button>
              ))}
            </div>
            <div className="palette-actions">
              {!checked && <button className="mini" onClick={() => setPaint(task.classes.map((_, c) => (inRange(c) ? brush : null)))}>Fill all with brush</button>}
              {!checked && <button className="mini" onClick={() => setPaint(Array(169).fill(null))}>Clear</button>}
            </div>
            <p className="hint">Click or drag to paint. Unpainted hands count as {task.actions[0]}.</p>
            {!checked ? (
              <button className="primary wide" onClick={() => setChecked(true)}>Check my range</button>
            ) : result && (
              <div className="rb-result">
                <div className="stats small">
                  <div><div className="v">{Math.round(result.acc * 100)}%</div><div className="l">matches the solver</div></div>
                  <div><div className="v">{m(result.loss, 2)}</div><div className="l">EV lost per hand ({result.lossPct.toFixed(1)}% of pot)</div></div>
                </div>
                <label className="toggle"><input type="checkbox" checked={showSolver} onChange={(e) => setShowSolver(e.target.checked)} /> Show the solver's range</label>
                <p className="hint">Red outline: the solver almost never does what you painted and it costs EV. Yellow: costs a little. Hover any hand for the solver's mix and EVs.</p>
                <button className="primary wide" onClick={load}>Next</button>
              </div>
            )}
          </div>
        </div>
      )}
    </main>
  );
}
