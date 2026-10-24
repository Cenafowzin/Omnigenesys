//! Coherent noise (PLANNING §2.11): smooth random fields, the base of
//! terrain. A value depends only on the seed and the position, never on
//! what was sampled before or around it, so every chunk of a map agrees.
//!
//! OpenSimplex2 (the "fast" 2D variant), ported from KdotJPG's reference
//! implementation, `rust/fast.rs` of github.com/KdotJPG/OpenSimplex2,
//! released under CC0 1.0 (public domain). Simplex noise on the A2* lattice
//! with a 24-direction gradient set: no grid-aligned artefacts, three
//! corners per sample, no permutation table (the lattice point is hashed
//! with the seed). Changes: gradients built at compile time, `floor` to
//! `i64` instead of a saturating `i32` cast.
//!
//! Only `+ − × ÷` and `floor`: IEEE makes them exact on every target, so
//! the same seed gives the same field natively and in wasm. No `sin`,
//! `exp` or `powf` (their results vary by platform), no `mul_add`.

use std::num::Wrapping;

const PRIME_X: i64 = 0x5205402B9270C86F;
const PRIME_Y: i64 = 0x598CD327003817B5;
const HASH_MULTIPLIER: i64 = 0x53A3F72DEEC546F5;

const SKEW_2D: f64 = 0.366025403784439;
const UNSKEW_2D: f64 = -0.21132486540518713;
const N_GRADS_2D_EXPONENT: i32 = 7;
const N_GRADS_2D: usize = 1 << N_GRADS_2D_EXPONENT;
const NORMALIZER_2D: f64 = 0.01001634121365712;
const RSQUARED_2D: f32 = 0.5;

// The reference computes these inline from UNSKEW_2D; same values.
const A1_T: f32 = (2.0 * (1.0 + 2.0 * UNSKEW_2D) * (1.0 / UNSKEW_2D + 2.0)) as f32;
const A1_C: f32 = (-2.0 * (1.0 + 2.0 * UNSKEW_2D) * (1.0 + 2.0 * UNSKEW_2D)) as f32;
const D1: f32 = (1.0 + 2.0 * UNSKEW_2D) as f32;
const D2_A: f32 = UNSKEW_2D as f32;
const D2_B: f32 = (UNSKEW_2D + 1.0) as f32;

#[rustfmt::skip]
const GRAD2_SRC: [f64; 48] = [
     0.38268343236509,   0.923879532511287,
     0.923879532511287,  0.38268343236509,
     0.923879532511287, -0.38268343236509,
     0.38268343236509,  -0.923879532511287,
    -0.38268343236509,  -0.923879532511287,
    -0.923879532511287, -0.38268343236509,
    -0.923879532511287,  0.38268343236509,
    -0.38268343236509,   0.923879532511287,
     0.130526192220052,  0.99144486137381,
     0.608761429008721,  0.793353340291235,
     0.793353340291235,  0.608761429008721,
     0.99144486137381,   0.130526192220051,
     0.99144486137381,  -0.130526192220051,
     0.793353340291235, -0.60876142900872,
     0.608761429008721, -0.793353340291235,
     0.130526192220052, -0.99144486137381,
    -0.130526192220052, -0.99144486137381,
    -0.608761429008721, -0.793353340291235,
    -0.793353340291235, -0.608761429008721,
    -0.99144486137381,  -0.130526192220052,
    -0.99144486137381,   0.130526192220051,
    -0.793353340291235,  0.608761429008721,
    -0.608761429008721,  0.793353340291235,
    -0.130526192220052,  0.99144486137381,
];

/// The 24 directions, scaled and repeated to fill the 128 slots the hash
/// indexes.
static GRAD2: [f32; N_GRADS_2D * 2] = {
    let mut out = [0.0f32; N_GRADS_2D * 2];
    let mut i = 0;
    while i < out.len() {
        out[i] = (GRAD2_SRC[i % GRAD2_SRC.len()] / NORMALIZER_2D) as f32;
        i += 1;
    }
    out
};

#[inline]
fn grad2(seed: Wrapping<i64>, xsvp: Wrapping<i64>, ysvp: Wrapping<i64>, dx: f32, dy: f32) -> f32 {
    let mut hash = seed ^ xsvp ^ ysvp;
    hash *= HASH_MULTIPLIER;
    hash ^= hash.0 >> (64 - N_GRADS_2D_EXPONENT + 1);
    let gi = (hash.0 as i32 & (((N_GRADS_2D - 1) << 1) as i32)) as usize;
    GRAD2[gi] * dx + GRAD2[gi | 1] * dy
}

