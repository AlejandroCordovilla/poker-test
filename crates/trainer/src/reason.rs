//! Short plain-English reason for the solver's choice, built from the hand, the board,
//! equity and the action frequencies (rule-based, offline, testable).

use crate::drill::{ActionResult, DrillView};
use crate::scoring::Score;
use crate::spots::SpotInfo;
use poker_core::describe::{Bucket, HandDesc};

fn pct(x: f64) -> String {
    format!("{:.0}%", x * 100.0)
}

fn kind(label: &str) -> &'static str {
    let l = label.to_lowercase();
    if l.starts_with("fold") { "fold" } else if l.starts_with("check") { "check" } else if l.starts_with("call") { "call" } else if l.starts_with("raise") { "raise" } else if l.starts_with("all-in") { "allin" } else { "bet" }
}

pub fn explain(v: &DrillView, acts: &[ActionResult], s: &Score, chosen: usize, hand: &HandDesc, equity: f64, spot: &SpotInfo) -> String {
    let best = &acts[s.best];
    let bk = kind(&best.label);
    let ck = kind(&acts[chosen].label);
    let mut out = Vec::new();
    let hand_txt = match &hand.draw {
        Some(d) if hand.bucket <= Bucket::Draw => format!("{} with a {d}", hand.made),
        Some(d) => format!("{} plus a {d}", hand.made),
        None => hand.made.clone(),
    };
    let aggressor = v.hero == spot.preflop_aggressor;

    // the main idea
    let main = match (bk, v.to_call_bb > 0.0) {
        ("fold", _) => {
            let need = v.to_call_bb / (v.pot_bb + v.to_call_bb);
            format!("Folding is best: you need about {} equity to call and have about {} with {hand_txt}.", pct(need), pct(equity))
        }
        ("call", _) => {
            let need = v.to_call_bb / (v.pot_bb + v.to_call_bb);
            if hand.bucket >= Bucket::Strong {
                format!("Calling is best: {hand_txt} is strong, but raising folds out the weaker hands you beat. Let them keep betting.")
            } else {
                format!("Calling is best: the price needs about {} equity and you have about {} with {hand_txt}.", pct(need), pct(equity))
            }
        }
        ("raise" | "allin", true) => {
            if hand.bucket >= Bucket::Strong {
                format!("Raise for value: {hand_txt} is ahead of most of their betting range ({} equity).", pct(equity))
            } else {
                format!("Raise as a semi-bluff: {hand_txt} can win by folding them out now or by improving later.")
            }
        }
        ("check", _) => match hand.bucket {
            Bucket::Nuts | Bucket::Strong => format!("Checking is best here even with {hand_txt}: it keeps their bluffs and weaker hands in, and protects your checking range."),
            Bucket::Medium | Bucket::Weak => format!("Check for pot control: {hand_txt} beats their bluffs but not their value hands, so betting mostly gets called by better."),
            _ => format!("Give up for now: {hand_txt} has little equity ({}) and no good reason to bluff here.", pct(equity)),
        },
        _ => {
            let size = best.label.split('(').nth(1).map(|x| x.trim_end_matches(')').to_string()).unwrap_or_default();
            match hand.bucket {
                Bucket::Nuts | Bucket::Strong => format!("Bet for value: {hand_txt} is ahead of most of their range ({} equity), so build the pot.", pct(equity)),
                Bucket::Medium => format!("Bet {size} for thin value and protection: {hand_txt} still beats a lot of their range."),
                Bucket::Draw => format!("Semi-bluff: {hand_txt} wins by folding them out now or by hitting later."),
                _ => {
                    if aggressor {
                        format!("Bluff ({size}): as the preflop raiser on a {} board you have the range advantage, so betting with {hand_txt} folds out many of their hands.", v.texture)
                    } else {
                        format!("Bluff ({size}): {hand_txt} has no showdown value, and this line is believable.")
                    }
                }
            }
        }
    };
    out.push(main);

    if let Some(r) = s.rng {
        if s.target != s.best || chosen != s.target {
            out.push(format!("RNG {r}: the roll picks {} from the solver's mix for this hand.", acts[s.target].label));
        }
    }
    // what the player chose
    if chosen != s.best {
        if ck == bk && ck == "bet" {
            out.push(format!("Right idea, different size: the solver prefers {}.", best.label));
        } else if s.rare {
            out.push(format!("The solver almost never plays {} with this hand ({}).", acts[chosen].label, pct(acts[chosen].freq_hand)));
        }
    }
    if s.mixed {
        let mix: Vec<String> = acts.iter().filter(|a| a.freq_hand >= 0.05).map(|a| format!("{} {}", a.label, pct(a.freq_hand))).collect();
        out.push(format!("The solver mixes with this hand ({}), so close options cost little.", mix.join(", ")));
    }
    // range context
    let r: Vec<String> = acts.iter().filter(|a| a.freq_range >= 0.05).map(|a| format!("{} {}", a.label.split(" (").next().unwrap_or(""), pct(a.freq_range))).collect();
    out.push(format!("Across all hands here the solver plays: {}.", r.join(", ")));
    out.join(" ")
}
