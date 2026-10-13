import { useMemo, useState } from "react";
import { HandRow } from "./api";

type Point = { i: number; hand: HandRow; net: number; ideal: number };

/** Real-game results from imported hands: cumulative net, sessions, and net by opponent. */
export function Results({ hands, m }: { hands: HandRow[]; m: (x: number, d?: number) => string }) {
  const [hover, setHover] = useState<number | null>(null);

  const pts = useMemo(() => {
    const sorted = [...hands].sort((a, b) => a.played.localeCompare(b.played) || a.hand_no - b.hand_no);
    let net = 0, ideal = 0;
    return sorted.map((h, i): Point => {
      net += h.net_bb;
      ideal += h.net_bb + h.ev_loss;
      return { i, hand: h, net, ideal };
    });
  }, [hands]);

  const sessions = useMemo(() => {
    const by = new Map<string, { day: string; n: number; net: number; loss: number }>();
    for (const p of pts) {
      const day = localDay(p.hand.played);
      const s = by.get(day) ?? { day, n: 0, net: 0, loss: 0 };
      s.n++; s.net += p.hand.net_bb; s.loss += p.hand.ev_loss;
      by.set(day, s);
    }
    return [...by.values()].reverse();
  }, [pts]);

  const opps = useMemo(() => {
    const by = new Map<string, { name: string; n: number; net: number }>();
    for (const h of hands) {
      for (const name of h.opps.split(",").map((x) => x.trim()).filter(Boolean)) {
        const s = by.get(name) ?? { name, n: 0, net: 0 };
        s.n++; s.net += h.net_bb;
        by.set(name, s);
      }
    }
    return [...by.values()].filter((o) => o.n >= 5).sort((a, b) => a.net - b.net);
  }, [hands]);

  if (pts.length === 0) return null;
  const last = pts[pts.length - 1];
  const totalLoss = hands.reduce((s, h) => s + h.ev_loss, 0);
  const per100 = (100 * last.net) / pts.length;

  // chart geometry
  const W = 720, H = 220, L = 52, R = 12, T = 12, B = 26;
  const lo = Math.min(0, ...pts.map((p) => Math.min(p.net, p.ideal)));
  const hi = Math.max(0, ...pts.map((p) => Math.max(p.net, p.ideal)));
  const span = hi - lo || 1;
  const x = (i: number) => L + ((W - L - R) * i) / Math.max(1, pts.length - 1);
  const y = (v: number) => T + ((H - T - B) * (hi - v)) / span;
  const path = (f: (p: Point) => number) => pts.map((p, k) => `${k ? "L" : "M"}${x(p.i).toFixed(1)},${y(f(p)).toFixed(1)}`).join("");
  const ticks = niceTicks(lo, hi);
  const hp = hover !== null ? pts[hover] : null;
  const maxAbs = Math.max(0.01, ...opps.map((o) => Math.abs(o.net)));

  const onMove = (e: React.MouseEvent<SVGRectElement>) => {
    const r = e.currentTarget.getBoundingClientRect();
    const fx = ((e.clientX - r.left) / r.width) * W;
    const k = Math.round(((fx - L) / (W - L - R)) * (pts.length - 1));
    setHover(Math.max(0, Math.min(pts.length - 1, k)));
  };

  return (
    <section className="results">
      <h3>Real-game results</h3>
      <div className="stats">
        <div><div className="v">{pts.length}</div><div className="l">hands imported</div></div>
        <div><div className={`v ${last.net >= 0 ? "won" : "lost"}`}>{m(last.net, 1)}</div><div className="l">net result</div></div>
        <div><div className="v">{m(per100, 1)}</div><div className="l">per 100 hands</div></div>
        <div><div className="v">{m(totalLoss, 1)}</div><div className="l">EV lost to graded mistakes</div></div>
      </div>

      <div className="legend">
        <span><i className="sw net" /> Your result</span>
        <span><i className="sw ideal" /> Same cards, no graded mistakes</span>
      </div>
      <div className="chart-wrap">
        <svg viewBox={`0 0 ${W} ${H}`} className="chart" role="img" aria-label="Cumulative result by hand">
          {ticks.map((v) => (
            <g key={v}>
              <line x1={L} x2={W - R} y1={y(v)} y2={y(v)} className={v === 0 ? "axis0" : "grid"} />
              <text x={L - 6} y={y(v) + 4} className="tick" textAnchor="end">{m(v, 0)}</text>
            </g>
          ))}
          <text x={L} y={H - 6} className="tick">hand 1</text>
          <text x={W - R} y={H - 6} className="tick" textAnchor="end">hand {pts.length}</text>
          <path d={path((p) => p.ideal)} className="line ideal" />
          <path d={path((p) => p.net)} className="line net" />
          <circle cx={x(last.i)} cy={y(last.net)} r={4} className="dot net" />
          {hp && (
            <g>
              <line x1={x(hp.i)} x2={x(hp.i)} y1={T} y2={H - B} className="cross" />
              <circle cx={x(hp.i)} cy={y(hp.ideal)} r={4} className="dot ideal" />
              <circle cx={x(hp.i)} cy={y(hp.net)} r={4} className="dot net" />
            </g>
          )}
          <rect x={L} y={T} width={W - L - R} height={H - T - B} fill="transparent" onMouseMove={onMove} onMouseLeave={() => setHover(null)} />
        </svg>
        {hp && (
          <div className="tip" style={{ left: `${(100 * x(hp.i)) / W}%` }}>
            <div className="muted">Hand #{hp.hand.hand_no} · {localDay(hp.hand.played)}</div>
            <div>This hand: <b>{m(hp.hand.net_bb, 1)}</b>{hp.hand.ev_loss > 0.005 ? ` (${m(hp.hand.ev_loss, 2)} lost to mistakes)` : ""}</div>
            <div>Running total: <b>{m(hp.net, 1)}</b></div>
            <div className="muted">No-mistakes total: {m(hp.ideal, 1)}</div>
          </div>
        )}
      </div>
      <p className="hint">The gap between the lines is what your graded mistakes cost. The solid line also swings with the cards; the gap doesn't.</p>

      <div className="results-tables">
        <div>
          <h4>Sessions</h4>
          <table className="cats">
            <thead><tr><th>Day</th><th className="num">Hands</th><th className="num">Net</th><th className="num">Mistakes cost</th></tr></thead>
            <tbody>
              {sessions.map((s) => (
                <tr key={s.day}>
                  <td>{s.day}</td>
                  <td className="num">{s.n}</td>
                  <td className={`num ${s.net >= 0 ? "won" : "lost"}`}>{m(s.net, 1)}</td>
                  <td className="num">{s.loss > 0.005 ? m(s.loss, 1) : "-"}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        {opps.length > 0 && (
          <div>
            <h4>Pots you played against</h4>
            <table className="cats opps">
              <thead><tr><th>Player</th><th className="num">Hands</th><th>Your net</th></tr></thead>
              <tbody>
                {opps.map((o) => (
                  <tr key={o.name} title={`${o.n} hands where ${o.name} was still in at the end: ${m(o.net, 1)} total, ${m((100 * o.net) / o.n, 1)} per 100`}>
                    <td>{o.name}</td>
                    <td className="num">{o.n}</td>
                    <td>
                      <div className="dbar">
                        <div className="half neg">{o.net < 0 && <div style={{ width: `${(100 * -o.net) / maxAbs}%` }} />}</div>
                        <div className="half pos">{o.net > 0 && <div style={{ width: `${(100 * o.net) / maxAbs}%` }} />}</div>
                        <span className={o.net >= 0 ? "won" : "lost"}>{m(o.net, 1)}</span>
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
            <p className="hint">Hands where that player was still in when the pot was decided (5+ hands).</p>
          </div>
        )}
      </div>
    </section>
  );
}

function localDay(iso: string) {
  const d = new Date(iso);
  if (isNaN(d.getTime())) return iso.slice(0, 10);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

function niceTicks(lo: number, hi: number) {
  const raw = (hi - lo) / 4 || 1;
  const p = Math.pow(10, Math.floor(Math.log10(raw)));
  const step = [1, 2, 5, 10].map((k) => k * p).find((s) => s >= raw) ?? raw;
  const out: number[] = [];
  for (let v = Math.ceil(lo / step) * step; v <= hi + 1e-9; v += step) out.push(+v.toFixed(6));
  if (!out.includes(0)) out.push(0);
  return out;
}