/// 2D OpenSimplex2 at `(x, y)`: about −1 to 1, smooth, about one feature
/// per unit.
#[inline]
pub fn simplex2(seed: i64, x: f64, y: f64) -> f32 {
    // A2* lattice.
    let s = SKEW_2D * (x + y);
    let (xs, ys) = (x + s, y + s);
    let seed = Wrapping(seed);

    let (xsb, ysb) = (xs.floor(), ys.floor());
    let xi = (xs - xsb) as f32;
    let yi = (ys - ysb) as f32;
    let xsbp = Wrapping(xsb as i64) * Wrapping(PRIME_X);
    let ysbp = Wrapping(ysb as i64) * Wrapping(PRIME_Y);

    let t = (xi + yi) * UNSKEW_2D as f32;
    let dx0 = xi + t;
    let dy0 = yi + t;

    let mut value = 0.0;
    let a0 = RSQUARED_2D - dx0 * dx0 - dy0 * dy0;
    if a0 > 0.0 {
        value = (a0 * a0) * (a0 * a0) * grad2(seed, xsbp, ysbp, dx0, dy0);
    }

    let a1 = A1_T * t + (A1_C + a0);
    if a1 > 0.0 {
        let (dx1, dy1) = (dx0 - D1, dy0 - D1);
        value += (a1 * a1)
            * (a1 * a1)
            * grad2(
                seed,
                xsbp + Wrapping(PRIME_X),
                ysbp + Wrapping(PRIME_Y),
                dx1,
                dy1,
            );
    }

    if dy0 > dx0 {
        let (dx2, dy2) = (dx0 - D2_A, dy0 - D2_B);
        let a2 = RSQUARED_2D - dx2 * dx2 - dy2 * dy2;
        if a2 > 0.0 {
            value += (a2 * a2) * (a2 * a2) * grad2(seed, xsbp, ysbp + Wrapping(PRIME_Y), dx2, dy2);
        }
    } else {
        let (dx2, dy2) = (dx0 - D2_B, dy0 - D2_A);
        let a2 = RSQUARED_2D - dx2 * dx2 - dy2 * dy2;
        if a2 > 0.0 {
            value += (a2 * a2) * (a2 * a2) * grad2(seed, xsbp + Wrapping(PRIME_X), ysbp, dx2, dy2);
        }
    }
    value
}

/// How octaves are summed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FractalKind {
    /// Plain sum: rolling hills, coasts.
    Fbm,
    /// `1 − 2|n|` per octave: sharp crests and valleys, broken coasts.
    Ridged,
}

/// Octaves of noise, as FastNoiseLite sums them: each octave at
/// `lacunarity` times the frequency and `gain` times the amplitude of the
/// one before, with its own seed (`seed + i`), the whole scaled so the sum
/// stays within −1..1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fractal {
    octaves: u32,
    lacunarity: f64,
    gain: f32,
    kind: FractalKind,
    /// `1 / Σ |gain|^i`: the first amplitude, so the sum stays within −1..1.
    bounding: f32,
}

impl Fractal {
    pub fn new(octaves: u32, lacunarity: f64, gain: f32, kind: FractalKind) -> Fractal {
        let (mut amp, mut total) = (1.0f32, 0.0f32);
        for _ in 0..octaves {
            total += amp;
            amp *= gain.abs();
        }
        let bounding = if total > 0.0 { 1.0 / total } else { 1.0 };
        Fractal {
            octaves,
            lacunarity,
            gain,
            kind,
            bounding,
        }
    }

    /// The fractal at `(x, y)`, within −1..1.
    #[inline]
    pub fn sample2(&self, seed: i64, x: f64, y: f64) -> f32 {
        let mut amp = self.bounding;
        let (mut x, mut y, mut seed) = (x, y, seed);
        let mut sum = 0.0;
        for _ in 0..self.octaves {
            let n = simplex2(seed, x, y);
            sum += match self.kind {
                FractalKind::Fbm => n * amp,
                FractalKind::Ridged => (1.0 - 2.0 * n.abs()) * amp,
            };
            seed = seed.wrapping_add(1);
            x *= self.lacunarity;
            y *= self.lacunarity;
            amp *= self.gain;
        }
        sum
    }
}

#[cfg(test)]
mod tests;
