import { useEffect, useMemo, useState } from "react";
import { Feedback, importApi, Replay, ReplayAct } from "./api";
import { Card } from "./Card";
import { LessonView } from "./LessonView";
import { prettyCards } from "./units";

const STREET = ["Preflop", "Flop", "Turn", "River"];
const VERDICT_CLASS: Record<string, string> = { Best: "best", Correct: "correct", Inaccuracy: "inacc", Wrong: "wrong", Blunder: "blunder" };

type SeatState = { name: string; label: string; pos: string; stack: number; put: number; folded: boolean; action: string; kind: string; hero: boolean };
type Frame = { street: number; pot: number; seats: SeatState[]; act: ReplayAct | null; end: boolean };

/** Steps through a real imported hand, action by action, with the solver's grade at each of your decisions. */
export function Replayer({ handId, onClose, m, t }: { handId: string; onClose: () => void; m: (x: number, d?: number) => string; t: (s: string) => string }) {
  const [rep, setRep] = useState<Replay | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [k, setK] = useState(0);
  const [guess, setGuess] = useState<number | null>(null);

  useEffect(() => { importApi.replay(handId).then(setRep).catch((e) => setError(String(e))); }, [handId]);

  const frames = useMemo(() => (rep ? buildFrames(rep) : []), [rep]);
  const fbAt = useMemo(() => new Map((rep?.decisions ?? []).map((d) => [d.act_index, d.feedback])), [rep]);
  const go = (n: number) => { setK(Math.max(0, Math.min(frames.length - 1, n))); setGuess(null); };

  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (e.key === "ArrowRight") go(k + 1);
      else if (e.key === "ArrowLeft") go(k - 1);
      else if (e.key === "Home") go(0);
      else if (e.key === "End") go(frames.length - 1);
      else if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  });

  if (error) return <Modal onClose={onClose}><div className="error">{error}</div></Modal>;
  if (!rep || frames.length === 0) return <Modal onClose={onClose}><div className="hint">Loading…</div></Modal>;

  const f = frames[k];
  const bb = rep.hand.bb_size || 1;
  const shown = f.end ? rep.board : rep.board.slice(0, [0, 3, 4, 5][f.street]);
  const fb = f.act ? fbAt.get(f.act.index) ?? null : null;
  const next = frames[k + 1]?.act ?? null;
  const upcoming = next && next.player === rep.hero ? fbAt.get(next.index) ?? null : null;
  const shows = new Map(rep.hand.shows.map(([p, c]) => [p, c]));
  const won = new Map<string, number>();
  rep.hand.collected.forEach(([p, x]) => won.set(p, (won.get(p) ?? 0) + x));
  const streetStarts = [0, 1, 2, 3].map((s) => frames.findIndex((x) => x.act && x.act.street === s));
  const heroIdx = f.seats.findIndex((s) => s.hero);
  const end = frames[frames.length - 1];
  const net = ((end.seats.find((s) => s.hero)?.stack ?? 0) - (rep.hand.seats.find((s) => s[1] === rep.hero)?.[2] ?? 0)) / bb;
  const n = f.seats.length;

  return (
    <Modal onClose={onClose}>
      <div className="rp-head">
        <h3>Hand #{rep.hand.no} <span className="muted">· {new Date(rep.hand.played).toLocaleString()} · {m(rep.hand.seats.find((s) => s[1] === rep.hero)?.[2]! / bb, 0)} stack</span></h3>
        <div className="rp-streets">
          {STREET.map((s, i) => (
            <button key={s} className={`mini ${f.street === i && !f.end ? "on" : ""}`} disabled={streetStarts[i] < 0} onClick={() => go(streetStarts[i])}>{s}</button>
          ))}
          <button className={`mini ${f.end ? "on" : ""}`} onClick={() => go(frames.length - 1)}>Result</button>
        </div>
      </div>

      <div className="rp-table">
        {f.seats.map((s, i) => {
          const a = (2 * Math.PI * (i - heroIdx)) / n + Math.PI / 2; // hero at the bottom
          const style = { left: `${50 + 41 * Math.cos(a)}%`, top: `${50 + 33 * Math.sin(a)}%` };
          const cards = s.hero ? rep.hero_cards : f.end ? shows.get(s.name) : undefined;
          return (
            <div key={s.name} className={`rp-seat ${s.folded ? "folded" : ""} ${s.hero ? "me" : ""} ${f.act?.player === s.name ? "acting" : ""}`} style={style}>
              <div className="pseat-top"><span className="pos">{s.pos}</span>{rep.hand.dealer === s.name && <span className="dealer">D</span>}<span className="who">{s.label}</span></div>
              <div className="stack">{m(s.stack / bb)}</div>
              {cards && cards.length > 0 && <div className="vh">{cards.map((c) => <Card key={c} card={c} />)}</div>}
              {f.end && won.has(s.name) && <div className="bubble won">Wins {m(won.get(s.name)! / bb)}</div>}
              {!f.end && s.action && <div className={`bubble ${s.kind}`}>{s.action}</div>}
              {!f.end && s.put > 0 && <div className="rp-put">{m(s.put / bb)}</div>}
            </div>
          );
        })}
        <div className="felt-center">
          <div className="pot">Pot {m(f.pot / bb)}</div>
          <div className="board">{shown.map((c) => <Card key={c} card={c} size="lg" />)}</div>
        </div>
      </div>

      <div className="rp-controls">
        <button className="mini" onClick={() => go(0)} disabled={k === 0}>⏮</button>
        <button onClick={() => go(k - 1)} disabled={k === 0}>← Back</button>
        <span className="muted">{k + 1} / {frames.length}</span>
        <button className="primary" onClick={() => go(k + 1)} disabled={k === frames.length - 1}>Next →</button>
        <button className="mini" onClick={() => go(frames.length - 1)} disabled={k === frames.length - 1}>⏭</button>
        <span className="hint inline">Arrow keys work too.</span>
      </div>

      <div className="rp-panel">
        {upcoming ? (
          <Guess fb={upcoming} guess={guess} setGuess={setGuess} m={m} t={t} />
        ) : fb ? (
          <Graded fb={fb} m={m} t={t} />
        ) : f.end ? (
          <p>{resultLine(rep, m)} · <b className={net >= 0 ? "won" : "lost"}>Your net: {net >= 0 ? "+" : ""}{m(net)}</b></p>
        ) : (
          <p className="muted">{f.act ? `${f.seats.find((s) => s.name === f.act!.player)?.label}: ${describe(f.act, bb, m)}` : "Blinds posted."}{f.act?.player === rep.hero ? " (not graded: outside the solved spots)" : ""}</p>
        )}
      </div>
    </Modal>
  );
}

