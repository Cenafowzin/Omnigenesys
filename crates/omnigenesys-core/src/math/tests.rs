// The platform's math is the oracle here (clippy.toml forbids it elsewhere).
#![allow(clippy::disallowed_methods)]

use super::*;

#[test]
fn whole_exponents_are_multiplications() {
    assert_eq!(Power::new(2.0), Power::Whole(2));
    assert_eq!(Power::new(64.0), Power::Whole(64));
    assert_eq!(Power::new(65.0), Power::Real(65.0));
    assert_eq!(Power::new(2.5), Power::Real(2.5));
    for x in [0.0, 1e-300, 0.1, 0.5, 0.999, 1.0, 3.7] {
        assert_eq!(Power::new(1.0).of(x), x);
        assert_eq!(Power::new(2.0).of(x), x * x);
        assert_eq!(Power::new(3.0).of(x), x * x * x);
        // Squaring: x · (x² · x²).
        assert_eq!(Power::new(5.0).of(x), x * ((x * x) * (x * x)));
    }
}

#[test]
fn powers_are_close_to_the_exact_ones() {
    for e in [
        0.25,
        0.4,
        1.0 / 3.0,
        1.0,
        2.0,
        2.5,
        3.0,
        4.0,
        7.3,
        13.0,
        64.0,
        100.0,
    ] {
        let p = Power::new(e);
        for i in 0..=1000 {
            let x = f64::from(i) / 1000.0;
            let (got, want) = (p.of(x), x.powf(e));
            assert!(
                // exp2(e · log2 x) carries the rounding of the product, a
                // few ulps per unit of |e · log2 x|; squaring, one per
                // multiplication, up to 2 · log2(e).
                (got - want).abs()
                    <= 4.0
                        * f64::EPSILON
                        * (2.0 + (e * x.log2()).abs() + 2.0 * e.log2().max(0.0))
                        * want
                    || got == want,
                "{x}^{e}: {got} against {want}"
            );
        }
    }
}

#[test]
fn the_ends_are_exact() {
    for e in [0.25, 2.5, 3.0, 100.0] {
        assert_eq!(Power::new(e).of(0.0), 0.0, "0^{e}");
        assert_eq!(Power::new(e).of(1.0), 1.0, "1^{e}");
    }
}

#[test]
fn length_is_the_hypotenuse() {
    assert_eq!(length(3.0, 4.0), 5.0);
    assert_eq!(length(0.0, 0.0), 0.0);
    for (u, v) in [(0.1, 0.2), (1e-3, 7.0), (123.4, 0.5)] {
        assert!((length(u, v) - u.hypot(v)).abs() <= 1e-15 * u.hypot(v));
    }
}
