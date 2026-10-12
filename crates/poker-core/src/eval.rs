//! 7-card (or 5-6 card) hand evaluator over a 52-bit card mask. Higher value = better hand.

use crate::card::Card;

const CAT: u32 = 26;

#[inline]
pub fn mask_of(cards: &[Card]) -> u64 {
    cards.iter().fold(0u64, |m, &c| m | (1u64 << c))
}

/// Highest straight in a 13-bit rank mask, as the rank of its top card (3 = five-high wheel).
#[inline]
fn straight_high(m: u32) -> Option<u32> {
    let m14 = (m << 1) | ((m >> 12) & 1); // ace also plays low
    (4..=13).rev().find(|&hi| (m14 >> (hi - 4)) & 0x1f == 0x1f).map(|hi| hi - 1)
}

/// Keeps the `n` highest set bits of a 13-bit mask.
#[inline]
fn top_bits(mut m: u32, n: u32) -> u32 {
    while m.count_ones() > n {
        m &= m - 1;
    }
    m
}

pub fn evaluate(mask: u64) -> u32 {
    let mut suits = [0u32; 4];
    for (s, sm) in suits.iter_mut().enumerate() {
        let mut v = 0u32;
        for r in 0..13 {
            if mask >> (4 * r + s) & 1 == 1 {
                v |= 1 << r;
            }
        }
        *sm = v;
    }
    let all = suits[0] | suits[1] | suits[2] | suits[3];

    if let Some(fl) = suits.iter().find(|m| m.count_ones() >= 5) {
        if let Some(h) = straight_high(*fl) {
            return (8 << CAT) | (h << 22);
        }
        let flush = (5 << CAT) | top_bits(*fl, 5);
        // a flush beats everything below quads/full house; check those first
        let best = paired_value(&suits, all);
        return if best >> CAT >= 6 { best } else { flush };
    }
    let paired = paired_value(&suits, all);
    if paired >> CAT >= 4 {
        return paired;
    }
    if let Some(h) = straight_high(all) {
        return (4 << CAT) | (h << 22);
    }
    paired
}

/// Quads, full house, trips, two pair, pair or high card (no flush/straight logic).
fn paired_value(suits: &[u32; 4], all: u32) -> u32 {
    let (mut quads, mut trips, mut pairs) = (0u32, 0u32, 0u32);
    for r in 0..13 {
        let n = (0..4).filter(|&s| suits[s] >> r & 1 == 1).count();
        match n {
            4 => quads |= 1 << r,
            3 => trips |= 1 << r,
            2 => pairs |= 1 << r,
            _ => {}
        }
    }
    let hi = |m: u32| 31 - m.leading_zeros();
    if quads != 0 {
        let q = hi(quads);
        return (7 << CAT) | (q << 22) | top_bits(all & !(1 << q), 1);
    }
    if trips != 0 {
        let t = hi(trips);
        let rest = (trips & !(1 << t)) | pairs;
        if rest != 0 {
            return (6 << CAT) | (t << 22) | (hi(rest) << 18);
        }
        return (3 << CAT) | (t << 22) | top_bits(all & !(1 << t), 2);
    }
    if pairs.count_ones() >= 2 {
        let p1 = hi(pairs);
        let p2 = hi(pairs & !(1 << p1));
        return (2 << CAT) | (p1 << 22) | (p2 << 18) | top_bits(all & !(1 << p1) & !(1 << p2), 1);
    }
    if pairs != 0 {
        let p = hi(pairs);
        return (1 << CAT) | (p << 22) | top_bits(all & !(1 << p), 3);
    }
    top_bits(all, 5)
}

/// Category 0 = high card .. 8 = straight flush.
pub fn category(value: u32) -> u32 {
    value >> CAT
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::cards_from_str;

    fn v(s: &str) -> u32 {
        evaluate(mask_of(&cards_from_str(s).unwrap()))
    }

    #[test]
    fn categories() {
        assert_eq!(category(v("AhKhQhJhTh2c3d")), 8);
        assert_eq!(category(v("AhAdAcAs2c3d4h")), 7);
        assert_eq!(category(v("AhAdAcKsKc3d4h")), 6);
        assert_eq!(category(v("2h5h9hJhKh2c3d")), 5);
        assert_eq!(category(v("Ah2d3c4s5c9d9h")), 4);
        assert_eq!(category(v("7h7d7c2s5cJdQh")), 3);
        assert_eq!(category(v("7h7d2c2s5cJdQh")), 2);
        assert_eq!(category(v("7h7d3c2s5cJdQh")), 1);
        assert_eq!(category(v("Ah9d3c2s5cJdQh")), 0);
    }

    #[test]
    fn ordering() {
        assert!(v("AhAd2c7s9dJcKh") > v("KhKd2c7s9dJcAh")); // AA > KK
        assert!(v("AhKd2c7s9dJcJh") > v("AhQd2c7s9dJcJh")); // kicker
        assert!(v("6h2d3c4s5c9dKh") > v("Ah2d3c4s5c9dKh")); // 6-high straight > wheel
        assert!(v("AhAdAcKsKcQdQh") > v("AhAdAcQsQcJdJh")); // AAAKK > AAAQQ
        assert!(v("KhKdQcQs4c2d3h") > v("KhKdQcQs3c2d2h")); // two pair kicker
        assert!(v("AsKs2s7s9sTdTh") > v("AdAhTsTc9s7d2c")); // flush > two pair
        assert!(v("9h9d9c2s2cAhAd") > v("AsKsQsJs9s2d3h")); // full house > flush
        assert_eq!(v("AhKdQcJs9d3c2h"), v("AdKhQsJc9h3d2s")); // suits don't matter
    }

    #[test]
    fn category_frequencies_7card() {
        // Monte Carlo against known 7-card frequencies (high card 17.4%, pair 43.8%, two pair 23.5%,
        // trips 4.83%, straight 4.62%, flush 3.03%, full house 2.60%).
        let mut rng = crate::rng::Rng::new(7);
        let mut counts = [0u32; 9];
        let n = 400_000;
        for _ in 0..n {
            let mut m = 0u64;
            while m.count_ones() < 7 {
                m |= 1 << rng.below(52);
            }
            counts[category(evaluate(m)) as usize] += 1;
        }
        let f = |c: usize| counts[c] as f64 / n as f64;
        assert!((f(0) - 0.174).abs() < 0.004);
        assert!((f(1) - 0.438).abs() < 0.004);
        assert!((f(2) - 0.235).abs() < 0.004);
        assert!((f(3) - 0.0483).abs() < 0.002);
        assert!((f(4) - 0.0462).abs() < 0.002);
        assert!((f(5) - 0.0303).abs() < 0.002);
        assert!((f(6) - 0.0260).abs() < 0.002);
    }
}
