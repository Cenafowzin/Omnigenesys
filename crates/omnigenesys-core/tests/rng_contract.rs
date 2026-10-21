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
