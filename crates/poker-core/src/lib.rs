//! Cards, hand evaluation, 169 starting-hand classes, preflop equity, flop isomorphism and clustering.

pub mod card;
pub mod describe;
pub mod equity;
pub mod eval;
pub mod flops;
pub mod hand169;
pub mod rng;

pub use card::{card_from_str, card_to_string, Card, RANK_CHARS, SUIT_CHARS};
