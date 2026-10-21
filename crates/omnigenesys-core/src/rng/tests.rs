use super::*;

// Philox: Random123 known-answer vectors (tests/kat_vectors, philox4x32 10).
// They pin our implementation to the published function, bit for bit.

#[test]
fn philox_matches_the_random123_known_answers() {
    let cases = [
        (
            [0, 0, 0, 0],
            [0, 0],
            [0x6627_e8d5, 0xe169_c58d, 0xbc57_ac4c, 0x9b00_dbd8],
        ),
        (
            [u32::MAX; 4],
            [u32::MAX; 2],
            [0x408f_276d, 0x41c8_3b0e, 0xa20b_c7c6, 0x6d54_51fd],
        ),
        (
            [0x243f_6a88, 0x85a3_08d3, 0x1319_8a2e, 0x0370_7344],
            [0xa409_3822, 0x299f_31d0],
            [0xd16c_fe09, 0x94fd_cceb, 0x5001_e420, 0x2412_6ea1],
        ),
    ];
    for (ctr, key, expected) in cases {
        assert_eq!(philox4x32_10(ctr, key), expected, "ctr {ctr:08x?}");
    }
}

// Chance / pick

#[test]
fn chance_rejects_values_outside_zero_to_one() {
    for p in [-0.1, 1.0000001, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(Chance::new(p), None, "p = {p}");
    }
}

#[test]
fn chance_zero_never_hits_and_one_always_hits() {
    let (never, always) = (Chance::new(0.0).unwrap(), Chance::new(1.0).unwrap());
    for r in [0, 1, u32::MAX / 2, u32::MAX] {
        assert!(!never.hit(r), "r = {r}");
        assert!(always.hit(r), "r = {r}");
    }
}

#[test]
fn chance_threshold_is_p_times_two_to_the_32() {
    // One half hits exactly the lower half of the u32 range.
    let half = Chance::new(0.5).unwrap();
    assert!(half.hit(0x7FFF_FFFF));
    assert!(!half.hit(0x8000_0000));
    // The smallest non-zero step: exactly one value of r hits.
    let tiny = Chance::new(1.0 / 4_294_967_296.0).unwrap();
    assert!(tiny.hit(0));
    assert!(!tiny.hit(1));
}

#[test]
fn pick_covers_every_option_from_the_ends_of_the_range() {
    assert_eq!(pick(0, 3), 0);
    assert_eq!(pick(u32::MAX, 3), 2);
    assert_eq!(pick(u32::MAX, 1), 0);
    assert_eq!(pick(u32::MAX, u32::MAX), u32::MAX - 1);
}

#[test]
#[should_panic(expected = "pick needs at least one option")]
fn pick_from_nothing_panics() {
    pick(7, 0);
}

// at: how a coordinate enters the hash

fn rng(step_id: &str) -> StepRng {
    StepRng::new("", 512, step_id)
}

#[test]
fn at_is_a_pure_function_of_the_position() {
    let a = rng("scatter");
    let mut b = rng("scatter");
    // Using the stream in between must not change `at`.
    b.next_u64();
    let c = Coord::new(-7, 3, 2);
    assert_eq!(a.at(c), b.at(c));
    assert_eq!(a.at(c), a.at(c));
}

#[test]
fn every_axis_and_the_sign_of_each_coordinate_matter() {
    // Distinct cells, including negative ones and swapped axes, never share a
    // value: a mapping that dropped z, folded a sign, or added the axes
    // together would collide here.
    let r = rng("scatter");
    let mut seen = std::collections::HashSet::new();
    for z in -3..3 {
        for y in -20..20 {
            for x in -20..20 {
                assert!(seen.insert(r.at(Coord::new(x, y, z))), "({x}, {y}, {z})");
            }
        }
    }
    let c = Coord::new(5, 9, 1);
    assert_ne!(r.at(c), r.at(Coord::new(9, 5, 1)), "x and y swapped");
    assert_ne!(r.at(c), r.at(Coord::new(5, 1, 9)), "y and z swapped");
}

#[test]
fn draws_and_steps_are_independent_sources() {
    let (a, b) = (rng("trees"), rng("rocks"));
    let c = Coord::new(1, 2, 3);
    assert_eq!(a.at(c), a.at_draw(c, 0));
    assert_ne!(a.at_draw(c, 0), a.at_draw(c, 1));
    assert_ne!(a.at(c), b.at(c));
    assert_ne!(
        rng("trees").at(c),
        StepRng::new("other-game", 512, "trees").at(c)
    );
    assert_ne!(rng("trees").at(c), StepRng::new("", 513, "trees").at(c));
}

// at: statistical quality. Philox already passes BigCrush; these catch a bug
// in how we feed it (PLANNING §2.1). Everything is seeded, so the results are
// fixed: a pass is a pass every run.

