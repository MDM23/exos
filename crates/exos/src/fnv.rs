//! A hash that means the same thing in every build.
//!
//! [`DefaultHasher`](std::collections::hash_map::DefaultHasher) is the obvious
//! thing to reach for and is wrong for anything whose value leaves the process.
//! Its algorithm is explicitly unspecified across releases, so two binaries of
//! one program built with different compilers can disagree about what a
//! [`Topic`](crate::Topic) is called. Nothing catches that: the tab subscribes
//! to the name the instance that served it produced, another instance publishes
//! under a different name, and the fragment simply stops updating, for the life
//! of that document, with no error anywhere.
//!
//! So the arithmetic is written down here instead. FNV-1a, 64-bit, which is
//! what [`signal`](crate::signal) already used for the same reason and now
//! shares.
//!
//! # What stability means here
//!
//! Every integer is written little-endian, and `usize` and `isize` are widened
//! before they are written, so the answer does not depend on the machine any
//! more than it depends on the compiler.
//!
//! What it cannot promise is that a value keeps its hash when its own
//! [`Hash`](core::hash::Hash) implementation changes. Adding a field to a type
//! used as a fragment argument renames every topic that argument appears in,
//! which is a deploy that has to drop its documents. That is the application's
//! to know about, and it is the same shape as changing a database column.

use core::hash::Hasher;

/// FNV-1a over whatever a [`Hash`](core::hash::Hash) implementation feeds it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Fnv1a(u64);

impl Fnv1a {
    /// The offset basis the algorithm starts from.
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;

    /// The prime it multiplies by after every byte.
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    pub(crate) const fn new() -> Self {
        Self(Self::OFFSET)
    }
}

/// The integer writes, in little endian rather than the native order the
/// default implementations use.
macro_rules! stable {
    ($($method:ident($type:ty)),* $(,)?) => {
        $(
            fn $method(&mut self, value: $type) {
                self.write(&value.to_le_bytes());
            }
        )*
    };
}

impl Hasher for Fnv1a {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 = (self.0 ^ u64::from(*byte)).wrapping_mul(Self::PRIME);
        }
    }

    stable!(
        write_i8(i8),
        write_i16(i16),
        write_i32(i32),
        write_i64(i64),
        write_i128(i128),
        write_u8(u8),
        write_u16(u16),
        write_u32(u32),
        write_u64(u64),
        write_u128(u128),
    );

    /// Widened first, so a 32-bit build and a 64-bit one agree.
    fn write_usize(&mut self, value: usize) {
        self.write_u64(value as u64);
    }

    /// Widened first, for the same reason as [`write_usize`](Self::write_usize).
    fn write_isize(&mut self, value: isize) {
        self.write_i64(value as i64);
    }
}

impl Default for Fnv1a {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use core::hash::Hash;

    use super::*;

    fn hashed(value: &impl Hash) -> u64 {
        let mut hasher = Fnv1a::new();
        value.hash(&mut hasher);
        hasher.finish()
    }

    /// The published test vector for the empty input, which is the offset basis
    /// itself, and for one byte, which pins the multiply.
    #[test]
    fn it_is_the_algorithm_it_says_it_is() {
        let mut hasher = Fnv1a::new();
        assert_eq!(hasher.finish(), 0xcbf2_9ce4_8422_2325);

        hasher.write(b"a");
        assert_eq!(hasher.finish(), 0xaf63_dc4c_8601_ec8c);
    }

    /// The whole point. These are golden values rather than a round trip,
    /// because a round trip would pass just as well against a hasher that
    /// changes between compilers, which is the failure this exists to stop.
    #[test]
    fn the_answer_is_written_down_rather_than_whatever_this_build_produces() {
        // A `str` hashes as its bytes followed by 0xff, which is std's doing
        // rather than this module's, and is therefore part of what is pinned.
        assert_eq!(hashed(&"presence"), 0x3872_d546_7adc_8f21);
        assert_eq!(hashed(&7_u32), 0x6d35_7266_9b2c_de42);
    }

    /// An integer is hashed the same way on a big-endian machine as on a
    /// little-endian one, which the default `write_u32` does not promise.
    #[test]
    fn an_integer_does_not_depend_on_the_machine() {
        let mut explicit = Fnv1a::new();
        explicit.write(&1_u32.to_le_bytes());

        assert_eq!(hashed(&1_u32), explicit.finish());
    }

    /// A pointer-sized integer is widened, so the two pointer widths agree.
    #[test]
    fn a_usize_is_hashed_as_a_u64() {
        assert_eq!(hashed(&1_usize), hashed(&1_u64));
        assert_eq!(hashed(&-1_isize), hashed(&-1_i64));
    }

    #[test]
    fn different_values_hash_differently() {
        assert_ne!(hashed(&7_u32), hashed(&8_u32));
        assert_ne!(hashed(&"presence"), hashed(&"status"));
    }
}
