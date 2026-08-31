//! Shared deterministic fingerprint primitives.

/// The FNV-1a 64-bit offset basis.
pub const FNV1A_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;

/// The FNV-1a 64-bit prime.
pub(crate) const FNV1A_PRIME: u64 = 0x0000_0100_0000_01b3;

/// Return the FNV-1a 64-bit hash of `bytes`.
pub(crate) fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = FNV1A_OFFSET_BASIS;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV1A_PRIME);
    }
    hash
}
