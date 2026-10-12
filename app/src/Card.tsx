const SUIT: Record<string, { sym: string; cls: string }> = {
  s: { sym: "♠", cls: "s" },
  h: { sym: "♥", cls: "h" },
  d: { sym: "♦", cls: "d" },
  c: { sym: "♣", cls: "c" },
};

export function Card({ card, size = "md" }: { card: string; size?: "md" | "lg" }) {
  const rank = card[0] === "T" ? "10" : card[0];
  const s = SUIT[card[1]];
  return (
    <div className={`card ${s.cls} ${size}`}>
      <span className="rank">{rank}</span>
      <span className="suit">{s.sym}</span>
    </div>
  );
}
