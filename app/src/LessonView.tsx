import { Feedback } from "./api";

/** Karpathy-style explanation: say the thing, the pain, one idea, one analogy, the numbers, a rule. */
export function LessonView({ fb, t }: { fb: Feedback; t: (s: string) => string }) {
  const l = fb.lesson;
  if (!l) return <p className="reason">{t(fb.reason)}</p>;
  return (
    <div className="lesson">
      <p className="l-verdict">{t(l.verdict)}</p>
      {l.pain && <p className="l-pain"><b>What it cost:</b> {t(l.pain)}</p>}
      <p><b>The idea: {l.concept}.</b> {t(l.idea)}</p>
      <p className="l-analogy"><b>Think of it like this:</b> {l.analogy}</p>
      <p className="muted"><b>The numbers:</b> {t(l.proof)}</p>
      <p className="l-rule"><b>Rule for next time:</b> {l.rule}</p>
    </div>
  );
}
