//! Flop isomorphism (1,755 strategically distinct flops) and texture clustering into a
//! small set of representative flops that the solver actually solves.

use crate::card::{card_to_string, rank, suit, Card};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub type Flop = [Card; 3];

const PERMS: [[u8; 4]; 24] = {
    let mut out = [[0u8; 4]; 24];
    let mut n = 0;
    let mut a = 0;
    while a < 4 {
        let mut b = 0;
        while b < 4 {
            let mut c = 0;
            while c < 4 {
                let d = 6 - a - b - c;
                if a != b && a != c && b != c && d < 4 && d != a && d != b && d != c {
                    out[n] = [a as u8, b as u8, c as u8, d as u8];
                    n += 1;
                }
                c += 1;
            }
            b += 1;
        }
        a += 1;
    }
    out
};

fn sorted_desc(mut f: Flop) -> Flop {
    f.sort_unstable_by(|a, b| b.cmp(a));
    f
}

/// Canonical form: the lexicographically largest sorted flop over all 24 suit relabelings.
pub fn canonical(f: Flop) -> Flop {
    PERMS
        .iter()
        .map(|p| sorted_desc(f.map(|c| 4 * rank(c) + p[suit(c) as usize])))
        .max()
        .unwrap()
}

/// All canonical flops with how many of the 22,100 raw flops map to each.
pub fn all_canonical() -> BTreeMap<Flop, u32> {
    let mut m = BTreeMap::new();
    for a in 0..52u8 {
        for b in 0..a {
            for c in 0..b {
                *m.entry(canonical([a, b, c])).or_insert(0) += 1;
            }
        }
    }
    m
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize, Deserialize)]
pub enum Stratum {
    Trips,
    PairedTwoTone,
    PairedRainbow,
    Monotone,
    TwoTone,
    Rainbow,
}

pub fn stratum(f: Flop) -> Stratum {
    let ranks: Vec<u8> = f.iter().map(|&c| rank(c)).collect();
    let mut suits: Vec<u8> = f.iter().map(|&c| suit(c)).collect();
    suits.sort_unstable();
    suits.dedup();
    let mut rs = ranks.clone();
    rs.sort_unstable();
    rs.dedup();
    match (rs.len(), suits.len()) {
        (1, _) => Stratum::Trips,
        (2, 2) => Stratum::PairedTwoTone,
        (2, _) => Stratum::PairedRainbow,
        (_, 1) => Stratum::Monotone,
        (_, 2) => Stratum::TwoTone,
        _ => Stratum::Rainbow,
    }
}

/// Rank features used for distances inside a stratum.
pub fn features(f: Flop) -> [f64; 5] {
    let mut r: Vec<i32> = f.iter().map(|&c| rank(c) as i32).collect();
    r.sort_unstable_by(|a, b| b.cmp(a));
    let mut d = r.clone();
    d.dedup();
    // connectedness: distinct ranks that fit in a 5-rank straight window (ace also low)
    let mut best = 0;
    for lo in -1..=8 {
        let n = d
            .iter()
            .filter(|&&x| (x >= lo && x <= lo + 4) || (x == 12 && lo == -1))
            .count();
        best = best.max(n);
    }
    let broadway = r.iter().filter(|&&x| x >= 8).count();
    [
        r[0] as f64 / 12.0 * 2.0, // high card matters most for range advantage
        r[1] as f64 / 12.0,
        r[2] as f64 / 12.0,
        if d.len() >= 3 { (best as f64 - 1.0) / 2.0 } else { 0.0 },
        broadway as f64 / 3.0,
    ]
}

fn dist(a: &[f64; 5], b: &[f64; 5]) -> f64 {
    a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum()
}

pub fn label(f: Flop) -> String {
    let s = stratum(f);
    let hi = f.iter().map(|&c| rank(c)).max().unwrap();
    let feats = features(f);
    let high = match hi {
        12 => "A-high",
        10 | 11 => "K/Q-high",
        7..=9 => "middle",
        _ => "low",
    };
    let texture = match s {
        Stratum::Trips => "trips",
        Stratum::PairedTwoTone | Stratum::PairedRainbow => "paired",
        Stratum::Monotone => "monotone",
        Stratum::TwoTone => "two-tone",
        Stratum::Rainbow => "rainbow",
    };
    let conn = if feats[3] >= 1.0 { " connected" } else if feats[3] <= 0.0 && s == Stratum::Rainbow { " dry" } else { "" };
    format!("{high} {texture}{conn}")
}

