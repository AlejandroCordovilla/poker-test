import { DrillView, HandResult } from "./api";
import { Card } from "./Card";

const SEAT_POS: Record<string, string> = { bottom: "seat-bottom", top: "seat-top", left: "seat-left" };

/** PokerNow-style table: every seat, dealer button, stacks, who's in, and each player's action. */
export function TableView({ drill, result, m, t }: { drill: DrillView; result: HandResult | null; m: (x: number, d?: number) => string; t: (s: string) => string }) {
  const handOver = !!result;
  const inHand = drill.seats.filter((s) => s.in_hand).length;
  const place = (who: string, inHandSeat: boolean) => (who === "You" ? "bottom" : inHandSeat ? "top" : "left");
  return (
    <div className="table">
      <div className="table-meta">{drill.seats.length} players · {inHand} in the hand</div>
      {drill.seats.map((s) => (
        <div key={s.pos} className={`pseat ${SEAT_POS[place(s.who, s.in_hand)]} ${s.in_hand ? "" : "folded"} ${s.who === "You" ? "me" : ""}`}>
          <div className="pseat-top">
            <span className="pos">{s.pos}</span>
            {s.dealer && <span className="dealer">D</span>}
            <span className="who">{s.who || "Folded"}</span>
          </div>
          <div className="stack">{s.in_hand ? m(s.stack_bb) : <span className="muted">out</span>}</div>
          <div className="pre muted">Preflop: {t(s.preflop)}</div>
          {s.who === "Villain" && result?.villain_hand && <div className="vh">{result.villain_hand.map((c) => <Card key={c} card={c} />)}</div>}
          {s.action && <div className={`bubble ${s.action_kind}`}>{t(s.action)}</div>}
        </div>
      ))}
      <div className="felt-center">
        <div className="pot">Pot {m(handOver && result ? result.pot_bb : drill.pot_bb)}</div>
        <div className="board">{(handOver && result ? result.board : drill.board).map((c) => <Card key={c} card={c} size="lg" />)}</div>
        <div className="line">{drill.line.length ? t(drill.line.join(" → ")) : `${drill.street === "flop" ? "Flop" : "Turn"}: first action`}</div>
        <div className="hand">{drill.hand.map((c) => <Card key={c} card={c} size="lg" />)}</div>
      </div>
    </div>
  );
}
