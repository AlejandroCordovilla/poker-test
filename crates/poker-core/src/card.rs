//! Card encoding shared with postflop-solver: `card = 4 * rank + suit`,
//! rank 0 = deuce .. 12 = ace, suit 0 = clubs, 1 = diamonds, 2 = hearts, 3 = spades.

pub type Card = u8;

pub const RANK_CHARS: &[u8; 13] = b"23456789TJQKA";
pub const SUIT_CHARS: &[u8; 4] = b"cdhs";

#[inline]
pub fn rank(c: Card) -> u8 {
    c >> 2
}

#[inline]
pub fn suit(c: Card) -> u8 {
    c & 3
}

/// Parses "Ah", "Td", "10d", "A♠".
pub fn card_from_str(s: &str) -> Result<Card, String> {
    let s = s.trim();
    let (r, rest) = if let Some(rest) = s.strip_prefix("10") {
        (b'T', rest)
    } else {
        let mut it = s.chars();
        let r = it.next().ok_or("empty card")?.to_ascii_uppercase() as u8;
        (r, it.as_str())
    };
    let rank = RANK_CHARS.iter().position(|&x| x == r).ok_or(format!("bad rank in {s:?}"))? as u8;
    let suit = match rest {
        "c" | "C" | "♣" => 0,
        "d" | "D" | "♦" => 1,
        "h" | "H" | "♥" => 2,
        "s" | "S" | "♠" => 3,
        _ => return Err(format!("bad suit in {s:?}")),
    };
    Ok(4 * rank + suit)
}

pub fn card_to_string(c: Card) -> String {
    format!("{}{}", RANK_CHARS[rank(c) as usize] as char, SUIT_CHARS[suit(c) as usize] as char)
}

/// Parses a run of cards such as "Ks7d2c".
pub fn cards_from_str(s: &str) -> Result<Vec<Card>, String> {
    let s: String = s.chars().filter(|c| !c.is_whitespace() && *c != ',').collect();
    let chars: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let len = if chars[i] == '1' { 3 } else { 2 };
        let tok: String = chars[i..(i + len).min(chars.len())].iter().collect();
        out.push(card_from_str(&tok)?);
        i += len;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        for c in 0..52u8 {
            assert_eq!(card_from_str(&card_to_string(c)).unwrap(), c);
        }
        assert_eq!(card_from_str("10♦").unwrap(), card_from_str("Td").unwrap());
        assert_eq!(card_from_str("2c").unwrap(), 0);
        assert_eq!(card_from_str("As").unwrap(), 51);
        assert_eq!(cards_from_str("Ks7d2c").unwrap().len(), 3);
    }
}
