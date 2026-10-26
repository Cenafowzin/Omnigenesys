//! Per-step randomness — FROZEN CONTRACT (PLANNING §2.1).
//!
//! Every value here ends up in generated maps. Changing the derivation, the
//! cipher, the hash, the byte order or how a coordinate enters `at` changes
//! every map ever generated. The golden vectors in `tests/rng_contract.rs`
//! guard this: if one fails, revert the change, never update the vector.

use chacha20::ChaCha8Rng;
use rand_core::{Rng, SeedableRng};
use sha2::{Digest, Sha256};

use crate::coords::Coord;

/// The randomness of one step, keyed by `SHA-256(salt ‖ seed_le64 ‖ step_id)`.
///
/// Two forms over the same key:
/// - [`at`](Self::at): a pure function of the position, for operators that
///   decide cell by cell. Same cell, same value, in any chunk, thread or order.
/// - a sequential stream ([`next_u32`](Self::next_u32) and friends), for
///   operators whose decisions depend on the whole map.
pub struct StepRng {
    stream: ChaCha8Rng,
    at_key: [u32; 2],
}

impl StepRng {
    pub fn new(salt: &str, seed: u64, step_id: &str) -> StepRng {
        StepRng::with_attempt(salt, seed, step_id, 0)
    }

    /// The randomness of the step in attempt `attempt` of a map (a file's
    /// `attempts`: a failed attempt is made again). Attempt 0 is
    /// [`new`](Self::new), byte for byte. A later attempt hashes a zero
    /// byte (never in a step id) and the attempt number, little-endian,
    /// after the step id: the seed and the salt stay the user's, so no
    /// attempt can land on another seed's map.
    pub fn with_attempt(salt: &str, seed: u64, step_id: &str, attempt: u32) -> StepRng {
        let mut hasher = Sha256::new();
        hasher.update(salt.as_bytes());
        hasher.update(seed.to_le_bytes());
        hasher.update(step_id.as_bytes());
        if attempt > 0 {
            hasher.update([0u8]);
            hasher.update(attempt.to_le_bytes());
        }
        let key: [u8; 32] = hasher.finalize().into();

        // The `at` key comes from ChaCha stream 1 of the same key; the main
        // stream is stream 0. Different nonces give unrelated keystreams, and
        // stream 0 is left untouched, so its golden vector still holds.
        let mut side = ChaCha8Rng::from_seed(key);
        side.set_stream(1);
        let at_key = [side.next_u32(), side.next_u32()];

        StepRng {
            stream: ChaCha8Rng::from_seed(key),
            at_key,
        }
    }

    /// Four independent values for cell `c`.
    #[inline]
    pub fn at(&self, c: Coord) -> [u32; 4] {
        self.at_draw(c, 0)
    }

    /// Four more independent values for cell `c`, for an operator that needs
    /// more than four per cell: draw 0 is [`at`](Self::at), then 1, 2, ...
    #[inline]
    pub fn at_draw(&self, c: Coord, draw: u32) -> [u32; 4] {
        // `as u32` reinterprets the bits of a negative coordinate (two's
        // complement); it never saturates or panics.
        philox4x32_10([c.x as u32, c.y as u32, c.z as u32, draw], self.at_key)
    }

    pub fn next_u32(&mut self) -> u32 {
        self.stream.next_u32()
    }

    pub fn next_u64(&mut self) -> u64 {
        self.stream.next_u64()
    }

    /// Bytes from the stream. Leftover bytes of the last 32-bit word are
    /// discarded, so the next call starts on a fresh word.
    pub fn fill_bytes(&mut self, dst: &mut [u8]) {
        self.stream.fill_bytes(dst);
    }
}

/// A probability turned into an integer threshold once, at build time, so the
/// per-cell decision is an integer comparison: no float, same on every platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chance(u64);

impl Chance {
    /// `None` unless `0 <= p <= 1`.
    pub fn new(p: f64) -> Option<Chance> {
        // Multiplying by 2^32 is exact in f64, and `round` is fully specified
        // by IEEE 754, so the threshold is the same everywhere.
        (0.0..=1.0)
            .contains(&p)
            .then(|| Chance((p * 4_294_967_296.0).round() as u64))
    }

    /// True with probability `p` for a uniform `r`. The threshold is a u64 so
    /// that `p = 1` (threshold 2^32) is true for every `r`.
    #[inline]
    pub fn hit(self, r: u32) -> bool {
        u64::from(r) < self.0
    }
}

/// One of `n` options, uniform for a uniform `r`, without a modulo or a retry
/// loop (Lemire's multiply-shift).
///
/// # Panics
///
/// If `n` is 0.
#[inline]
pub fn pick(r: u32, n: u32) -> u32 {
    assert!(n > 0, "pick needs at least one option");
    ((u64::from(r) * u64::from(n)) >> 32) as u32
}

/// Philox4x32 with 10 rounds (Salmon et al., "Parallel Random Numbers: As Easy
/// as 1, 2, 3", SC11): a keyed bijection on 128 bits. Checked against the
/// Random123 known-answer vectors in the tests.
#[inline]
fn philox4x32_10(mut ctr: [u32; 4], mut key: [u32; 2]) -> [u32; 4] {
    const M0: u64 = 0xD251_1F53;
    const M1: u64 = 0xCD9E_8D57;
    const W0: u32 = 0x9E37_79B9;
    const W1: u32 = 0xBB67_AE85;
    for round in 0..10 {
        if round > 0 {
            key = [key[0].wrapping_add(W0), key[1].wrapping_add(W1)];
        }
        let p0 = M0 * u64::from(ctr[0]);
        let p1 = M1 * u64::from(ctr[2]);
        let (hi0, lo0) = ((p0 >> 32) as u32, p0 as u32);
        let (hi1, lo1) = ((p1 >> 32) as u32, p1 as u32);
        ctr = [hi1 ^ ctr[1] ^ key[0], lo1, hi0 ^ ctr[3] ^ key[1], lo0];
    }
    ctr
}

#[cfg(test)]
mod tests;
