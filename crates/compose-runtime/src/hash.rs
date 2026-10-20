//! Deterministic hashing for group keys and `key(…)` data keys.

use std::hash::{Hash, Hasher};

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// The group key of a declaration path, as a `const`: `key_of(concat!(module_path!(), "::Greeting"))`.
///
/// It is the 64-bit FNV-1a hash of the path, so it depends neither on line numbers nor on
/// formatting, and stays stable across unrelated edits.
pub const fn key_of(path: &str) -> u64 {
    let bytes = path.as_bytes();
    let mut hash = FNV_OFFSET;
    let mut index = 0;
    while index < bytes.len() {
        hash ^= bytes[index] as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
        index += 1;
    }
    hash
}

/// A deterministic hasher (FNV-1a), so data keys are identical across runs and machines.
pub struct KeyHasher(u64);

impl Default for KeyHasher {
    fn default() -> Self {
        KeyHasher(FNV_OFFSET)
    }
}

impl Hasher for KeyHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 ^= byte as u64;
            self.0 = self.0.wrapping_mul(FNV_PRIME);
        }
    }

    // Integers hash as little-endian, 64-bit for `usize`, so keys are the same on every target.
    fn write_u16(&mut self, value: u16) {
        self.write(&value.to_le_bytes())
    }

    fn write_u32(&mut self, value: u32) {
        self.write(&value.to_le_bytes())
    }

    fn write_u64(&mut self, value: u64) {
        self.write(&value.to_le_bytes())
    }

    fn write_u128(&mut self, value: u128) {
        self.write(&value.to_le_bytes())
    }

    fn write_usize(&mut self, value: usize) {
        self.write_u64(value as u64)
    }

    fn write_i16(&mut self, value: i16) {
        self.write_u16(value as u16)
    }

    fn write_i32(&mut self, value: i32) {
        self.write_u32(value as u32)
    }

    fn write_i64(&mut self, value: i64) {
        self.write_u64(value as u64)
    }

    fn write_i128(&mut self, value: i128) {
        self.write_u128(value as u128)
    }

    fn write_isize(&mut self, value: isize) {
        self.write_u64(value as i64 as u64)
    }
}

/// Hashes a `key(…)` argument into a movable group's data key.
pub fn hash_key<K: Hash + ?Sized>(key: &K) -> u64 {
    let mut hasher = KeyHasher::default();
    key.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_of_is_fnv1a() {
        assert_eq!(key_of(""), FNV_OFFSET);
        assert_eq!(key_of("a"), 0xaf63_dc4c_8601_ec8c);
        const KEY: u64 = key_of("app::Counter");
        assert_eq!(KEY, key_of("app::Counter"));
        assert_ne!(key_of("app::Counter"), key_of("app::Counter2"));
    }

    #[test]
    fn hash_key_is_deterministic() {
        assert_eq!(hash_key(&42u64), hash_key(&42u64));
        assert_ne!(hash_key(&1u64), hash_key(&2u64));
        assert_eq!(hash_key("abc"), hash_key(&String::from("abc")));
    }
}
