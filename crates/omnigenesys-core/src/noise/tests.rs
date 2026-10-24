use super::*;

/// KdotJPG's reference 2D function, verbatim apart from formatting
/// (OpenSimplex2 `rust/fast.rs`, CC0 1.0): the oracle the port must match
/// bit for bit.
mod reference {
    use std::num::Wrapping;

    const PRIME_X: i64 = 0x5205402B9270C86F;
    const PRIME_Y: i64 = 0x598CD327003817B5;
    const HASH_MULTIPLIER: i64 = 0x53A3F72DEEC546F5;
    const SKEW_2D: f64 = 0.366025403784439;
    const UNSKEW_2D: f64 = -0.21132486540518713;
    const N_GRADS_2D_EXPONENT: i32 = 7;
    const N_GRADS_2D: i32 = 1 << N_GRADS_2D_EXPONENT;
    const NORMALIZER_2D: f64 = 0.01001634121365712;
    const RSQUARED_2D: f32 = 0.5;

    fn gradients() -> Vec<f32> {
        super::super::GRAD2_SRC
            .iter()
            .map(|v| (v / NORMALIZER_2D) as f32)
            .collect::<Vec<_>>()
            .into_iter()
            .cycle()
            .take((N_GRADS_2D * 2) as usize)
            .collect()
    }

    fn fast_floor(x: f64) -> i32 {
        let xi = x as i32;
        if x < xi as f64 { xi - 1 } else { xi }
    }

    fn grad2(
        g: &[f32],
        seed: Wrapping<i64>,
        xsvp: Wrapping<i64>,
        ysvp: Wrapping<i64>,
        dx: f32,
        dy: f32,
    ) -> f32 {
        let mut hash = seed ^ xsvp ^ ysvp;
        hash *= HASH_MULTIPLIER;
        hash ^= hash.0 >> (64 - N_GRADS_2D_EXPONENT + 1);
        let gi = (hash.0 as i32 & ((N_GRADS_2D - 1) << 1)) as usize;
        g[gi] * dx + g[gi | 1] * dy
    }

    pub fn noise2(seed: i64, x: f64, y: f64) -> f32 {
        let g = gradients();
        let s = SKEW_2D * (x + y);
        let xs = x + s;
        let ys = y + s;
        let seed = Wrapping(seed);
        let xsb = fast_floor(xs);
        let ysb = fast_floor(ys);
        let xi = (xs - xsb as f64) as f32;
        let yi = (ys - ysb as f64) as f32;
        let xsbp = Wrapping(xsb as i64) * Wrapping(PRIME_X);
        let ysbp = Wrapping(ysb as i64) * Wrapping(PRIME_Y);
        let t = (xi + yi) * UNSKEW_2D as f32;
        let dx0 = xi + t;
        let dy0 = yi + t;
        let mut value = 0.0;
        let a0 = RSQUARED_2D - dx0 * dx0 - dy0 * dy0;
        if a0 > 0.0 {
            value = (a0 * a0) * (a0 * a0) * grad2(&g, seed, xsbp, ysbp, dx0, dy0);
        }
        let a1 = (2.0 * (1.0 + 2.0 * UNSKEW_2D) * (1.0 / UNSKEW_2D + 2.0)) as f32 * t
            + ((-2.0 * (1.0 + 2.0 * UNSKEW_2D) * (1.0 + 2.0 * UNSKEW_2D)) as f32 + a0);
        if a1 > 0.0 {
            let dx1 = dx0 - (1.0 + 2.0 * UNSKEW_2D) as f32;
            let dy1 = dy0 - (1.0 + 2.0 * UNSKEW_2D) as f32;
            value += (a1 * a1)
                * (a1 * a1)
                * grad2(
                    &g,
                    seed,
                    xsbp + Wrapping(PRIME_X),
                    ysbp + Wrapping(PRIME_Y),
                    dx1,
                    dy1,
                );
        }
        if dy0 > dx0 {
            let dx2 = dx0 - UNSKEW_2D as f32;
            let dy2 = dy0 - (UNSKEW_2D + 1.0) as f32;
            let a2 = RSQUARED_2D - dx2 * dx2 - dy2 * dy2;
            if a2 > 0.0 {
                value += (a2 * a2)
                    * (a2 * a2)
                    * grad2(&g, seed, xsbp, ysbp + Wrapping(PRIME_Y), dx2, dy2);
            }
        } else {
            let dx2 = dx0 - (UNSKEW_2D + 1.0) as f32;
            let dy2 = dy0 - UNSKEW_2D as f32;
            let a2 = RSQUARED_2D - dx2 * dx2 - dy2 * dy2;
            if a2 > 0.0 {
                value += (a2 * a2)
                    * (a2 * a2)
                    * grad2(&g, seed, xsbp + Wrapping(PRIME_X), ysbp, dx2, dy2);
            }
        }
        value
    }
}

