// Display amounts in big blinds or dollars. The engine always works in bb.
export type Units = { mode: "bb" | "$"; bb: number };

const KEY = "units";

export function loadUnits(): Units {
  try {
    const u = JSON.parse(localStorage.getItem(KEY) || "");
    if ((u.mode === "bb" || u.mode === "$") && u.bb > 0) return u;
  } catch {}
  return { mode: "bb", bb: 0.4 };
}

export function saveUnits(u: Units) {
  try { localStorage.setItem(KEY, JSON.stringify(u)); } catch {}
}

/** One amount given in bb. */
export function money(x: number, u: Units, digits = 1): string {
  if (u.mode === "bb") return `${x.toFixed(digits)}bb`;
  const d = x * u.bb;
  const sign = d < 0 ? "-" : "";
  return `${sign}$${Math.abs(d).toFixed(2)}`;
}

/** Rewrites every "12.5bb" inside engine text (action labels, lines, reasons). */
export function convertText(s: string, u: Units): string {
  const SUITS: Record<string, string> = { s: "♠", h: "♥", d: "♦", c: "♣" };
  s = s.replace(/\b(Turn|River) ([2-9TJQKA])([shdc])\b/g, (_, w, r, su) => `${w} ${r === "T" ? "10" : r}${SUITS[su]}`);
  if (u.mode === "bb") return s;
  return s.replace(/(-?\d+(?:\.\d+)?)bb\b/g, (_, n) => money(parseFloat(n), u));
}

/** "AcQd" -> "A♣Q♦" (leaves already-pretty text alone). */
export function prettyCards(s: string): string {
  const SUITS: Record<string, string> = { s: "♠", h: "♥", d: "♦", c: "♣" };
  return s.replace(/([2-9TJQKA])([shdc])/g, (_, r, su) => `${r === "T" ? "10" : r}${SUITS[su]}`);
}
