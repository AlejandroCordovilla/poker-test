import { useState } from "react";
import { Feedback } from "./api";

/** Short by default: the answer, why (with your numbers), the rule. "More" opens the rest. */
export function LessonView({ fb, t }: { fb: Feedback; t: (s: string) => string }) {
  const [more, setMore] = useState(false);
  const l = fb.lesson;
  if (!l) return <p className="reason">{t(fb.reason)}</p>;
  return (
    <div className="lesson">
      <p className="l-verdict">{t(l.verdict)} <span className="l-why">{t(l.why)}</span></p>
      <p className="l-rule"><b>Rule:</b> {l.rule}</p>
      {more ? (
        <>
          <p><b>{l.concept}.</b> {t(l.idea)}</p>
          {l.pain && <p className="l-pain">{t(l.pain)}</p>}
          <p className="l-analogy">{l.analogy}</p>
          <button className="link" onClick={() => setMore(false)}>Less</button>
        </>
      ) : (
        <button className="link" onClick={() => setMore(true)}>More: {l.concept.toLowerCase()} explained</button>
      )}
    </div>
  );
}
