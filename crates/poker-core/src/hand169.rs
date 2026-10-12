//! The 169 strategically distinct starting hands. Index = 13 * r1 + r2 over ranks 0..13:
//! r1 == r2 is a pair, r1 > r2 is suited (high card r1), r1 < r2 is offsuit (high card r2).

use crate::card::{rank, suit, Card, RANK_CHARS};

pub const N: usize = 169;

pub fn class_of(a: Card, b: Card) -> usize {
    let (ra, rb) = (rank(a) as usize, rank(b) as usize);
    let (hi, lo) = if ra >= rb { (ra, rb) } else { (rb, ra) };
    if hi == lo {
        13 * hi + hi
    } else if suit(a) == suit(b) {
        13 * hi + lo
    } else {
        13 * lo + hi
    }
}

pub fn name(i: usize) -> String {
    let (r1, r2) = (i / 13, i % 13);
    let c = |r: usize| RANK_CHARS[r] as char;
    match r1.cmp(&r2) {
        std::cmp::Ordering::Equal => format!("{}{}", c(r1), c(r1)),
        std::cmp::Ordering::Greater => format!("{}{}s", c(r1), c(r2)),
        std::cmp::Ordering::Less => format!("{}{}o", c(r2), c(r1)),
    }
}

pub fn is_pair(i: usize) -> bool {
    i / 13 == i % 13
}

pub fn is_suited(i: usize) -> bool {
    i / 13 > i % 13
}

/// (high rank, low rank)
pub fn ranks(i: usize) -> (usize, usize) {
    let (r1, r2) = (i / 13, i % 13);
    (r1.max(r2), r1.min(r2))
}

pub fn combos(i: usize) -> Vec<(Card, Card)> {
    let (hi, lo) = ranks(i);
    let mut out = Vec::new();
    for s1 in 0..4u8 {
        for s2 in 0..4u8 {
            let a = 4 * hi as u8 + s1;
            let b = 4 * lo as u8 + s2;
            let ok = if is_pair(i) { s1 < s2 } else if is_suited(i) { s1 == s2 } else { s1 != s2 };
            if ok {
                out.push((a, b));
            }
        }
    }
    out
}

pub fn combo_count(i: usize) -> f64 {
    if is_pair(i) {
        6.0
    } else if is_suited(i) {
        4.0
    } else {
        12.0
    }
}

pub fn index_of(name_: &str) -> Option<usize> {
    (0..N).find(|&i| name(i) == name_)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts() {
        let total: f64 = (0..N).map(combo_count).sum();
        assert_eq!(total, 1326.0);
        for i in 0..N {
            assert_eq!(combos(i).len() as f64, combo_count(i));
            for (a, b) in combos(i) {
                assert_eq!(class_of(a, b), i);
            }
        }
        assert_eq!(name(index_of("AKs").unwrap()), "AKs");
        assert_eq!(name(index_of("72o").unwrap()), "72o");
        assert_eq!(name(index_of("TT").unwrap()), "TT");
    }
}