function Guess({ fb, guess, setGuess, m, t }: { fb: Feedback; guess: number | null; setGuess: (i: number) => void; m: (x: number, d?: number) => string; t: (s: string) => string }) {
  const best = Math.max(...fb.actions.map((a) => a.ev_bb));
  return (
    <div>
      <p><b>Your move.</b> What do you do here? <span className="muted">Pick one, or press Next to see what you did.</span></p>
      <div className="actions">
        {fb.actions.map((a, i) => (
          <button key={i} className={`action ${guess === i ? "chosen" : ""}`} onClick={() => setGuess(i)}>{t(a.label)}</button>
        ))}
      </div>
      {guess !== null && (() => {
        const loss = best - fb.actions[guess].ev_bb;
        const ok = loss < Math.max(0.05, 0.01 * Math.abs(best));
        return (
          <p className={`vtext ${ok ? "best" : "wrong"}`}>
            {ok ? "Good pick." : `Costs ${m(loss, 2)} vs the best play.`} Solver plays it {Math.round(fb.actions[guess].freq_hand * 100)}% with this hand. Press Next to see what you did.
          </p>
        );
      })()}
    </div>
  );
}

function Graded({ fb, m, t }: { fb: Feedback; m: (x: number, d?: number) => string; t: (s: string) => string }) {
  return (
    <div>
      <p>
        You: <b>{t(fb.actions[fb.chosen]?.label ?? "")}</b> · <span className={`vtext ${VERDICT_CLASS[fb.verdict] ?? ""}`}>{fb.verdict}</span>
        {fb.score.ev_loss_bb > 0.005 && <span className="muted"> · cost {m(fb.score.ev_loss_bb, 2)}</span>}
      </p>
      <table className="cats inner">
        <thead><tr><th>Action</th><th className="num">Solver, this hand</th><th className="num">EV</th></tr></thead>
        <tbody>
          {fb.actions.map((a, i) => (
            <tr key={i} className={i === fb.chosen ? "sel" : ""}>
              <td>{t(a.label)}</td>
              <td className="num">{Math.round(a.freq_hand * 100)}%</td>
              <td className="num">{m(a.ev_bb, 2)}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <LessonView fb={fb} t={t} />
    </div>
  );
}

function Modal({ children, onClose }: { children: React.ReactNode; onClose: () => void }) {
  return (
    <div className="modal-bg" onClick={onClose}>
      <div className="modal replayer" onClick={(e) => e.stopPropagation()}>
        <button className="close" onClick={onClose} aria-label="Close">×</button>
        {children}
      </div>
    </div>
  );
}

function describe(a: ReplayAct, bb: number, m: (x: number, d?: number) => string) {
  const amt = m(a.amount / bb);
  if (a.kind === "Fold") return "Fold";
  if (a.kind === "Check") return "Check";
  if (a.all_in) return `All-in ${amt}`;
  if (a.kind === "Call") return `Call ${amt}`;
  if (a.kind === "Bet") return `Bet ${amt}`;
  return `Raise to ${amt}`;
}

function resultLine(rep: Replay, m: (x: number, d?: number) => string) {
  const bb = rep.hand.bb_size || 1;
  const name = (p: string) => (p === rep.hero ? "You" : rep.names[p] || p);
  const w = rep.hand.collected.map(([p, x]) => `${name(p)} won ${m(x / bb)}`).join(", ");
  const s = rep.hand.shows.filter(([p]) => p !== rep.hero).map(([p, c]) => `${name(p)} showed ${prettyCards(c.join(" "))}`).join("; ");
  return [w || "No winner recorded.", s].filter(Boolean).join(" · ");
}

function buildFrames(rep: Replay): Frame[] {
  const h = rep.hand;
  const bb = h.bb_size || 1;
  const posOf = new Map(rep.positions);
  // seat order around the table, starting after the dealer
  const seats: SeatState[] = h.seats.map(([, name, stack]) => ({
    name, label: name === rep.hero ? "You" : rep.names[name] || name, pos: posOf.get(name) ?? "", stack, put: 0, folded: false, action: "", kind: "", hero: name === rep.hero,
  }));
  const by = new Map(seats.map((s) => [s.name, s]));
  let pot = 0, street = 0;
  const snap = (act: ReplayAct | null, end = false): Frame => ({ street, pot: pot + seats.reduce((x, s) => x + s.put, 0), seats: seats.map((s) => ({ ...s })), act, end });
  const frames: Frame[] = [];
  const apply = (a: ReplayAct) => {
    const s = by.get(a.player);
    if (!s) return;
    if (a.street !== 9 && a.street !== street) {
      pot += seats.reduce((x, q) => x + q.put, 0);
      seats.forEach((q) => { q.put = 0; q.action = ""; q.kind = ""; });
      street = a.street;
    }
    if (a.kind === "Fold") { s.folded = true; s.action = "Fold"; s.kind = "fold"; return; }
    if (a.kind === "Check") { s.action = "Check"; s.kind = "check"; return; }
    const delta = Math.max(0, a.amount - s.put);
    s.stack -= delta; s.put = a.amount;
    const amt = `${(a.amount / bb).toFixed(1)}bb`;
    if (a.street === 9) { s.action = ""; return; }
    s.kind = a.all_in ? "allin" : a.kind === "Call" ? "call" : a.kind === "Bet" ? "bet" : "raise";
    s.action = a.all_in ? `All-in ${amt}` : a.kind === "Call" ? `Call ${amt}` : a.kind === "Bet" ? `Bet ${amt}` : `Raise ${amt}`;
  };
  h.acts.filter((a) => a.street === 9).forEach(apply);
  frames.push(snap(null));
  for (const a of h.acts.filter((x) => x.street !== 9)) {
    apply(a);
    frames.push(snap(a));
  }
  // result: uncalled bets back, chips collected
  pot += seats.reduce((x, s) => x + s.put, 0);
  seats.forEach((s) => { s.put = 0; });
  h.returned.forEach(([p, x]) => { const s = by.get(p); if (s) { s.stack += x; pot -= x; } });
  h.collected.forEach(([p, x]) => { const s = by.get(p); if (s) s.stack += x; });
  frames.push({ ...snap(null, true), pot, street: Math.max(street, [0, 0, 0, 1, 2, 3][h.board.length] ?? 0) });
  return frames;
}
