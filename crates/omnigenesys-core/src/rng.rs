use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use sha2::{Digest, Sha256};

/// Per-step RNG derivation — FROZEN COMPATIBILITY CONTRACT (PLANNING §2.1).
///
/// ```text
/// step_rng = ChaCha8Rng::from_seed( SHA-256(salt ‖ seed_le64 ‖ step_id) )
/// ```
///
/// Changing the hash, the cipher, the byte order of `seed`, or the
/// concatenation order changes every map ever generated. Never touch
/// this within the same major version of the pipeline format.
pub fn derive_step_rng(salt: &str, seed: u64, step_id: &str) -> ChaCha8Rng {
    let mut hasher = Sha256::new();
    hasher.update(salt.as_bytes());
    hasher.update(seed.to_le_bytes());
    hasher.update(step_id.as_bytes());
    let digest: [u8; 32] = hasher.finalize().into();
    ChaCha8Rng::from_seed(digest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::RngCore;

    fn first_bytes(salt: &str, seed: u64, step_id: &str) -> [u8; 16] {
        let mut rng = derive_step_rng(salt, seed, step_id);
        let mut buf = [0u8; 16];
        rng.fill_bytes(&mut buf);
        buf
    }

    #[test]
    fn deterministic() {
        assert_eq!(first_bytes("s", 42, "step"), first_bytes("s", 42, "step"));
    }

    #[test]
    fn step_id_changes_stream() {
        assert_ne!(first_bytes("", 42, "a"), first_bytes("", 42, "b"));
    }

    #[test]
    fn salt_changes_stream() {
        assert_ne!(first_bytes("game1", 42, "a"), first_bytes("game2", 42, "a"));
    }

    #[test]
    fn seed_changes_stream() {
        assert_ne!(first_bytes("", 1, "a"), first_bytes("", 2, "a"));
    }

    /// Golden vector freezing the derivation contract. If this test ever
    /// fails, the derivation changed and every existing map is broken —
    /// do NOT update the constant; revert the change instead.
    #[test]
    fn frozen_contract_golden_vector() {
        let bytes = first_bytes("", 512, "base_floor");
        let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(hex, "d7b2685b307f802b247afbaa6df54e93");
    }
}
