use crate::tables::{GENERATOR, MN, NM, NUM_ROWS};

pub const N: usize = 174;
pub const K: usize = 91;
pub const M: usize = 83;

fn rows() -> &'static [u128; M] {
    static ROWS: std::sync::OnceLock<[u128; M]> = std::sync::OnceLock::new();
    ROWS.get_or_init(|| {
        let mut r = [0u128; M];
        for (row, g) in r.iter_mut().zip(GENERATOR.iter()) {
            *row = g.iter().fold(0u128, |a, &b| a << 8 | b as u128) >> 5;
        }
        r
    })
}

fn edges() -> &'static [[(usize, usize); 7]; M] {
    static EDGES: std::sync::OnceLock<[[(usize, usize); 7]; M]> = std::sync::OnceLock::new();
    EDGES.get_or_init(|| {
        let mut e = [[(0, 0); 7]; M];
        for (m, row) in e.iter_mut().enumerate() {
            for (k, v) in row.iter_mut().take(NUM_ROWS[m] as usize).enumerate() {
                let n = NM[m][k] as usize - 1;
                let j = MN[n]
                    .iter()
                    .position(|&c| c as usize - 1 == m)
                    .expect("edge");
                *v = (n, j);
            }
        }
        e
    })
}

pub fn encode(a91: u128) -> [u8; N] {
    let mut cw = [0u8; N];
    for (i, c) in cw.iter_mut().take(K).enumerate() {
        *c = (a91 >> (K - 1 - i) & 1) as u8;
    }
    for (i, row) in rows().iter().enumerate() {
        cw[K + i] = ((row & a91).count_ones() & 1) as u8;
    }
    cw
}

pub fn bits_to_a91(cw: &[u8]) -> u128 {
    cw[..K].iter().fold(0u128, |a, &b| a << 1 | b as u128)
}

fn errors(cw: &[u8; N]) -> usize {
    (0..M)
        .filter(|&m| {
            NM[m][..NUM_ROWS[m] as usize]
                .iter()
                .fold(0, |x, &n| x ^ cw[n as usize - 1])
                != 0
        })
        .count()
}

const STALL: usize = 10;

pub fn decode(llr: &[f32; N], iterations: usize) -> Option<[u8; N]> {
    let mut tov = [[0f32; 3]; N];
    let mut cw = [0u8; N];
    let mut best = M;
    let mut since = 0;
    for _ in 0..iterations {
        for n in 0..N {
            cw[n] = (llr[n] + tov[n][0] + tov[n][1] + tov[n][2] > 0.0) as u8;
        }
        let e = errors(&cw);
        if e == 0 {
            return cw.contains(&1).then_some(cw);
        }
        if e < best {
            best = e;
            since = 0;
        } else {
            since += 1;
            if since > STALL {
                return None;
            }
        }
        let mut next = [[0f32; 3]; N];
        for (m, row) in edges().iter().enumerate() {
            let d = NUM_ROWS[m] as usize;
            let mut a = [0f32; 7];
            for (v, &(n, j)) in a.iter_mut().zip(&row[..d]) {
                let mut t = llr[n];
                for (i, x) in tov[n].iter().enumerate() {
                    if i != j {
                        t += x;
                    }
                }
                *v = (-t / 2.0).tanh();
            }
            let mut after = [1f32; 8];
            for k in (0..d).rev() {
                after[k] = after[k + 1] * a[k];
            }
            let mut before = 1f32;
            for (k, &(n, j)) in row[..d].iter().enumerate() {
                let p = before * after[k + 1];
                next[n][j] = -2.0 * p.clamp(-0.999_999, 0.999_999).atanh();
                before *= a[k];
            }
        }
        tov = next;
    }
    None
}

fn column(n: usize) -> u128 {
    if n < K {
        1 << (K - 1 - n)
    } else {
        rows()[n - K]
    }
}