/// Fraction of samples where `f` holds, over a 256×256×4 block.
fn fraction(mut f: impl FnMut(Coord) -> bool) -> f64 {
    let mut hits = 0u32;
    let mut total = 0u32;
    for z in 0..4 {
        for y in -128..128 {
            for x in -128..128 {
                hits += u32::from(f(Coord::new(x, y, z)));
                total += 1;
            }
        }
    }
    f64::from(hits) / f64::from(total)
}

/// |observed - p| within 5 standard deviations of a binomial over n samples.
fn assert_close(observed: f64, p: f64, n: f64, what: &str) {
    let sigma = (p * (1.0 - p) / n).sqrt().max(1e-12);
    let z = (observed - p).abs() / sigma;
    assert!(
        z < 5.0,
        "{what}: observed {observed}, expected {p} ({z:.1} sigma)"
    );
}

const SAMPLES: f64 = 256.0 * 256.0 * 4.0;

#[test]
fn hit_rate_matches_the_probability() {
    let r = rng("scatter");
    for p in [0.001, 0.1, 0.25, 0.5, 0.9] {
        let chance = Chance::new(p).unwrap();
        let observed = fraction(|c| chance.hit(r.at(c)[0]));
        assert_close(observed, p, SAMPLES, &format!("p = {p}"));
    }
}

#[test]
fn every_output_bit_is_fair() {
    // One pass, counting all 128 bits of each value.
    let r = rng("scatter");
    let mut ones = [0u32; 128];
    fraction(|c| {
        let v = r.at(c);
        for (k, count) in ones.iter_mut().enumerate() {
            *count += (v[k / 32] >> (k % 32)) & 1;
        }
        true
    });
    for (k, &n) in ones.iter().enumerate() {
        let what = format!("lane {} bit {}", k / 32, k % 32);
        assert_close(f64::from(n) / SAMPLES, 0.5, SAMPLES, &what);
    }
}

#[test]
fn neighbours_steps_and_lanes_are_uncorrelated() {
    // For each pair, how often the top bits agree. Correlated sources would
    // agree more (or less) than half the time.
    // Each pair gives the two values compared for cell `c`, from this step's
    // rng `r` and another step's `o`.
    type Pair = fn(&StepRng, &StepRng, Coord) -> (u32, u32);
    let pairs: [(&str, Pair); 6] = [
        ("x neighbour", |r, _, c| {
            (r.at(c)[0], r.at(c + Coord::new(1, 0, 0))[0])
        }),
        ("y neighbour", |r, _, c| {
            (r.at(c)[0], r.at(c + Coord::new(0, 1, 0))[0])
        }),
        ("z neighbour", |r, _, c| {
            (r.at(c)[0], r.at(c + Coord::new(0, 0, 1))[0])
        }),
        // Same cell on the diagonal, where it always agrees: 1 in 256
        // samples, well inside the tolerance.
        ("transposed", |r, _, c| {
            (r.at(c)[0], r.at(Coord::new(c.y, c.x, c.z))[0])
        }),
        ("other step", |r, o, c| (r.at(c)[0], o.at(c)[0])),
        ("lanes 0 and 1", |r, _, c| (r.at(c)[0], r.at(c)[1])),
    ];
    let (r, other) = (rng("scatter"), rng("other"));
    for (what, pair) in pairs {
        let agree = fraction(|c| {
            let (a, b) = pair(&r, &other, c);
            a >> 31 == b >> 31
        });
        assert_close(agree, 0.5, SAMPLES, what);
    }
}

#[test]
fn flipping_one_input_bit_flips_half_the_output() {
    // Avalanche: each of the 96 coordinate bits must reach every output bit.
    // Neighbouring cells differ in low bits, so this is what keeps them apart.
    let r = rng("scatter");
    let mut seed = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed as u32
    };
    const N: u32 = 4000;
    for axis in 0..3 {
        for bit in 0..32 {
            let mut flips = [0u32; 128];
            for _ in 0..N {
                let mut v = [next(), next(), next()];
                let a = r.at(Coord::new(v[0] as i32, v[1] as i32, v[2] as i32));
                v[axis] ^= 1 << bit;
                let b = r.at(Coord::new(v[0] as i32, v[1] as i32, v[2] as i32));
                for (k, flip) in flips.iter_mut().enumerate() {
                    *flip += ((a[k / 32] ^ b[k / 32]) >> (k % 32)) & 1;
                }
            }
            for (k, &f) in flips.iter().enumerate() {
                let what = format!("axis {axis} bit {bit} -> output bit {k}");
                assert_close(f64::from(f) / f64::from(N), 0.5, f64::from(N), &what);
            }
        }
    }
}
