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
//!
//! The invariant is enforced rather than asked for. A fragment body renders
//! through [`detached`](crate::detached), so [`scope`](crate::scope) panics
//! inside one whether or not a request is being served: a fragment's arguments
//! are its whole input.

use core::hash::{Hash, Hasher as _};

use crate::{Markup, Render, escape_into, fnv::Fnv1a, keys};

mod stream;

pub use crate::live::stream::{connected, connection_count, publish, send};

pub(crate) use crate::live::stream::{disconnect, routes};

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
    ///
    /// # It is the same name in every build
    ///
    /// The hash is FNV-1a, written down in this crate, rather than
    /// `DefaultHasher`, whose algorithm std declines to promise across
    /// releases. That matters because a topic is the one name a client and a
    /// server have to agree on without either having been told it: a tab
    /// subscribes to what the instance that served the page produced, and a
    /// publish reaches it only if the instance that sends spells it the same
    /// way.
    ///
    /// Two binaries of one program built with different compilers are exactly
    /// the case that breaks, and it breaks silently: the fragment stops
    /// updating for the life of that document, no request fails, and a reload
    /// fixes it, so it reads as a network glitch. A rolling deploy is enough to
    /// produce it.
    ///
    /// What still renames a topic is a change to the arguments' own [`Hash`]
    /// implementations. Adding a field to a type used as a fragment argument
    /// renames every topic it appears in, which is a deploy that has to drop
    /// its documents.
    #[must_use]
    pub fn new(name: &str, arguments: &impl Hash) -> Self {
        let mut hasher = Fnv1a::new();
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
    ///
    /// HMAC-SHA256 under the key [`keys`](crate::keys) configures, truncated
    /// to 128 bits. It proves the server rendered this topic, which is not yet
    /// the same as proving it rendered it *for this viewer*: anywhere an id and
    /// token escape a page together, by a screenshot or a shared profile, the
    /// holder can subscribe. Binding the tag to a session id is what closes
    /// that, and needs a session to bind to.
    #[must_use]
    pub fn token(&self) -> String {
        keys::tag(LIVE_TOKEN, self.0.as_bytes())
    }

    /// Whether `token` was produced for this topic by this server.
    #[must_use]
    pub fn verify(&self, token: &str) -> bool {
        keys::verify(LIVE_TOKEN, self.0.as_bytes(), token)
    }
}

/// The label the live token derives its subkey under.
const LIVE_TOKEN: &str = "live-token";

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

    /// A written-down name rather than whatever this build happens to produce.
    ///
    /// The test above would pass against `DefaultHasher` too, and that is the
    /// point: a topic is the one name a client and a server agree on without
    /// either being told it, so a hash that drifts between compilers is a
    /// fragment that quietly stops updating on the far side of a rolling
    /// deploy. Changing this value is changing what every open tab is
    /// subscribed to, and it should take an argument rather than a rebuild.
    #[test]
    fn a_topic_is_named_the_same_way_by_every_build() {
        assert_eq!(
            Topic::new("presence", &(7_u32,)).as_str(),
            "live-presence-8eb61815f6eafc86"
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
