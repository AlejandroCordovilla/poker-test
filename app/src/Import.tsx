import { Fragment, useEffect, useMemo, useState } from "react";
import { DecisionRow, HandRow, ImportProgress, importApi, Preview } from "./api";
import { Replayer } from "./Replayer";
import { prettyCards } from "./units";

const STATUS_LABEL: Record<string, string> = {
  scored: "Scored", partial: "Partial", queued: "Solving…", "preflop only": "Preflop only", skipped: "Not modeled",
};
const VERDICT_CLASS: Record<string, string> = { Best: "best", Correct: "correct", Inaccuracy: "inacc", Wrong: "wrong", Blunder: "blunder" };

/** Import PokerNow logs; every decision graded against the solver (like an "Analyze" page). */
export function Import({ m, t }: { m: (x: number, d?: number) => string; t: (s: string) => string }) {
  const [text, setText] = useState<string | null>(null);
  const [file, setFile] = useState("");
  const [preview, setPreview] = useState<Preview | null>(null);
  const [hero, setHero] = useState("");
  const [aliases, setAliases] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [progress, setProgress] = useState<ImportProgress | null>(null);
  const [hands, setHands] = useState<HandRow[]>([]);
  const [status, setStatus] = useState<string>("all");
  const [sort, setSort] = useState<"loss" | "date">("loss");
  const [open, setOpen] = useState<string | null>(null);
  const [detail, setDetail] = useState<DecisionRow[]>([]);
  const [replay, setReplay] = useState<string | null>(null);

  const refresh = () => importApi.hands().then(setHands).catch((e) => setError(String(e)));
  useEffect(() => { refresh(); }, []);
  useEffect(() => {
    const id = setInterval(async () => {
      try {
        const p = await importApi.status();
        setProgress((old) => {
          if (old && p.done + p.failed !== old.done + old.failed) refresh();
          return p;
        });
      } catch {}
    }, 2000);
    return () => clearInterval(id);
  }, []);
  useEffect(() => { if (open) importApi.detail(open).then(setDetail).catch((e) => setError(String(e))); }, [open, hands]);

  const pick = async (f: File) => {
    setError(null); setMsg(null);
    const txt = await f.text();
    try {
      const p = await importApi.preview(txt);
      setText(txt); setFile(f.name); setPreview(p);
      setHero(p.hero ?? p.players[0]?.[0] ?? "");
      setAliases(Object.fromEntries(p.players.map(([n]) => [n, p.aliases[n] ?? ""])));
    } catch (e) { setError(String(e)); }
  };

  const run = async () => {
    if (!text) return;
    setBusy(true); setError(null);
    try {
      const r = await importApi.run(text, file, hero, aliases);
      setMsg(`Imported ${r.imported} hands (${r.skipped_existing} already in). ${r.preflop_decisions} preflop decisions graded; ${r.postflop_queued} hands queued for postflop solving.`);
      setPreview(null); setText(null);
      refresh();
    } catch (e) { setError(String(e)); } finally { setBusy(false); }
  };

  const shown = useMemo(() => {
    let h = hands.filter((x) => status === "all" || x.status === status);
    h = [...h].sort((a, b) => (sort === "loss" ? b.ev_loss - a.ev_loss : b.played.localeCompare(a.played)));
    return h;
  }, [hands, status, sort]);

  const counts = useMemo(() => {
    const c: Record<string, number> = {};
    hands.forEach((h) => { c[h.status] = (c[h.status] ?? 0) + 1; });
    return c;
  }, [hands]);
  const totalLoss = hands.reduce((s, h) => s + h.ev_loss, 0);
  const totalNet = hands.reduce((s, h) => s + h.net_bb, 0);
  const pending = progress ? progress.queued - progress.done - progress.failed : 0;

  return (
    <main className="main import">
      {replay && <Replayer handId={replay} onClose={() => setReplay(null)} m={m} t={t} />}
      {error && <div className="error">{error}</div>}
      <div className="import-head">
        <label className="primary file-btn">
          Import PokerNow log…
          <input type="file" accept=".csv,text/csv" onChange={(e) => e.target.files?.[0] && pick(e.target.files[0])} />
        </label>
        <span className="hint inline">In PokerNow: game log → Download full log (.csv).</span>
      </div>

      {preview && (
        <div className="card-panel">
          <h3>{file}</h3>
          <p className="hint">{preview.hands} hands, {preview.with_cards} with your hole cards{preview.already ? `, ${preview.already} already imported` : ""}.</p>
          <div className="players">
            <div className="filter-label">Which one is you?</div>
            <select value={hero} onChange={(e) => setHero(e.target.value)}>
              {preview.players.map(([n, c]) => <option key={n} value={n}>{n} ({c} hands)</option>)}
            </select>
            <div className="filter-label" style={{ marginTop: 12 }}>Friendly names (used in leak filters and the "vs" view)</div>
            {preview.players.filter(([n]) => n !== hero).map(([n, c]) => (
              <div key={n} className="alias-row">
                <span>{n} <span className="muted">({c})</span></span>
                <input placeholder="e.g. Sam" value={aliases[n] ?? ""} onChange={(e) => setAliases({ ...aliases, [n]: e.target.value })} />
              </div>
            ))}
          </div>
          <button className="primary" disabled={busy || !hero} onClick={run}>{busy ? "Grading preflop…" : "Import and grade"}</button>
        </div>
      )}
      {msg && <div className="note">{msg}</div>}

      {progress && pending > 0 && (
        <div className="card-panel progress-panel">
          <div>Postflop solving: {progress.done + progress.failed} of {progress.queued} hands{progress.current ? ` · now ${progress.current}` : ""}</div>
          <div className="bar wide"><div style={{ width: `${(100 * (progress.done + progress.failed)) / Math.max(1, progress.queued)}%` }} /></div>
          <div className="hint">About {Math.round(progress.last_seconds || 45)}s per hand; roughly {Math.ceil((pending * (progress.last_seconds || 45)) / 60)} min left. You can keep drilling meanwhile.</div>
        </div>
      )}

      {hands.length > 0 && (
        <>
          <div className="stats">
            <div><div className="v">{hands.length}</div><div className="l">hands imported</div></div>
            <div><div className="v">{m(totalLoss, 1)}</div><div className="l">EV lost to mistakes</div></div>
            <div><div className="v">{m(totalNet, 1)}</div><div className="l">actual result</div></div>
          </div>
          <div className="leak-controls">
            <select value={status} onChange={(e) => setStatus(e.target.value)}>
              <option value="all">All hands</option>
              {Object.entries(counts).map(([s, c]) => <option key={s} value={s}>{STATUS_LABEL[s] ?? s} ({c})</option>)}
            </select>
            <div className="seg small">
              <button className={sort === "loss" ? "on" : ""} onClick={() => setSort("loss")}>Biggest mistakes</button>
              <button className={sort === "date" ? "on" : ""} onClick={() => setSort("date")}>Newest</button>
            </div>
          </div>
          <table className="cats hands">
            <thead><tr><th>#</th><th>Hand</th><th>Board</th><th>Pos</th><th>Vs</th><th>Status</th><th className="num">Graded</th><th className="num">EV lost</th><th className="num">Result</th></tr></thead>
            <tbody>
              {shown.map((h) => (
                <Fragment key={h.hand_id}>
                  <tr className={`clickable ${open === h.hand_id ? "sel" : ""}`} onClick={() => setOpen(open === h.hand_id ? null : h.hand_id)}>
                    <td className="muted">{h.hand_no}</td>
                    <td>{prettyCards(h.hero_cards)}</td>
                    <td>{prettyCards(h.board)}</td>
                    <td>{h.hero_pos}</td>
                    <td>{h.villain}</td>
                    <td><span className={`status ${h.status.replace(" ", "-")}`} title={h.note}>{STATUS_LABEL[h.status] ?? h.status}</span></td>
                    <td className="num">{h.decisions}</td>
                    <td className="num">{h.ev_loss > 0.005 ? m(h.ev_loss, 2) : "-"}</td>
                    <td className={`num ${h.net_bb >= 0 ? "won" : "lost"}`}>{m(h.net_bb, 1)}</td>
                  </tr>
                  {open === h.hand_id && (
                    <tr className="detail-row">
                      <td colSpan={9}>
                        <button className="primary" onClick={() => setReplay(h.hand_id)}>▶ Replay this hand</button>
                        {h.note && <div className="hint">{h.note}</div>}
                        {detail.length === 0 ? <div className="hint">No graded decisions in this hand.</div> : (
                          <table className="cats inner">
                            <thead><tr><th>Street</th><th>Spot</th><th>You</th><th>Solver</th><th className="num">Lost</th><th>Grade</th></tr></thead>
                            <tbody>
                              {detail.map((d, i) => (
                                <tr key={i}>
                                  <td>{d.street}</td>
                                  <td className="muted">{d.situation.replace("pf_", "").replace(/_/g, " ")}</td>
                                  <td>{t(d.chosen)}</td>
                                  <td>{t(d.best)}</td>
                                  <td className="num">{m(d.ev_loss_bb, 2)} <span className="muted">({d.ev_loss_pct.toFixed(1)}%)</span></td>
                                  <td className={`vtext ${VERDICT_CLASS[d.verdict] ?? ""}`}>{d.verdict}</td>
                                </tr>
                              ))}
                            </tbody>
                          </table>
                        )}
                      </td>
                    </tr>
                  )}
                </Fragment>
              ))}
            </tbody>
          </table>
        </>
      )}
    </main>
  );
}
