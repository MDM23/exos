//! Fragments that keep themselves up to date.
//!
//! A live fragment is markup with a name the **server** chose. That one
//! decision does most of the work:
//!
//! * The client never invents a topic, so there is no vocabulary to keep in
//!   step between two languages and nothing to typo.
//! * The name is derived from the function and its arguments, so the same
//!   fragment always lands on the same element and morphing keys on it.
//! * Subscribing is authorized by construction. A topic carries a token only
//!   this server can produce, handed out only by rendering the fragment, so
//!   being able to subscribe means the server already decided you could see
//!   it. There is no second permission check to write, or to forget.
//!
//! ```ignore
//! #[exos::live]
//! fn presence(user: u32) -> Markup {
//!     view! { <span class="dot" data-online={ online(user) }></span> }
//! }
//!
//! { presence(user.id) }        // in a template, renders it
//! exos::publish(presence(id)); // anywhere, re-renders and pushes it
//! ```
//!
//! # The invariant
//!
//! A topic must completely determine its content: the same topic means the
//! same HTML, for everybody. Presence satisfies that. Anything depending on
//! the *viewer*, their session, their permissions, their draft input, does
//! not, and must not be a live fragment, because two viewers would share a
//! topic and receive each other's content.
//!
//! When content depends on the viewer, either make the viewer part of the
//! topic, as in `inbox_count(user_id)`, or answer with an
//! [`Effect`](crate::Effect), which reaches only the requester.

use core::{
    hash::{Hash, Hasher},
    time::Duration,
};
use std::{
    collections::hash_map::DefaultHasher,
    sync::OnceLock,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{Markup, Render, escape_into};

mod stream;

pub use crate::live::stream::{connection_count, publish};

pub(crate) use crate::live::stream::routes;

// -----------------------------------------------------------------------------
//                                    TOPICS
// -----------------------------------------------------------------------------

/// The identity of a live fragment: a function and the arguments it was called
/// with, reduced to something a DOM id can be.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Topic(String);

impl Topic {
    /// Derives a topic from a fragment's name and its arguments.
    ///
    /// The name stays in the id so the DOM is readable in a debugger, and the
    /// hash disambiguates the arguments.
    #[must_use]
    pub fn new(name: &str, arguments: &impl Hash) -> Self {
        let mut hasher = DefaultHasher::new();
        name.hash(&mut hasher);
        arguments.hash(&mut hasher);

        Self(format!("live-{name}-{:016x}", hasher.finish()))
    }

    /// Wraps an id that arrived over the wire, so its token can be checked.
    ///
    /// Deliberately not a `From` implementation: this is the untrusted
    /// direction, and the only useful thing to do with the result is
    /// [`verify`](Self::verify).
    #[must_use]
    pub fn from_raw(id: &str) -> Self {
        Self(id.to_owned())
    }

    /// The topic as it appears in the DOM.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Proof that this server produced this topic.
    ///
    /// A topic id hashes public things, since a fragment name and a record id
    /// are both guessable, so the id alone would let anyone subscribe to
    /// anyone's fragment. The token is what makes a subscription unforgeable:
    /// it can only be obtained by being served the fragment.
    #[must_use]
    pub fn token(&self) -> String {
        let mut hasher = DefaultHasher::new();
        secret().hash(&mut hasher);
        self.0.hash(&mut hasher);

        format!("{:016x}", hasher.finish())
    }

    /// Whether `token` was produced for this topic by this server.
    #[must_use]
    pub fn verify(&self, token: &str) -> bool {
        // Not constant-time. A timing oracle here leaks a token for a topic
        // the attacker already knows the id of, and the real fix is a proper
        // MAC; see the note on `secret`.
        self.token() == token
    }
}

/// The process-wide signing secret.
///
/// Regenerated on every start, which is the right default: a restart drops
/// every stream anyway, so no token outlives the process that minted it.
///
/// This is a stand-in for a MAC. [`DefaultHasher`] is not a cryptographic
/// primitive and is not keyed the way HMAC is. Before this guards anything
/// real it wants HMAC-SHA256 with a configured key, and the token bound to a
/// session so it proves *this* viewer was served the fragment rather than
/// merely that somebody was.
fn secret() -> u64 {
    static SECRET: OnceLock<u64> = OnceLock::new();

    *SECRET.get_or_init(|| {
        let mut hasher = DefaultHasher::new();

        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_nanos()
            .hash(&mut hasher);

        std::process::id().hash(&mut hasher);
        hasher.finish()
    })
}

// -----------------------------------------------------------------------------
//                                  FRAGMENTS
// -----------------------------------------------------------------------------

/// A rendered live fragment: its markup, plus the topic that keeps it fresh.
///
/// The same value is used two ways, which is the point. Put it in a template
/// to render it, or hand it to [`publish`] to broadcast it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Fragment {
    topic: Topic,
    markup: Markup,
}

impl Fragment {
    /// Pairs markup with the topic that identifies it.
    #[must_use]
    pub fn new(topic: Topic, markup: Markup) -> Self {
        Self { topic, markup }
    }

    /// The topic this fragment answers to.
    #[must_use]
    pub fn topic(&self) -> &Topic {
        &self.topic
    }

    /// The wrapper that carries the subscription, as HTML.
    ///
    /// `display: contents` keeps the wrapper invisible to layout: a fragment
    /// inside a flex row must not become a box in it.
    #[must_use]
    pub fn to_markup(&self) -> Markup {
        let mut out = String::from("<exos-live style=\"display:contents\" id=\"");
        escape_into(self.topic.as_str(), &mut out);

        out.push_str("\" data-token=\"");
        escape_into(&self.topic.token(), &mut out);

        out.push_str("\">");
        out.push_str(self.markup.as_str());
        out.push_str("</exos-live>");

        Markup(out)
    }
}

impl Render for Fragment {
    fn render_to(&self, out: &mut String) {
        out.push_str(self.to_markup().as_str());
    }
}

// -----------------------------------------------------------------------------
//                                     TESTS
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_arguments_always_name_the_same_fragment() {
        // This is what lets a published patch land on the element that is
        // already there rather than appending a second one.
        assert_eq!(
            Topic::new("presence", &(7_u32,)),
            Topic::new("presence", &(7_u32,))
        );
        assert_ne!(
            Topic::new("presence", &(7_u32,)),
            Topic::new("presence", &(8_u32,))
        );
        assert_ne!(
            Topic::new("presence", &(7_u32,)),
            Topic::new("status", &(7_u32,))
        );
    }

    #[test]
    fn the_name_stays_readable_in_the_id() {
        assert!(
            Topic::new("presence", &(7_u32,))
                .as_str()
                .starts_with("live-presence-")
        );
    }

    #[test]
    fn a_token_only_opens_its_own_topic() {
        let mine = Topic::new("presence", &(1_u32,));
        let theirs = Topic::new("presence", &(2_u32,));

        assert!(mine.verify(&mine.token()));
        assert!(!mine.verify(&theirs.token()));
        assert!(!mine.verify("0000000000000000"));
    }

    #[test]
    fn the_wrapper_is_invisible_to_layout_and_carries_the_subscription() {
        let fragment = Fragment::new(
            Topic::new("presence", &(1_u32,)),
            Markup(String::from("<span>online</span>")),
        );

        let html = fragment.to_markup().into_string();

        assert!(html.contains("style=\"display:contents\""));
        assert!(html.contains("id=\"live-presence-"));
        assert!(html.contains("data-token=\""));
        assert!(html.contains("<span>online</span>"));
    }
}
