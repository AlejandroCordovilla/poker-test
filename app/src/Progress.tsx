import { useEffect, useState } from "react";
import { api, Summary } from "./api";

export function Progress({ m }: { m: (x: number, d?: number) => string }) {
  const [s, setS] = useState<Summary | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => { api.summary().then(setS).catch((e) => setError(String(e))); }, []);
  if (error) return <main className="main"><div className="error">{error}</div></main>;
  if (!s) return <main className="main" />;
  const maxLoss = Math.max(0.01, ...s.by_day.map((d) => d.avg_loss_bb));
  return (
    <main className="main progress">
      <div className="stats">
        <div><div className="v">{s.attempts}</div><div className="l">decisions graded</div></div>
        <div><div className="v">{m(s.avg_loss_bb, 2)}</div><div className="l">average EV loss ({s.avg_loss_pct.toFixed(1)}% of pot)</div></div>
        <div><div className="v">{Math.round(s.good_pct)}%</div><div className="l">Best or Correct</div></div>
      </div>
      {s.by_day.length > 0 && (
        <>
          <h3>Average EV loss by day</h3>
          <div className="trend">
            {s.by_day.map((d) => (
              <div key={d.day} className="trend-col" title={`${d.day}: ${d.n} decisions, ${Math.round(d.good_pct)}% good`}>
                <div className="trend-bar" style={{ height: `${(d.avg_loss_bb / maxLoss) * 100}%` }} />
                <div className="trend-label">{d.day.slice(5)}</div>
              </div>
            ))}
          </div>
        </>
      )}
      <h3>Where you lose the most</h3>
      <table className="cats">
        <thead><tr><th>Spot type</th><th className="num">Decisions</th><th className="num">Avg loss</th></tr></thead>
        <tbody>
          {s.by_category.map((c) => (
            <tr key={c.category}><td>{c.category}</td><td className="num">{c.n}</td><td className="num">{m(c.avg_loss_bb, 2)}</td></tr>
          ))}
        </tbody>
      </table>
    </main>
  );
}
