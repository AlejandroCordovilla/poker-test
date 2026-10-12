//! Grading rules.
//!
//! Five classes (green: Best, Correct; yellow: Inaccuracy; red: Wrong, Blunder):
//! - Best: the highest-EV action (or within 0.01bb of it).
//! - Correct: an action the solver plays with this hand (5%+) that loses no more than solver
//!   noise (1% of the pot; solves stop at 0.5% of the pot).
//! - Inaccuracy: a played action losing a little (up to 4% of the pot), or a rare action
//!   (solver under 5%) that doesn't lose much. Anything the solver plays under 5% is at
//!   least an Inaccuracy.
//! - Wrong: loses up to 12% of the pot, or an action the solver never plays (under 0.5%).
//! - Blunder: loses more than 12% of the pot (at least 1bb).
//!
//! RNG mode: a 1-100 roll picks which action of the solver's mix is "the" answer for this
//! hand. Taking another action from the mix is an Inaccuracy (wrong side of the mix).

use serde::{Deserialize, Serialize};

/// Below this solver frequency an action counts as a mistake regardless of EV.
pub const RARE_ACTION: f64 = 0.05;
/// Below this the solver "never" plays the action.
pub const NEVER: f64 = 0.005;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Verdict {
    Best,
    Correct,
    Inaccuracy,
    Wrong,
    Blunder,
}

