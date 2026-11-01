//! Powers and lengths that come out the same on every target, at the speed
//! of the platform's own.
//!
//! IEEE 754 fixes `+`, `*`, `/` and `sqrt` to the bit everywhere, but not
//! `pow`, `hypot`, `sin`… which come from each platform's math library
//! (pesquisa 2026-10-31-adaptador-unity, risco 1). The `libm` crate is the
//! same code everywhere but its `pow` costs 3× the Windows one (48 against
//! 16 ns a call, measured 2026-11-01). So:
//!
//! - a whole exponent, the usual case (2, 3), is multiplications:
//!   0.9 ns, exact by the standard;
//! - another exponent is `exp2(e · log2 x)` from `libm`: 20 ns, a few f64
//!   ulps from the exact power, gone when the value is stored as f32;
//! - a length is `sqrt(u² + v²)`: 4.3 ns, exact by the standard. Unlike
//!   `hypot` it overflows above 1e154, far beyond any distance on a map.
//!
//! The kind of exponent is settled when the operator is built, not per cell.

/// `x^e` for `x >= 0` and `e > 0`, with the way to take it chosen once.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Power {
    /// 1 to 64: multiplications.
    Whole(u32),
    Real(f64),
}

impl Power {
    /// `e` must be positive and finite (the operators check it first).
    pub(crate) fn new(e: f64) -> Power {
        debug_assert!(e > 0.0 && e.is_finite(), "{e}");
        if e.fract() == 0.0 && e <= 64.0 {
            Power::Whole(e as u32)
        } else {
            Power::Real(e)
        }
    }

    #[inline]
    pub(crate) fn of(self, x: f64) -> f64 {
        match self {
            Power::Whole(n) => whole(x, n),
            // x = 0: log2 is -inf and exp2(-inf) is 0, as it should be.
            Power::Real(e) => libm::exp2(e * libm::log2(x)),
        }
    }
}

/// `x^n` by squaring: at most 2·log2(n) multiplications, in an order fixed
/// by `n`, so the same bits everywhere.
#[inline]
fn whole(x: f64, mut n: u32) -> f64 {
    let (mut out, mut base) = (1.0, x);
    while n > 0 {
        if n & 1 == 1 {
            out *= base;
        }
        n >>= 1;
        if n > 0 {
            base *= base;
        }
    }
    out
}

/// The length of `(u, v)`.
#[inline]
pub(crate) fn length(u: f64, v: f64) -> f64 {
    (u * u + v * v).sqrt()
}

#[cfg(test)]
mod tests;
