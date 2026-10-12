//! Grading rules. EV loss in big blinds decides the grade; an action the solver plays under
//! 5% of the time with this exact hand is always at least a mistake, even when its EV is close.

use serde::{Deserialize, Serialize};

/// Below this solver frequency an action counts as a mistake regardless of EV.
pub const RARE_ACTION: f64 = 0.05;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Verdict {
    Best,
    Fine,
    Inaccuracy,
    Mistake,
    Blunder,
}

impl Verdict {
    pub fn label(self) -> &'static str {
        match self {
            Verdict::Best => "Best",
            Verdict::Fine => "Fine",
            Verdict::Inaccuracy => "Inaccuracy",
            Verdict::Mistake => "Mistake",
            Verdict::Blunder => "Blunder",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Score {
    pub best: usize,
    pub ev_loss_bb: f64,
    /// the solver plays the chosen action under 5% of the time with this hand
    pub rare: bool,
    /// the solver splits this hand between two or more actions (each at 5%+)
    pub mixed: bool,
    pub verdict: Verdict,
}

fn by_loss(loss: f64) -> Verdict {
    if loss <= 0.05 {
        Verdict::Fine
    } else if loss <= 0.25 {
        Verdict::Inaccuracy
    } else if loss <= 1.5 {
        Verdict::Mistake
    } else {
        Verdict::Blunder
    }
}

/// `ev_bb[a]`: EV of action `a` for this hand, in bb. `freq[a]`: solver frequency for this hand.
pub fn score(ev_bb: &[f64], freq: &[f64], chosen: usize) -> Score {
    assert_eq!(ev_bb.len(), freq.len());
    assert!(chosen < ev_bb.len());
    let best = (0..ev_bb.len()).max_by(|&a, &b| ev_bb[a].partial_cmp(&ev_bb[b]).unwrap()).unwrap();
    let loss = (ev_bb[best] - ev_bb[chosen]).max(0.0);
    let rare = freq[chosen] < RARE_ACTION;
    let mixed = freq.iter().filter(|&&f| f >= RARE_ACTION).count() >= 2;
    let verdict = if rare {
        by_loss(loss).max(Verdict::Mistake)
    } else if chosen == best || loss <= 0.01 {
        Verdict::Best
    } else {
        by_loss(loss)
    };
    Score { best, ev_loss_bb: loss, rare, mixed, verdict }
}

/// Range-wide frequency of each action: sum over hands of weight x strategy, normalized.
pub fn range_freqs(strategy: &[f32], weights: &[f32], n_actions: usize) -> Vec<f64> {
    let h = weights.len();
    let mut out = vec![0.0; n_actions];
    let total: f64 = weights.iter().map(|&w| w as f64).sum();
    if total <= 0.0 {
        return out;
    }
    for (a, o) in out.iter_mut().enumerate() {
        *o = (0..h).map(|i| strategy[a * h + i] as f64 * weights[i] as f64).sum::<f64>() / total;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn best_action_is_best() {
        let s = score(&[1.0, 2.0, 0.5], &[0.1, 0.9, 0.0], 1);
        assert_eq!(s.verdict, Verdict::Best);
        assert_eq!(s.ev_loss_bb, 0.0);
    }

    #[test]
    fn mixed_strategy_both_fine() {
        // solver splits 40/60 with near-equal EV: either choice is fine
        let ev = [1.000, 1.004];
        let f = [0.4, 0.6];
        assert_eq!(score(&ev, &f, 0).verdict, Verdict::Best);
        assert_eq!(score(&ev, &f, 1).verdict, Verdict::Best);
        assert!(score(&ev, &f, 0).mixed);
    }

    #[test]
    fn rare_action_is_a_mistake_even_with_close_ev() {
        let s = score(&[1.0, 0.98], &[0.97, 0.03], 1);
        assert!(s.rare);
        assert_eq!(s.verdict, Verdict::Mistake);
        assert!((s.ev_loss_bb - 0.02).abs() < 1e-9);
    }

    #[test]
    fn graded_by_ev_loss() {
        let f = [0.5, 0.5];
        assert_eq!(score(&[1.0, 0.97], &f, 1).verdict, Verdict::Fine);
        assert_eq!(score(&[1.0, 0.8], &f, 1).verdict, Verdict::Inaccuracy);
        assert_eq!(score(&[1.0, 0.0], &f, 1).verdict, Verdict::Mistake);
        assert_eq!(score(&[3.0, 0.0], &f, 1).verdict, Verdict::Blunder);
        assert_eq!(score(&[3.0, 0.0], &[1.0, 0.0], 1).verdict, Verdict::Blunder); // rare + big loss
    }

    #[test]
    fn range_frequency_math() {
        // 2 actions, 3 hands; weights 1, 1, 2
        let strat = [1.0, 0.0, 0.5, /* action 1 */ 0.0, 1.0, 0.5];
        let w = [1.0, 1.0, 2.0];
        let r = range_freqs(&strat, &w, 2);
        assert!((r[0] - 0.5).abs() < 1e-9 && (r[1] - 0.5).abs() < 1e-9);
        let r = range_freqs(&strat, &[1.0, 0.0, 0.0], 2);
        assert_eq!(r, vec![1.0, 0.0]);
    }
}