/// splitmix64: test points only.
fn points(n: usize, spread: f64) -> Vec<(i64, f64, f64)> {
    let mut s = 0x9e37_79b9_7f4a_7c15u64;
    let mut next = || {
        s = s.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = s;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    };
    (0..n)
        .map(|_| {
            let seed = next() as i64;
            let x = (next() as f64 / u64::MAX as f64 - 0.5) * spread;
            let y = (next() as f64 / u64::MAX as f64 - 0.5) * spread;
            (seed, x, y)
        })
        .collect()
}

#[test]
fn matches_the_reference_bit_for_bit() {
    // Within the reference's i32 floor range; the port widens it to i64.
    for (seed, x, y) in points(100_000, 2.0e6) {
        let (ours, theirs) = (simplex2(seed, x, y), reference::noise2(seed, x, y));
        assert_eq!(
            ours.to_bits(),
            theirs.to_bits(),
            "seed {seed} at ({x}, {y})"
        );
    }
    // Lattice points and halves, where floors and branches flip.
    for i in -40..40 {
        for j in -40..40 {
            let (x, y) = (f64::from(i) * 0.5, f64::from(j) * 0.5);
            assert_eq!(
                simplex2(7, x, y).to_bits(),
                reference::noise2(7, x, y).to_bits(),
                "({x}, {y})"
            );
        }
    }
}

#[test]
fn stays_within_one_and_is_centred() {
    let (mut lo, mut hi, mut sum) = (f32::MAX, f32::MIN, 0.0f64);
    let pts = points(200_000, 5000.0);
    for &(seed, x, y) in &pts {
        let v = simplex2(seed, x, y);
        lo = lo.min(v);
        hi = hi.max(v);
        sum += f64::from(v);
    }
    assert!(lo >= -1.0 && hi <= 1.0, "{lo}..{hi}");
    assert!(lo < -0.8 && hi > 0.8, "uses the range: {lo}..{hi}");
    assert!(
        (sum / pts.len() as f64).abs() < 0.01,
        "mean {}",
        sum / pts.len() as f64
    );
}

#[test]
fn is_smooth_and_seeded() {
    for (seed, x, y) in points(2000, 1000.0) {
        let d = (simplex2(seed, x, y) - simplex2(seed, x + 1e-4, y)).abs();
        assert!(d < 1e-2, "jump {d} at ({x}, {y})");
    }
    let differ = points(500, 1000.0)
        .iter()
        .filter(|&&(s, x, y)| simplex2(s, x, y) != simplex2(s ^ 1, x, y))
        .count();
    assert!(differ > 450, "another seed, another field: {differ}/500");
}

#[test]
fn far_coordinates_do_not_collapse() {
    // Past i32: the reference's floor saturates there; ours keeps going.
    let x = 3.0e9;
    let a = simplex2(1, x, 0.3);
    let b = simplex2(1, x + 7.3, 0.3);
    assert_ne!(a, b);
}

#[test]
fn fractals_stay_within_one() {
    for kind in [FractalKind::Fbm, FractalKind::Ridged] {
        for octaves in [1, 3, 6] {
            let f = Fractal::new(octaves, 2.0, 0.5, kind);
            for (seed, x, y) in points(20_000, 500.0) {
                let v = f.sample2(seed, x, y);
                assert!((-1.0..=1.0).contains(&v), "{kind:?} {octaves}: {v}");
            }
        }
    }
    // One octave of fBm is the noise itself.
    let one = Fractal::new(1, 2.0, 0.5, FractalKind::Fbm);
    assert_eq!(one.sample2(3, 1.25, -7.5), simplex2(3, 1.25, -7.5));
}

#[test]
fn golden_values() {
    // Pinned: any change here changes every map made with noise.
    let got: Vec<u32> = [(0i64, 0.5, 0.5), (512, 10.25, -3.75), (-9, 1234.5, 987.25)]
        .iter()
        .map(|&(s, x, y)| simplex2(s, x, y).to_bits())
        .collect();
    assert_eq!(got, [1_034_263_812, 1_058_949_830, 3_204_449_634]);
}
