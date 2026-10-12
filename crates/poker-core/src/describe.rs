//! Plain-English hand description on a board ("top pair", "flush draw"), used in drill feedback.

use crate::card::{rank, suit, Card, RANK_CHARS};
use crate::eval::{category, evaluate, mask_of};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub enum Bucket {
    Air,
    Draw,
    Weak,
    Medium,
    Strong,
    Nuts,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HandDesc {
    pub made: String,
    pub draw: Option<String>,
    pub bucket: Bucket,
}

fn rank_name(r: u8) -> String {
    match r {
        12 => "ace".into(),
        11 => "king".into(),
        10 => "queen".into(),
        9 => "jack".into(),
        8 => "ten".into(),
        r => (RANK_CHARS[r as usize] as char).to_string(),
    }
}

fn straight_draw(all: &[Card]) -> Option<&'static str> {
    let mut m: u32 = 0;
    for &c in all {
        m |= 1 << rank(c);
    }
    let m14 = (m << 1) | ((m >> 12) & 1);
    let mut gut = false;
    for lo in 0..=9u32 {
        let w = (m14 >> lo) & 0x1f;
        if w.count_ones() == 4 {
            // open-ended: four in a row with both ends open (not ace-high or wheel-capped)
            let run = (m14 >> lo) & 0xf == 0xf || (m14 >> (lo + 1)) & 0xf == 0xf;
            if run && lo >= 1 && lo + 5 <= 13 {
                return Some("open-ended straight draw");
            }
            gut = true;
        }
    }
    gut.then_some("gutshot")
}

pub fn describe(hole: [Card; 2], board: &[Card]) -> HandDesc {
    let all: Vec<Card> = hole.iter().chain(board.iter()).copied().collect();
    let cat = category(evaluate(mask_of(&all)));
    let board_cat = if board.len() >= 5 { category(evaluate(mask_of(board))) } else { 0 };
    let mut br: Vec<u8> = board.iter().map(|&c| rank(c)).collect();
    br.sort_unstable_by(|a, b| b.cmp(a));
    br.dedup();
    let (h1, h2) = (rank(hole[0]).max(rank(hole[1])), rank(hole[0]).min(rank(hole[1])));
    let pair_hole = h1 == h2;

    // draws (only before the river)
    let mut draw = None;
    if board.len() < 5 && cat < 4 {
        for s in 0..4u8 {
            let n = all.iter().filter(|&&c| suit(c) == s).count();
            let mine = hole.iter().filter(|&&c| suit(c) == s).count();
            if n == 4 && mine >= 1 {
                draw = Some(if mine == 2 || hole.iter().any(|&c| suit(c) == s && rank(c) == 12) { "nut-ish flush draw".to_string() } else { "flush draw".to_string() });
            }
        }
        if let Some(sd) = straight_draw(&all) {
            draw = Some(match draw {
                Some(f) => format!("{f} + {sd}"),
                None => sd.to_string(),
            });
        }
    }

    let (made, bucket) = match cat {
        8 => ("straight flush".to_string(), Bucket::Nuts),
        7 => ("quads".to_string(), Bucket::Nuts),
        6 => ("a full house".to_string(), Bucket::Nuts),
        5 => ("a flush".to_string(), Bucket::Strong),
        4 => ("a straight".to_string(), Bucket::Strong),
        3 if pair_hole => (format!("a set of {}s", rank_name(h1)), Bucket::Strong),
        3 => ("trips".to_string(), Bucket::Strong),
        2 if board_cat >= 2 => ("two pair on the board".to_string(), Bucket::Weak),
        2 => {
            let both = br.contains(&h1) && br.contains(&h2);
            if both { ("two pair".to_string(), Bucket::Strong) } else { ("two pair (one pair on board)".to_string(), Bucket::Medium) }
        }
        1 => {
            let top = br[0];
            if pair_hole && h1 > top {
                (format!("an overpair ({}s)", rank_name(h1)), Bucket::Strong)
            } else if pair_hole {
                (format!("an underpair ({}s)", rank_name(h1)), Bucket::Weak)
            } else if br.contains(&h1) || br.contains(&h2) {
                let p = if br.contains(&h1) { h1 } else { h2 };
                let kicker = if p == h1 { h2 } else { h1 };
                if p == top {
                    let good = kicker >= 9;
                    (format!("top pair, {} kicker", if good { "good" } else { "weak" }), if good { Bucket::Strong } else { Bucket::Medium })
                } else if br.len() > 1 && p == br[1] {
                    ("second pair".to_string(), Bucket::Medium)
                } else {
                    ("a weak pair".to_string(), Bucket::Weak)
                }
            } else {
                ("only a board pair".to_string(), Bucket::Air)
            }
        }
        _ => (format!("{} high", rank_name(h1)), Bucket::Air),
    };
    let bucket = if bucket <= Bucket::Weak && draw.is_some() { Bucket::Draw } else { bucket };
    HandDesc { made, draw, bucket }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::cards_from_str;

    fn d(h: &str, b: &str) -> HandDesc {
        let h = cards_from_str(h).unwrap();
        describe([h[0], h[1]], &cards_from_str(b).unwrap())
    }

    #[test]
    fn descriptions() {
        assert_eq!(d("AhKd", "Ks7d2c").made, "top pair, good kicker");
        assert_eq!(d("QhQd", "Js7d2c").made, "an overpair (queens)");
        assert_eq!(d("7h7c", "Ks7d2c").made, "a set of 7s");
        assert_eq!(d("Ah5h", "Kh7h2c").draw.as_deref(), Some("nut-ish flush draw"));
        assert_eq!(d("9c8d", "Ts7h2c").draw.as_deref(), Some("open-ended straight draw"));
        assert_eq!(d("Ac4d", "Ts7h2c").bucket, Bucket::Air);
        assert_eq!(d("T♠7♣", "9♠Q♠J♠J♦3♠").made, "a flush");
    }
}
