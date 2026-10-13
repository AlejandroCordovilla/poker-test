import { useCallback, useEffect, useState } from "react";
import { api, Feedback, Meta } from "./api";
import { Drill, Session } from "./Drill";
import { Import } from "./Import";
import { Leaks } from "./Leaks";
import { MathDrill } from "./MathDrill";
import { Progress } from "./Progress";
import { Quickfire } from "./Quickfire";
import { RangeBuilder } from "./RangeBuilder";
import { convertText, loadUnits, money, saveUnits, Units } from "./units";

type Tab = "drill" | "preflop" | "math" | "ranges" | "leaks" | "import" | "progress";

export default function App() {
  const [tab, setTab] = useState<Tab>("drill");
  const [meta, setMeta] = useState<Meta | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [session, setSession] = useState<Session>({ n: 0, loss: 0, good: 0, mistakes: 0, mistakeLoss: 0 });
  const [units, setUnitsState] = useState<Units>(loadUnits);
  const [redrill, setRedrill] = useState<number | null>(null);
  const setUnits = (u: Units) => { setUnitsState(u); saveUnits(u); };
  const m = (x: number, d = 1) => money(x, units, d);
  const t = (s: string) => convertText(s, units);

  useEffect(() => { api.meta().then(setMeta).catch((e) => setError(String(e))); }, []);

  const onAnswered = useCallback((f: Feedback) => {
    const bad = !(f.verdict === "Best" || f.verdict === "Correct");
    setSession((s) => ({
      n: s.n + 1,
      loss: s.loss + f.score.ev_loss_bb,
      good: s.good + (bad ? 0 : 1),
      mistakes: s.mistakes + (bad ? 1 : 0),
      mistakeLoss: s.mistakeLoss + (bad ? f.score.ev_loss_bb : 0),
    }));
  }, []);

  return (
    <div className="app">
      <header className="top">
        <div className="brand">♠ Poker Trainer</div>
        <nav>
          {(["drill", "preflop", "math", "ranges", "leaks", "import", "progress"] as Tab[]).map((x) => (
            <button key={x} className={tab === x ? "on" : ""} onClick={() => setTab(x)}>
              {x === "drill" ? "Drill" : x === "preflop" ? "Preflop" : x === "math" ? "Math" : x === "ranges" ? "Ranges" : x === "leaks" ? "Leaks" : x === "import" ? "Real hands" : "Progress"}
            </button>
          ))}
        </nav>
        <div className="session">
          {session.n > 0 ? (
            <>
              {session.n} decision{session.n === 1 ? "" : "s"} · {m(session.loss / session.n, 2)} per decision · {Math.round((100 * session.good) / session.n)}% good
              {session.mistakes > 0 && <> · {session.mistakes} mistake{session.mistakes === 1 ? "" : "s"}, {m(session.mistakeLoss / session.mistakes, 2)} each</>}
            </>
          ) : meta ? <>{meta.spots} solved spots</> : null}
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
      {error && <div className="error">{error}</div>}
      <div style={{ display: tab === "drill" ? "contents" : "none" }}>
        <Drill meta={meta} m={m} t={t} redrill={redrill} onRedrillDone={() => setRedrill(null)} onAnswered={onAnswered} />
      </div>
      {tab === "leaks" && <Leaks m={m} t={t} onDrill={(id) => { setRedrill(id); setTab("drill"); }} />}
      {tab === "preflop" && <Quickfire m={m} />}
      {tab === "math" && <MathDrill />}
      {tab === "ranges" && <RangeBuilder m={m} />}
      {tab === "import" && <Import m={m} t={t} />}
      {tab === "progress" && <Progress m={m} />}
    </div>
  );
}