pub fn flop_string(f: Flop) -> String {
    f.iter().map(|&c| card_to_string(c)).collect()
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Cluster {
    pub rep: String,
    pub label: String,
    pub stratum: Stratum,
    /// share of all raw flops this representative stands for
    pub weight: f64,
    pub members: Vec<String>,
}

/// Weighted k-medoids inside each stratum; k is split across strata by flop frequency
/// (largest remainder, at least one per stratum).
pub fn cluster(k: usize) -> Vec<Cluster> {
    let all = all_canonical();
    let total: f64 = all.values().map(|&w| w as f64).sum();
    let mut by: BTreeMap<Stratum, Vec<(Flop, f64)>> = BTreeMap::new();
    for (&f, &w) in &all {
        by.entry(stratum(f)).or_default().push((f, w as f64));
    }
    // allocate k
    let shares: Vec<(Stratum, f64)> = by
        .iter()
        .map(|(s, v)| (*s, v.iter().map(|x| x.1).sum::<f64>() / total * k as f64))
        .collect();
    let mut alloc: BTreeMap<Stratum, usize> = shares.iter().map(|(s, x)| (*s, (x.floor() as usize).max(1))).collect();
    let mut rest: Vec<(Stratum, f64)> = shares.iter().map(|(s, x)| (*s, x - x.floor())).collect();
    rest.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    let mut i = 0;
    while alloc.values().sum::<usize>() < k {
        *alloc.get_mut(&rest[i % rest.len()].0).unwrap() += 1;
        i += 1;
    }

    let mut out = Vec::new();
    for (s, pts) in &by {
        let kk = alloc[s].min(pts.len());
        let feats: Vec<[f64; 5]> = pts.iter().map(|p| features(p.0)).collect();
        // deterministic farthest-point init, starting from the heaviest point
        let mut med: Vec<usize> = vec![(0..pts.len()).max_by(|&a, &b| pts[a].1.partial_cmp(&pts[b].1).unwrap().then(b.cmp(&a))).unwrap()];
        while med.len() < kk {
            let next = (0..pts.len())
                .max_by(|&a, &b| {
                    let da = med.iter().map(|&m| dist(&feats[a], &feats[m])).fold(f64::MAX, f64::min) * pts[a].1;
                    let db = med.iter().map(|&m| dist(&feats[b], &feats[m])).fold(f64::MAX, f64::min) * pts[b].1;
                    da.partial_cmp(&db).unwrap().then(b.cmp(&a))
                })
                .unwrap();
            med.push(next);
        }
        let mut assign = vec![0usize; pts.len()];
        for _ in 0..100 {
            for (p, a) in assign.iter_mut().enumerate() {
                *a = (0..med.len())
                    .min_by(|&x, &y| dist(&feats[p], &feats[med[x]]).partial_cmp(&dist(&feats[p], &feats[med[y]])).unwrap())
                    .unwrap();
            }
            let mut changed = false;
            for (ci, m) in med.iter_mut().enumerate() {
                let members: Vec<usize> = (0..pts.len()).filter(|&p| assign[p] == ci).collect();
                if members.is_empty() {
                    continue;
                }
                let best = *members
                    .iter()
                    .min_by(|&&x, &&y| {
                        let cx: f64 = members.iter().map(|&q| pts[q].1 * dist(&feats[x], &feats[q])).sum();
                        let cy: f64 = members.iter().map(|&q| pts[q].1 * dist(&feats[y], &feats[q])).sum();
                        cx.partial_cmp(&cy).unwrap().then(x.cmp(&y))
                    })
                    .unwrap();
                if best != *m {
                    *m = best;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        for (ci, &m) in med.iter().enumerate() {
            let members: Vec<usize> = (0..pts.len()).filter(|&p| assign[p] == ci).collect();
            out.push(Cluster {
                rep: flop_string(pts[m].0),
                label: label(pts[m].0),
                stratum: *s,
                weight: members.iter().map(|&p| pts[p].1).sum::<f64>() / total,
                members: members.iter().map(|&p| flop_string(pts[p].0)).collect(),
            });
        }
    }
    out.sort_by(|a, b| b.weight.partial_cmp(&a.weight).unwrap());
    out
}

/// Nearest representative for any real flop (same stratum, closest rank features).
pub fn nearest<'a>(clusters: &'a [Cluster], f: Flop) -> &'a Cluster {
    let c = canonical(f);
    let s = flop_string(c);
    if let Some(cl) = clusters.iter().find(|cl| cl.members.contains(&s)) {
        return cl;
    }
    let st = stratum(c);
    let fe = features(c);
    clusters
        .iter()
        .filter(|cl| cl.stratum == st)
        .min_by(|a, b| {
            let fa = features(crate::card::cards_from_str(&a.rep).unwrap().try_into().unwrap());
            let fb = features(crate::card::cards_from_str(&b.rep).unwrap().try_into().unwrap());
            dist(&fe, &fa).partial_cmp(&dist(&fe, &fb)).unwrap()
        })
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::cards_from_str;

    fn flop(s: &str) -> Flop {
        cards_from_str(s).unwrap().try_into().unwrap()
    }

    #[test]
    fn isomorphism_counts() {
        let all = all_canonical();
        assert_eq!(all.len(), 1755);
        assert_eq!(all.values().sum::<u32>(), 22100);
        assert_eq!(canonical(flop("Ks7d2c")), canonical(flop("Kh7s2d")));
        assert_ne!(canonical(flop("Ks7s2c")), canonical(flop("Ks7d2c")));
    }

    #[test]
    fn strata_frequencies() {
        let all = all_canonical();
        let share = |s: Stratum| all.iter().filter(|(f, _)| stratum(**f) == s).map(|(_, w)| *w).sum::<u32>() as f64 / 22100.0;
        assert!((share(Stratum::Monotone) - 0.0518).abs() < 0.001);
        assert!((share(Stratum::Trips) - 0.00235).abs() < 0.0005);
    }

    #[test]
    fn clustering_covers_everything() {
        let cl = cluster(75);
        assert_eq!(cl.len(), 75);
        assert!((cl.iter().map(|c| c.weight).sum::<f64>() - 1.0).abs() < 1e-9);
        assert_eq!(cl.iter().map(|c| c.members.len()).sum::<usize>(), 1755);
        assert!(cl.iter().any(|c| c.stratum == Stratum::Trips));
        let n = nearest(&cl, flop("Ah7d2c"));
        assert_eq!(n.stratum, Stratum::Rainbow);
        assert!(n.rep.starts_with('A'));
    }
}
