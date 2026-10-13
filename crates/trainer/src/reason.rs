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

/// A Karpathy-style lesson for one decision: say the thing, show the pain, one core idea,
/// one analogy, prove it with the numbers, then a rule to rebuild it next time.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Lesson {
    /// the verdict in one line
    pub verdict: String,
    /// what the wrong move costs (empty when the player was right)
    pub pain: String,
    /// name of the core idea
    pub concept: String,
    /// the core idea in one or two sentences
    pub idea: String,
    pub analogy: String,
    /// the numbers that prove it
    pub proof: String,
    /// a one-line rule to apply next time (the "rebuild it" step)
    pub rule: String,
    /// the reason in a few words, with the key numbers (shown by default)
    #[serde(default)]
    pub why: String,
}

fn bb(x: f64) -> String {
    format!("{x:.1}bb")
}

pub fn lesson(v: &DrillView, acts: &[ActionResult], s: &Score, chosen: usize, hand: &HandDesc, equity: f64, spot: &SpotInfo) -> Lesson {
    let target = &acts[s.target];
    let tk = kind(&target.label);
    let ck = kind(&acts[chosen].label);
    let hand_txt = match &hand.draw {
        Some(d) if hand.bucket <= Bucket::Draw => format!("{} with a {d}", hand.made),
        Some(d) => format!("{} plus a {d}", hand.made),
        None => hand.made.clone(),
    };
    let aggressor = v.hero == spot.preflop_aggressor;
    let facing = v.to_call_bb > 0.0;
    let need = if facing { v.to_call_bb / (v.pot_bb + v.to_call_bb) } else { 0.0 };
    let size_frac = |label: &str| -> Option<f64> {
        label.split('(').nth(1).and_then(|x| x.trim_end_matches(')').trim_end_matches(" pot").trim_end_matches('%').parse::<f64>().ok()).map(|x| x / 100.0)
    };
    let target_big = matches!(tk, "allin") || size_frac(&target.label).is_some_and(|f| f >= 0.74);
    let loss = s.ev_loss_bb;
    let right = chosen == s.target || loss <= 0.01;

    // 1) say the thing
    let short = |l: &str| l.split(" (").next().unwrap_or(l).to_string();
    let verdict = if right {
        format!("{} is right.", short(&target.label))
    } else if s.mixed && !s.rare {
        format!("Close. {} is a bit better than {}.", short(&target.label), short(&acts[chosen].label))
    } else {
        format!("{}, not {}.", short(&target.label), short(&acts[chosen].label))
    };

    // 2) core idea + analogy + rule, by the type of decision the solver chose
    let (concept, idea, analogy, rule) = match (tk, facing) {
        ("fold", true) | ("call", true) => (
            "Pot odds".to_string(),
            format!(
                "When you face a bet, the only question is price vs. equity. You pay {} to win a pot of {}, so you need to win about {} of the time.",
                bb(v.to_call_bb), bb(v.pot_bb + v.to_call_bb), pct(need)
            ),
            "It's a bet at the betting window: if the payout is 3-to-1, your horse has to win more than 1 race in 4 or you're donating.".to_string(),
            "Facing a bet: continue when your equity beats the price.".to_string(),
        ),
        ("raise" | "allin", true) => (
            if hand.bucket >= Bucket::Strong { "Raise for value" } else { "Fold equity" }.to_string(),
            if hand.bucket >= Bucket::Strong {
                "You're ahead of most of what bets here, so you want more chips in now, before a scary card freezes the action.".to_string()
            } else {
                "A raise wins two ways: they fold right now, or you hit and win a bigger pot. A hand with outs makes the best raise because it isn't dead when called.".to_string()
            },
            if hand.bucket >= Bucket::Strong { "If someone's already buying, you raise the price while they still want it.".to_string() } else { "It's a fake-out with a backup plan: if the bluff gets called, you still have outs.".to_string() },
            "Raise your best hands and best draws; call the middle.".to_string(),
        ),
        ("check", false) => match hand.bucket {
            Bucket::Nuts | Bucket::Strong => (
                "Protect your checks".to_string(),
                "If you only check weak hands, an aggressive player bets every time you check. Keeping some strong hands in your checks makes that auto-bet lose money.".to_string(),
                "A shop that sometimes leaves the door unlocked with a guard dog inside: burglars stop trying every door.".to_string(),
                "Check some strong hands too.".to_string(),
            ),
            Bucket::Medium | Bucket::Weak => (
                "Pot control".to_string(),
                format!("{hand_txt} beats bluffs but loses to value. Betting mostly gets called by better and folds out worse, so you're betting into the wrong half of their range."),
                "Don't start a fight you can only win against people who'd have walked away anyway.".to_string(),
                "Medium hands: check and decide on their bet.".to_string(),
            ),
            _ => (
                "Pick good bluffs".to_string(),
                format!("With {} equity and no draw, betting needs folds right now and has no backup plan. Better bluffs are hands that can still improve.", pct(equity)),
                "A bluff with outs is a fake-out with a parachute. Pure air is jumping without one.".to_string(),
                "Bluff with draws, not pure air.".to_string(),
            ),
        },
        _ => {
            // betting
            if target_big {
                (
                    "Polarization".to_string(),
                    "Big bets are for the two ends of your range: very strong hands that want to get paid, and draws or bluffs that want folds. Medium hands bet small or check.".to_string(),
                    "A big bet is shouting. You only shout when you either have the goods or you really need them to back off.".to_string(),
                    "Big bets: nuts and draws. Middle hands: small or check.".to_string(),
                )
            } else if aggressor && v.street == "flop" {
                (
                    "Range advantage".to_string(),
                    format!("As the preflop raiser on a {} board, you have more strong hands than they do. A small bet makes money with almost your whole range because they have to fold a lot of hands.", v.texture),
                    "Like a landlord collecting small rent from every tenant: a little from many beats a lot from a few.".to_string(),
                    "Raiser on a good board: bet small, often.".to_string(),
                )
            } else if hand.bucket >= Bucket::Medium {
                (
                    "Value".to_string(),
                    format!("{hand_txt} is ahead of the hands that will call. A bet gets paid by worse; checking lets them see a free card."),
                    "You're selling something: price it so the buyers who are worse off still buy.".to_string(),
                    "Bet when worse hands call.".to_string(),
                )
            } else {
                (
                    "Fold equity".to_string(),
                    format!("{hand_txt} rarely wins at showdown, so the bet's job is to win the pot now, and your draw is the backup when they call."),
                    "A fake-out with a backup plan.".to_string(),
                    "Semi-bluff with draws.".to_string(),
                )
            }
        }
    };

    // 3) the pain: what the chosen move costs
    let pain = if right {
        String::new()
    } else {
        let what = match (ck, facing) {
            ("call", true) => format!("Calling pays {} to win {} with about {} equity when you need {}.", bb(v.to_call_bb), bb(v.pot_bb + v.to_call_bb), pct(equity), pct(need)),
            ("fold", true) => format!("Folding throws away about {} equity when the price only needs {}.", pct(equity), pct(need)),
            ("check", false) => "Checking gives up value (or a profitable bluff) the solver takes here.".to_string(),
            _ => format!("{} puts chips in where the solver doesn't want them.", acts[chosen].label),
        };
        format!("{what} About {} each time.", bb(loss))
    };

    // 4) proof: the numbers
    let mut proof = acts.iter().map(|a| format!("{} {} (solver {} with this hand)", a.label, bb(a.ev_bb), pct(a.freq_hand))).collect::<Vec<_>>().join(" · ");
    if facing {
        proof = format!("Need {} equity, have about {}. {proof}", pct(need), pct(equity));
    } else {
        proof = format!("Your equity vs their range: about {}. {proof}", pct(equity));
    }
    if s.mixed {
        proof.push_str(". The solver mixes here, so being on the right side of the mix matters less than mixing at all.");
    }
    let why = match concept.as_str() {
        "Pot odds" => format!("You need {} equity and have about {}.", pct(need), pct(equity)),
        "Raise for value" => "You're ahead of most hands that bet here.".to_string(),
        "Protect your checks" => "Checking some strong hands stops them from betting every time you check.".to_string(),
        "Pot control" => "Betting mostly gets called by better hands.".to_string(),
        "Pick good bluffs" => format!("{} equity and no draw: no backup plan if called.", pct(equity)),
        "Polarization" => "Big bets are for very strong hands and draws.".to_string(),
        "Range advantage" => "Your range is stronger on this board, so small bets work with most hands.".to_string(),
        "Value" => "Worse hands will call.".to_string(),
        _ => "It wins now, or later with your outs.".to_string(),
    };
    Lesson { verdict, pain, concept, idea, analogy, proof, rule, why }
}
