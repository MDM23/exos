//! The one key everything signed derives from.
//!
//! There is a single configured secret, and every use derives its own subkey
//! from it by label. So the live token, a CSRF token, and whatever comes later
//! are independent of one another, and none of them is the secret itself:
//! recovering one tag tells an attacker nothing about the others.
//!
//! ```no_run
//! # fn main() -> Result<(), std::env::VarError> {
//! exos::keys(exos::Keys::from_secret(std::env::var("EXOS_SECRET")?));
//! # Ok(())
//! # }
//! ```
//!
//! # Unconfigured
//!
//! Nothing has to be configured, and what happens then is a random key per
//! process, announced on stderr. That is right for `cargo run`, where a restart
//! drops every stream anyway so no token outlives the process that minted it,
//! and wrong for everything else: two instances behind a load balancer never
//! agree, and a deploy invalidates every token in flight. The warning is there
//! because the failure mode is otherwise silent until it isn't.

use std::sync::OnceLock;

use hmac::{Hmac, Mac as _};
use sha2::{Digest as _, Sha256};
use subtle::ConstantTimeEq as _;

use crate::hex;

/// How much of the MAC a tag carries.
///
/// 128 bits is far beyond forgery and half the length in the attribute, which
/// is rendered once per live fragment on the page.
const TAG_BYTES: usize = 16;

/// Key material for signing.
///
/// Deliberately opaque. It cannot be read back out, and its [`Debug`] prints
/// nothing, because the one thing a key must never do is end up in a log line.
#[derive(Clone)]
pub struct Keys([u8; 32]);

impl Keys {
    /// Derives key material from a secret of any length.
    ///
    /// The secret is hashed rather than used raw, so a passphrase and 64 bytes
    /// of random both give 32 bytes of key and the caller never has to think
    /// about length.
    #[must_use]
    pub fn from_secret(secret: impl AsRef<[u8]>) -> Self {
        Self(Sha256::digest(secret.as_ref()).into())
    }

    /// Fresh key material from the operating system.
    ///
    /// # Panics
    ///
    /// If the operating system has no entropy to give. There is no sensible
    /// fallback: a guessable signing key is worse than not starting.
    #[must_use]
    pub fn random() -> Self {
        let mut key = [0_u8; 32];

        getrandom::fill(&mut key).expect("the operating system provides entropy for a signing key");

        Self(key)
    }

    /// The MAC of `message` under `label`'s subkey.
    fn sign(&self, label: &str, message: &[u8]) -> [u8; 32] {
        mac(&mac(&self.0, label.as_bytes()), message)
    }
}

impl core::fmt::Debug for Keys {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.debug_struct("Keys").finish_non_exhaustive()
    }
}

/// HMAC-SHA256, which is the only primitive in here.
fn mac(key: &[u8; 32], message: &[u8]) -> [u8; 32] {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC accepts a key of any length");

    mac.update(message);
    mac.finalize().into_bytes().into()
}

static KEYS: OnceLock<Keys> = OnceLock::new();

/// Configures the key everything signed derives from.
///
/// Call it once, before serving.
///
/// # Panics
///
/// If a key is already in place, either because this was called twice or
/// because something was signed first and got the random one. Both mean two
/// parts of the program disagree about the key, and quietly keeping the older
/// one would show up later as tokens that intermittently fail to verify.
pub fn keys(keys: Keys) {
    assert!(
        KEYS.set(keys).is_ok(),
        "the signing key is already in place; exos::keys goes once, before \
         anything is served"
    );
}

/// The configured key, or the random one this process fell back to.
fn configured() -> &'static Keys {
    KEYS.get_or_init(|| {
        eprintln!(
            "exos: no signing key configured, using a random one for this \
             process. Tokens will not survive a restart and two instances will \
             not agree. Call exos::keys with a secret before serving."
        );

        Keys::random()
    })
}

/// Signs `message` under `label`, as hex.
pub(crate) fn tag(label: &str, message: &[u8]) -> String {
    hex::encode(&configured().sign(label, message)[..TAG_BYTES])
}

/// Whether `candidate` is the tag for `message` under `label`.
pub(crate) fn verify(label: &str, message: &[u8], candidate: &str) -> bool {
    // Constant time over the whole tag. Comparing as strings would return on
    // the first wrong character, which hands back the position of that
    // character, and a tag can be learned one character at a time from that.
    tag(label, message)
        .as_bytes()
        .ct_eq(candidate.as_bytes())
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    // These exercise `Keys` rather than the process-wide key. Whichever test
    // signs something first initialises that one, so a test that set it would
    // pass or panic depending on the order the harness happened to pick.
    const LABEL: &str = "live-token";

    #[test]
    fn one_secret_always_gives_one_key() {
        let mac = |secret| Keys::from_secret(secret).sign(LABEL, b"live-presence-1");

        assert_eq!(mac("hunter2"), mac("hunter2"));
        assert_ne!(mac("hunter2"), mac("hunter3"));
    }

    #[test]
    fn a_secret_of_any_length_is_accepted() {
        let short = Keys::from_secret("x").sign(LABEL, b"topic");
        let long = Keys::from_secret([7_u8; 4096]).sign(LABEL, b"topic");

        assert_ne!(short, long);
    }

    /// The point of deriving by label: a tag recovered from one use tells an
    /// attacker nothing about any other use of the same secret.
    #[test]
    fn two_labels_sign_the_same_message_differently() {
        let keys = Keys::from_secret("hunter2");

        assert_ne!(
            keys.sign("live-token", b"topic"),
            keys.sign("csrf", b"topic")
        );
    }

    #[test]
    fn two_processes_without_a_secret_do_not_agree() {
        assert_ne!(
            Keys::random().sign(LABEL, b"topic"),
            Keys::random().sign(LABEL, b"topic")
        );
    }

    /// A key in a panic message or a log line is a key in a bug report.
    #[test]
    fn a_key_never_prints_itself() {
        let printed = format!("{:?}", Keys::from_secret("hunter2"));

        assert_eq!(printed, "Keys { .. }");
        assert!(!printed.contains("hunter2"));
    }

    #[test]
    fn a_tag_is_hex_and_half_the_mac() {
        let tagged = tag(LABEL, b"live-presence-1");

        assert_eq!(tagged.len(), TAG_BYTES * 2);
        assert!(
            tagged
                .chars()
                .all(|character| character.is_ascii_hexdigit())
        );
    }

    #[test]
    fn a_tag_verifies_only_against_what_it_signed() {
        let tagged = tag(LABEL, b"live-presence-1");

        assert!(verify(LABEL, b"live-presence-1", &tagged));
        assert!(!verify(LABEL, b"live-presence-2", &tagged));
        assert!(!verify("csrf", b"live-presence-1", &tagged));
        assert!(!verify(LABEL, b"live-presence-1", ""));
    }
}