pub fn osd(llr: &[f32; N], accept: impl Fn(u128) -> bool) -> Option<([u8; N], f32)> {
    let mut order: Vec<usize> = (0..N).collect();
    order.sort_by(|&a, &b| llr[b].abs().total_cmp(&llr[a].abs()));
    let mut basis: Vec<(u128, u128)> = Vec::with_capacity(K);
    let mut hard = 0u128;
    for &n in &order {
        let mut c = column(n);
        let mut mask = 1u128 << basis.len();
        for &(b, m) in &basis {
            if c & b.isolate_lowest_one() != 0 {
                c ^= b;
                mask ^= m;
            }
        }
        if c == 0 {
            continue;
        }
        let pivot = c.isolate_lowest_one();
        for e in basis.iter_mut() {
            if e.0 & pivot != 0 {
                e.0 ^= c;
                e.1 ^= mask;
            }
        }
        if llr[n] > 0.0 {
            hard |= 1 << basis.len();
        }
        basis.push((c, mask));
        if basis.len() == K {
            break;
        }
    }
    let solve = |h: u128| {
        basis
            .iter()
            .filter(|(_, m)| (m & h).count_ones() & 1 == 1)
            .fold(0u128, |a, (b, _)| a | b.isolate_lowest_one())
    };
    let cost = |a91: u128| {
        let cw = encode(a91);
        let d: f32 = (0..N)
            .filter(|&n| (cw[n] == 1) != (llr[n] > 0.0))
            .map(|n| llr[n].abs())
            .sum();
        (cw, d)
    };
    let mut best: Option<([u8; N], f32)> = None;
    for flip in std::iter::once(0).chain((0..K).map(|j| 1u128 << j)) {
        let a91 = solve(hard ^ flip);
        if !accept(a91) {
            continue;
        }
        let (cw, d) = cost(a91);
        if best.as_ref().is_none_or(|b| d < b.1) {
            best = Some((cw, d));
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crc::{crc_ok, with_crc};
    use crate::sim::Rng;

    #[test]
    fn every_encoded_word_meets_all_parity_checks() {
        let mut rng = Rng::new(3);
        for _ in 0..200 {
            let payload = (rng.next_u64() as u128) << 13 ^ rng.next_u64() as u128 & ((1 << 77) - 1);
            let cw = encode(with_crc(payload & ((1 << 77) - 1)));
            assert_eq!(errors(&cw), 0);
            assert!(crc_ok(bits_to_a91(&cw)));
        }
    }

    #[test]
    fn belief_propagation_repairs_flipped_bits() {
        let cw = encode(with_crc(0x1234_5678_9abc_def0_1234));
        let mut llr = [0f32; N];
        for (i, l) in llr.iter_mut().enumerate() {
            let sign = if cw[i] == 1 { 1.0 } else { -1.0 };
            *l = if i % 9 == 0 { -sign * 0.5 } else { sign * 2.0 };
        }
        assert_eq!(decode(&llr, 30), Some(cw));
    }

    fn flipped(cw: &[u8; N], flips: usize, rng: &mut Rng) -> [f32; N] {
        let mut llr = [0f32; N];
        for (i, l) in llr.iter_mut().enumerate() {
            *l = if cw[i] == 1 { 3.0 } else { -3.0 };
        }
        let mut done = 0;
        while done < flips {
            let i = (rng.next_u64() % N as u64) as usize;
            if llr[i].signum() == if cw[i] == 1 { 1.0 } else { -1.0 } {
                llr[i] = -llr[i];
                done += 1;
            }
        }
        llr
    }

    #[test]
    fn repair_limits_for_random_codewords() {
        let mut rng = Rng::new(11);
        let mut rate = Vec::new();
        for flips in [4, 8, 12, 40] {
            let ok = (0..40)
                .filter(|_| {
                    let cw = encode(with_crc(rng.next_u64() as u128 & ((1 << 77) - 1)));
                    decode(&flipped(&cw, flips, &mut rng), 50) == Some(cw)
                })
                .count();
            rate.push((flips, ok));
        }
        assert!(
            rate[0].1 == 40 && rate[1].1 >= 38 && rate[2].1 >= 20,
            "{rate:?}"
        );
        assert!(rate[3].1 <= 2, "{rate:?}");
    }

    #[test]
    fn ordered_statistics_recover_a_word_with_many_erasures() {
        let a91 = with_crc(0x0abc_def0_1234_5678_9abc);
        let cw = encode(a91);
        let mut llr = [0f32; N];
        for (i, l) in llr.iter_mut().enumerate() {
            let sign = if cw[i] == 1 { 1.0 } else { -1.0 };
            *l = if i % 5 < 2 {
                0.0
            } else {
                sign * (1.0 + (i % 7) as f32)
            };
        }
        assert_eq!(decode(&llr, 30), None);
        let (got, d) = osd(&llr, crc_ok).unwrap();
        assert_eq!((got, d), (cw, 0.0));
    }

    #[test]
    fn noise_never_yields_an_invalid_codeword() {
        let mut rng = Rng::new(12);
        for _ in 0..100 {
            let mut llr = [0f32; N];
            llr.iter_mut().for_each(|l| *l = 2.0 * rng.gauss() as f32);
            if let Some(cw) = decode(&llr, 30) {
                assert_eq!(errors(&cw), 0);
            }
        }
        assert_eq!(decode(&[0.0; N], 30), None);
        assert_eq!(decode(&[-5.0; N], 30), None);
    }
}