impl Verdict {
    pub fn label(self) -> &'static str {
        match self {
            Verdict::Best => "Best",
            Verdict::Correct => "Correct",
            Verdict::Inaccuracy => "Inaccuracy",
            Verdict::Wrong => "Wrong",
            Verdict::Blunder => "Blunder",
        }
    }
    pub fn is_good(self) -> bool {
        self <= Verdict::Correct
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Score {
    /// highest-EV action
    pub best: usize,
    /// the action graded as "the answer": `best`, or the RNG pick in RNG mode
    pub target: usize,
    pub ev_loss_bb: f64,
    /// EV loss as % of the pot at this decision
    pub ev_loss_pct: f64,
    /// frequency of the solver's most common action minus the frequency of the chosen one
    pub freq_diff: f64,
    /// the solver plays the chosen action under 5% of the time with this hand
    pub rare: bool,
    /// the solver splits this hand between two or more actions (each at 5%+)
    pub mixed: bool,
    pub rng: Option<u32>,
    pub verdict: Verdict,
}

pub struct Thresholds {
    pub noise: f64,
    pub small: f64,
    pub big: f64,
}

pub fn thresholds(pot_bb: f64) -> Thresholds {
    Thresholds { noise: (0.01 * pot_bb).max(0.05), small: (0.04 * pot_bb).max(0.2), big: (0.12 * pot_bb).max(1.0) }
}

/// Which action a 1-100 roll selects from a mix: cumulative frequencies in action order.
pub fn rng_pick(freq: &[f64], roll: u32) -> usize {
    let x = (roll.clamp(1, 100) as f64 - 0.5) / 100.0;
    let total: f64 = freq.iter().sum();
    let mut acc = 0.0;
    for (a, f) in freq.iter().enumerate() {
        acc += f / total.max(1e-12);
        if x < acc {
            return a;
        }
    }
    freq.len() - 1
}

/// `ev_bb[a]`: EV of action `a` for this hand, in bb. `freq[a]`: solver frequency for this hand.
pub fn score(ev_bb: &[f64], freq: &[f64], chosen: usize, pot_bb: f64, rng: Option<u32>) -> Score {
    assert_eq!(ev_bb.len(), freq.len());
    assert!(chosen < ev_bb.len());
    let best = (0..ev_bb.len()).max_by(|&a, &b| ev_bb[a].partial_cmp(&ev_bb[b]).unwrap()).unwrap();
    let loss = (ev_bb[best] - ev_bb[chosen]).max(0.0);
    let t = thresholds(pot_bb);
    let f = freq[chosen];
    let rare = f < RARE_ACTION;
    let mixed = freq.iter().filter(|&&x| x >= RARE_ACTION).count() >= 2;
    let fmax = freq.iter().cloned().fold(0.0, f64::max);
    let target = match rng {
        Some(r) => rng_pick(freq, r),
        None => best,
    };
    let by_loss = |loss: f64| {
        if loss <= t.small {
            Verdict::Inaccuracy
        } else if loss <= t.big {
            Verdict::Wrong
        } else {
            Verdict::Blunder
        }
    };
    let mut verdict = if f < NEVER {
        if loss <= t.big { Verdict::Wrong } else { Verdict::Blunder }
    } else if rare {
        by_loss(loss)
    } else if chosen == best || loss <= 0.01 {
        Verdict::Best
    } else if loss <= t.noise {
        Verdict::Correct
    } else {
        by_loss(loss)
    };
    if rng.is_some() && !rare {
        verdict = if chosen == target { verdict.min(Verdict::Best) } else { verdict.max(Verdict::Inaccuracy) };
    }
    Score {
        best,
        target,
        ev_loss_bb: loss,
        ev_loss_pct: if pot_bb > 0.0 { 100.0 * loss / pot_bb } else { 0.0 },
        freq_diff: (fmax - f).max(0.0),
        rare,
        mixed,
        rng,
        verdict,
    }
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
    use Verdict::*;

    fn v(ev: &[f64], f: &[f64], c: usize, pot: f64) -> Verdict {
        score(ev, f, c, pot, None).verdict
    }

    #[test]
    fn best_action_is_best() {
        let s = score(&[1.0, 2.0, 0.5], &[0.1, 0.9, 0.0], 1, 5.0, None);
        assert_eq!(s.verdict, Best);
        assert_eq!(s.ev_loss_bb, 0.0);
        assert_eq!(s.freq_diff, 0.0);
    }

    #[test]
    fn mixed_strategy_both_fine() {
        let ev = [1.000, 1.004];
        let f = [0.4, 0.6];
        assert_eq!(v(&ev, &f, 0, 5.0), Best);
        assert_eq!(v(&ev, &f, 1, 5.0), Best);
        assert!(score(&ev, &f, 0, 5.0, None).mixed);
        // solver noise: 0.19bb gap in a 20bb pot between two played actions is Correct
        assert_eq!(v(&[30.34, 30.15], &[0.6, 0.4], 1, 20.5), Correct);
    }

    #[test]
    fn rare_action_is_at_least_inaccuracy() {
        let s = score(&[1.0, 0.98], &[0.97, 0.03], 1, 20.0, None);
        assert!(s.rare);
        assert_eq!(s.verdict, Inaccuracy);
        assert!((s.ev_loss_bb - 0.02).abs() < 1e-9);
        assert!((s.freq_diff - 0.94).abs() < 1e-9);
    }

    #[test]
    fn never_played_is_wrong_or_blunder() {
        assert_eq!(v(&[1.0, 0.99], &[1.0, 0.0], 1, 5.0), Wrong);
        assert_eq!(v(&[3.0, 0.0], &[1.0, 0.0], 1, 5.0), Blunder);
    }

    #[test]
    fn graded_by_ev_loss_relative_to_pot() {
        let f = [0.5, 0.5];
        assert_eq!(v(&[1.0, 0.97], &f, 1, 5.0), Correct); // 0.03 <= noise 0.05
        assert_eq!(v(&[1.0, 0.85], &f, 1, 5.0), Inaccuracy); // 0.15 <= small 0.2
        assert_eq!(v(&[1.0, 0.3], &f, 1, 5.0), Wrong); // 0.7 <= big 1.0
        assert_eq!(v(&[3.0, 0.0], &f, 1, 5.0), Blunder);
        // the same 0.3bb is noise in a 40bb pot (0.75%) but Wrong in a 5bb pot (6%)
        assert_eq!(v(&[10.0, 9.7], &f, 1, 40.0), Correct);
        assert_eq!(v(&[10.0, 9.7], &f, 1, 5.0), Wrong);
        let s = score(&[10.0, 9.0], &f, 1, 20.0, None);
        assert!((s.ev_loss_pct - 5.0).abs() < 1e-9);
    }

    #[test]
    fn rng_mode_grades_the_rolled_action() {
        let ev = [2.0, 2.0];
        let f = [0.3, 0.7];
        assert_eq!(rng_pick(&f, 10), 0);
        assert_eq!(rng_pick(&f, 30), 0);
        assert_eq!(rng_pick(&f, 31), 1);
        assert_eq!(rng_pick(&f, 100), 1);
        assert_eq!(score(&ev, &f, 0, 10.0, Some(20)).verdict, Best);
        assert_eq!(score(&ev, &f, 1, 10.0, Some(20)).verdict, Inaccuracy); // wrong side of the mix
        assert_eq!(score(&ev, &f, 1, 10.0, Some(80)).verdict, Best);
    }

    #[test]
    fn range_frequency_math() {
        let strat = [1.0, 0.0, 0.5, /* action 1 */ 0.0, 1.0, 0.5];
        let w = [1.0, 1.0, 2.0];
        let r = range_freqs(&strat, &w, 2);
        assert!((r[0] - 0.5).abs() < 1e-9 && (r[1] - 0.5).abs() < 1e-9);
        assert_eq!(range_freqs(&strat, &[1.0, 0.0, 0.0], 2), vec![1.0, 0.0]);
    }
}
