//! Preflop all-in equity between the 169 hand classes (Monte Carlo, deterministic seed),
//! plus the exact number of non-conflicting combo pairs for card removal.

use crate::eval::{evaluate, mask_of};
use crate::hand169::{combos, N};
use crate::rng::Rng;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct EquityTable {
    pub samples: u32,
    /// eq[i * N + j] = equity of class i against class j (ties count half)
    pub eq: Vec<f32>,
    /// w[i * N + j] = number of (combo of i, combo of j) pairs that share no card
    pub w: Vec<f32>,
}

impl EquityTable {
    pub fn eq(&self, i: usize, j: usize) -> f64 {
        self.eq[i * N + j] as f64
    }
    pub fn w(&self, i: usize, j: usize) -> f64 {
        self.w[i * N + j] as f64
    }

    pub fn compute(samples: u32, seed: u64) -> Self {
        let cs: Vec<Vec<(u8, u8)>> = (0..N).map(combos).collect();
        let rows: Vec<(Vec<f32>, Vec<f32>)> = (0..N)
            .into_par_iter()
            .map(|i| {
                let mut eq = vec![0f32; N];
                let mut w = vec![0f32; N];
                for j in 0..N {
                    let pairs: Vec<((u8, u8), (u8, u8))> = cs[i]
                        .iter()
                        .flat_map(|&a| cs[j].iter().map(move |&b| (a, b)))
                        .filter(|(a, b)| a.0 != b.0 && a.0 != b.1 && a.1 != b.0 && a.1 != b.1)
                        .collect();
                    w[j] = pairs.len() as f32;
                    if j < i || pairs.is_empty() {
                        continue; // lower triangle filled by symmetry below
                    }
                    let mut rng = Rng::new(seed ^ ((i * N + j) as u64).wrapping_mul(0x2545F4914F6CDD1D));
                    let mut score = 0f64;
                    for _ in 0..samples {
                        let (a, b) = pairs[rng.below(pairs.len() as u64) as usize];
                        let dead = mask_of(&[a.0, a.1, b.0, b.1]);
                        let mut board = 0u64;
                        while board.count_ones() < 5 {
                            let c = rng.below(52);
                            if dead >> c & 1 == 0 {
                                board |= 1 << c;
                            }
                        }
                        let va = evaluate(board | mask_of(&[a.0, a.1]));
                        let vb = evaluate(board | mask_of(&[b.0, b.1]));
                        score += if va > vb { 1.0 } else if va == vb { 0.5 } else { 0.0 };
                    }
                    eq[j] = (score / samples as f64) as f32;
                }
                (eq, w)
            })
            .collect();
        let mut eq = vec![0f32; N * N];
        let mut w = vec![0f32; N * N];
        for i in 0..N {
            for j in 0..N {
                w[i * N + j] = rows[i].1[j];
                if j >= i {
                    eq[i * N + j] = rows[i].0[j];
                    eq[j * N + i] = 1.0 - rows[i].0[j];
                }
            }
        }
        for i in 0..N {
            eq[i * N + i] = 0.5;
        }
        EquityTable { samples, eq, w }
    }

    pub fn load_or_compute(path: &std::path::Path, samples: u32) -> std::io::Result<Self> {
        if let Ok(f) = std::fs::File::open(path) {
            if let Ok(t) = serde_json::from_reader::<_, EquityTable>(std::io::BufReader::new(f)) {
                if t.samples >= samples {
                    return Ok(t);
                }
            }
        }
        let t = Self::compute(samples, 2026);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        serde_json::to_writer(std::io::BufWriter::new(std::fs::File::create(path)?), &t)?;
        Ok(t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hand169::index_of;

    #[test]
    fn known_matchups() {
        let t = EquityTable::compute(20_000, 1);
        let e = |a: &str, b: &str| t.eq(index_of(a).unwrap(), index_of(b).unwrap());
        assert!((e("AA", "KK") - 0.82).abs() < 0.015, "AA vs KK {}", e("AA", "KK"));
        assert!((e("AKs", "QQ") - 0.46).abs() < 0.015, "AKs vs QQ {}", e("AKs", "QQ"));
        assert!((e("22", "AKo") - 0.52).abs() < 0.015, "22 vs AKo {}", e("22", "AKo"));
        assert!((e("72o", "AA") - 0.12).abs() < 0.015);
        assert_eq!(t.w(index_of("AA").unwrap(), index_of("AA").unwrap()), 6.0); // AhAd vs AcAs etc.
        assert_eq!(t.w(index_of("AKs").unwrap(), index_of("72o").unwrap()), 48.0);
    }
}
