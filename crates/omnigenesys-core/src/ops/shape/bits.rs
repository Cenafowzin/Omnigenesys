//! Bit packing shared by the shape operators.

/// 64 bytes of 0 or 1 to 64 bits, byte i to bit i. Per 8 bytes, the
/// multiply moves byte i's bit to bit 56 + i; the partial products land on
/// distinct bits, so nothing carries into the top byte. `from_le_bytes`
/// fixes the byte order, so it is the same on big-endian.
#[inline]
pub(super) fn pack(bytes: &[u8; 64]) -> u64 {
    let mut word = 0;
    for (j, b) in bytes.chunks_exact(8).enumerate() {
        let v = u64::from_le_bytes(b.try_into().expect("8 bytes"));
        word |= (v.wrapping_mul(0x0102_0408_1020_4080) >> 56) << (8 * j);
    }
    word
}
