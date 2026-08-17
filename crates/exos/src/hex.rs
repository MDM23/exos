//! Bytes as text, which is how every opaque name in exos is spelled.
//!
//! A live token, a connection id and a session id are all bytes that have to
//! survive an HTML attribute, a JSON string and a cookie value without anything
//! needing to escape them. Hex is the encoding that needs no alphabet decision
//! and no padding rule, and the characters it costs over base64url are free at
//! sixteen and thirty-two bytes.

use core::fmt::Write as _;

/// `bytes` as lowercase hex.
pub(crate) fn encode(bytes: &[u8]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut out, byte| {
            write!(out, "{byte:02x}").expect("writing to a String cannot fail");
            out
        })
}

/// Whether `text` is exactly `bytes` bytes of lowercase hex.
///
/// The untrusted direction, for names that arrive from a browser. Rejecting the
/// shape before it becomes a store key is what stops a cookie from naming
/// something of any length the attacker chooses.
pub(crate) fn is(text: &str, bytes: usize) -> bool {
    text.len() == bytes * 2
        && text
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_byte_becomes_two_characters() {
        assert_eq!(encode(&[0x00, 0x0f, 0xff]), "000fff");
        assert_eq!(encode(&[]), "");
    }

    #[test]
    fn only_the_encoding_s_own_output_is_accepted() {
        assert!(is(&encode(&[7_u8; 16]), 16));

        assert!(!is("00FF", 2), "uppercase is not what encode writes");
        assert!(!is("00ff00", 2), "too long");
        assert!(!is("00", 2), "too short");
        assert!(!is("00gg", 2), "not hex");
    }
}
