//! FROZEN CONTRACT (PLANNING §2.1). These vectors pin every map ever
//! generated. If one fails, the change broke compatibility: revert it.
//! Never update a vector.

use omnigenesys_core::coords::Coord;
use omnigenesys_core::rng::StepRng;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn stream_golden_vector() {
    let mut rng = StepRng::new("", 512, "base_floor");
    let mut buf = [0u8; 16];
    rng.fill_bytes(&mut buf);
    assert_eq!(hex(&buf), "d7b2685b307f802b247afbaa6df54e93");
}

/// Frozen 2026-10-21. Cross-checked against an independent Python
/// implementation (SHA-256 → ChaCha8 stream 1 → Philox4x32-10).
#[test]
fn at_golden_vectors() {
    let rng = StepRng::new("", 512, "base_floor");
    let cases = [
        (
            Coord::new(0, 0, 0),
            0,
            [0xbc57_3f0d, 0x3e17_f950, 0xcac5_b5d3, 0x8ad6_947a],
        ),
        (
            Coord::new(-1, 2, -3),
            0,
            [0x2cd8_a6ef, 0x195b_0b8c, 0x3af0_34c9, 0x3ec6_559b],
        ),
        (
            Coord::new(i32::MAX, i32::MIN, 7),
            0,
            [0x6e2e_afb3, 0x6d58_d0f2, 0x784d_8421, 0x563f_8a58],
        ),
        (
            Coord::new(10, 20, 5),
            1,
            [0x78d7_f079, 0x382b_cf9d, 0x2b71_93d9, 0xe2ee_30a4],
        ),
    ];
    for (c, draw, expected) in cases {
        assert_eq!(rng.at_draw(c, draw), expected, "{c:?} draw {draw}");
    }
}

/// Frozen 2026-10-26: the key of a retried attempt (a file's `attempts`).
/// Attempt 0 is the stream vector above; later ones hash `\0` and the
/// attempt number after the step id. Cross-checked against an independent
/// Python implementation (SHA-256 → ChaCha8 stream 0).
#[test]
fn attempt_golden_vectors() {
    let first16 = |salt: &str, seed: u64, step: &str, attempt: u32| {
        let mut rng = StepRng::with_attempt(salt, seed, step, attempt);
        let mut buf = [0u8; 16];
        rng.fill_bytes(&mut buf);
        hex(&buf)
    };
    assert_eq!(
        first16("", 512, "base_floor", 0),
        "d7b2685b307f802b247afbaa6df54e93"
    );
    assert_eq!(
        first16("", 512, "base_floor", 1),
        "99d19883585da55fe4e259331b956258"
    );
    assert_eq!(
        first16("", 512, "base_floor", 2),
        "78be35a17bf7b66a7a7edbc0f88193f9"
    );
    assert_eq!(
        first16("", 512, "base_floor", u32::MAX),
        "dc7e137c06894bc76b46a5fb672aa228"
    );
    assert_eq!(
        first16("s", 7, "chefe", 3),
        "f01cdefa25a1e7a6f98645cfe3f7bbf7"
    );
}
