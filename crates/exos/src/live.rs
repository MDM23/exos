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
//! { presence(user.id) }           // in a template, renders it
//! exos::publish(presence(id));    // anywhere, re-renders and pushes it
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

use core::{
    fmt,
    hash::{Hash, Hasher as _},
};

use crate::{Id, Markup, Render, escape_into, fnv::Fnv1a, keys};

mod bus;
mod stream;

pub use crate::live::{
    bus::{Frame, Kind, Sent, bus, deliver},
    stream::{connected, connection_count, publish, send},
};

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
    /// `#[live]` hashes the module a fragment was declared in alongside them,
    /// so two modules may each hold a `status` without becoming one topic. The
    /// module stays out of the readable half, which is for reading rather than
    /// for deciding anything, and out of this signature, which is for the
    /// fragments written by hand.
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
    /// its documents. Moving a fragment to another module is the same thing
    /// and reads as less of one, since it is a refactor rather than a change to
    /// the fragment: the tabs holding the old name stop updating until they are
    /// reloaded, and during a rolling deploy the two versions disagree.
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
    pub fn from_raw(id: &str) -> Self {
        Self(id.to_owned())
    }

    /// The topic as it appears in the DOM.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Proof that this server served this topic to this viewer.
    ///
    /// A topic id hashes public things, since a fragment name and a record id
    /// are both guessable, so the id alone would let anyone subscribe to
    /// anyone's fragment. The token is what makes a subscription unforgeable:
    /// it can only be obtained by being served the fragment, and only the
    /// browser it was served to can present it.
    ///
    /// HMAC-SHA256 under the key [`keys`](crate::keys) configures, over the
    /// topic and the session together, truncated to 128 bits. Binding it to
    /// the session is what makes an id and a token escaping a page, by a
    /// screenshot or a shared profile, worth nothing to whoever finds them.
    ///
    /// # It starts a session
    ///
    /// Being served a live fragment gives an anonymous visitor a name, because
    /// there has to be something to bind to. That is a cookie and nothing
    /// else, since exos keeps no store, so it costs a header rather than a
    /// row.
    ///
    /// # There is not always one
    ///
    /// `None` wherever there is no viewer to bind to, which is two places and
    /// both of them are right:
    ///
    /// * **A publish**, which renders outside any request. The patch it sends
    ///   therefore says nothing about the token, and the element it lands on
    ///   keeps the grant it was served with. A patch has never been a grant.
    /// * **Inside another fragment**, whose body renders through
    ///   [`detached`](crate::detached). A nested fragment's markup is
    ///   published to everybody watching the outer one, so a token in it would
    ///   be one viewer's grant handed to all of them. The mask that keeps a
    ///   fragment's content viewer-independent turns out to be the same rule.
    pub fn token(&self) -> Option<String> {
        let id = crate::session::current()?.start();

        Some(keys::tag(LIVE_TOKEN, self.bound(&id).as_bytes()))
    }

    /// Whether `token` was served to the viewer presenting it, for this topic.
    ///
    /// False for a browser carrying no session, since every token was made
    /// against one. That is a browser refusing cookies, and it costs live
    /// fragments rather than being quietly waved through.
    pub fn verify(&self, token: &str) -> bool {
        let Some(id) = crate::session::current().and_then(|session| session.id()) else {
            return false;
        };

        keys::verify(LIVE_TOKEN, self.bound(&id).as_bytes(), token)
    }

    /// What the tag is over.
    ///
    /// A session id is a fixed-length name, so the two cannot blur into one
    /// another whatever a fragment happens to be called.
    fn bound(&self, id: &Id) -> String {
        format!("{}{id}", self.0)
    }
}

/// The label the live token derives its subkey under.
const LIVE_TOKEN: &str = "live-token";

// -----------------------------------------------------------------------------
//                                  FRAGMENTS
// -----------------------------------------------------------------------------

/// A live fragment: the topic that keeps it fresh, and the render behind it.
///
/// The same value is used two ways, which is the point. Put it in a template
/// to render it, or hand it to [`publish`] to broadcast it.
///
/// # It carries the render rather than the markup
///
/// So that a fragment can be named before it exists. A publish is ordered
/// against every other publish **of its own topic** and has to hold that order
/// across the render, which it can only do if it can ask what the topic is
/// first. A value that arrived already rendered leaves nothing to ask, and the
/// order then has to be one lock for everything, where an expensive fragment
/// holds up an unrelated one.
///
/// The render runs whenever the markup is asked for, once per template the
/// fragment appears in and once per publish. Naming one costs a hash and
/// nothing else, which is what leaves the lock covering the whole read:
///
/// ```
/// use exos::Markup;
///
/// #[exos::live]
/// fn tag(name: &str) -> Markup {
///     Markup::default()
/// }
///
/// // Nothing has rendered here, and the argument was borrowed rather than
/// // kept, because naming the fragment is all this call does.
/// let label = String::from("shipped");
/// assert!(tag(&label).topic().as_str().starts_with("live-tag-"));
/// ```
///
/// # Why the render is a type parameter
///
/// Because a fragment is built on the way through a page and dropped again,
/// and a boxed closure would put an allocation and an indirect call in front of
/// every one of them, to buy an erasure that only somebody keeping fragments in
/// a collection needs. `#[live]` writes the type, so it is not spelled out
/// anywhere an application looks, and whoever does want them in a collection
/// writes `Fragment<Box<dyn Fn() -> Markup>>`, since a boxed closure is itself
/// a render.
#[derive(Clone)]
pub struct Fragment<R> {
    topic: Topic,
    render: R,
}

impl<R: Fn() -> Markup> Fragment<R> {
    /// Pairs a topic with the render that produces its markup.
    pub fn new(topic: Topic, render: R) -> Self {
        Self { topic, render }
    }

    /// The topic this fragment answers to.
    pub fn topic(&self) -> &Topic {
        &self.topic
    }

    /// Renders the markup the topic determines, without the wrapper around it.
    ///
    /// The two halves are worth telling apart: this is the same for everybody
    /// watching, which is the invariant a live fragment is held to, while the
    /// wrapper carries a grant to one browser and is the only part of a
    /// fragment that may differ between two viewers.
    pub fn markup(&self) -> Markup {
        (self.render)()
    }

    /// The wrapper that carries the subscription, as HTML.
    ///
    /// `display: contents` keeps the wrapper invisible to layout: a fragment
    /// inside a flex row must not become a box in it.
    ///
    /// The token is written where there is a viewer to bind one to and left
    /// out where there is not, which is every publish; see
    /// [`Topic::token`]. The client keeps a grant a patch does not restate,
    /// so what a publish sends is the content and the name, and the
    /// subscription stays the one the page was served with.
    pub fn to_markup(&self) -> Markup {
        let mut out = String::from("<exos-live style=\"display:contents\" id=\"");
        escape_into(self.topic.as_str(), &mut out);
        out.push('"');

        if let Some(token) = self.topic.token() {
            out.push_str(" data-token=\"");
            escape_into(&token, &mut out);
            out.push('"');
        }

        out.push('>');
        out.push_str(self.markup().as_str());
        out.push_str("</exos-live>");

        Markup(out)
    }
}

/// The topic and nothing else, since a render has nothing to print and running
/// it to find out would make formatting a fragment a side effect.
impl<R> fmt::Debug for Fragment<R> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Fragment")
            .field("topic", &self.topic)
            .finish_non_exhaustive()
    }
}

impl<R: Fn() -> Markup> Render for Fragment<R> {
    fn render_to(&self, out: &mut String) {
        out.push_str(self.to_markup().as_str());
    }
}

// -----------------------------------------------------------------------------
//                                     TESTS
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use core::cell::Cell;

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

    /// One render, since a fragment holds one rather than markup.
    fn online() -> Markup {
        Markup(String::from("<span>online</span>"))
    }

    /// The token for a topic, as one browser was served it.
    ///
    /// A scope is one request, so two calls are two browsers: nothing here can
    /// mint a token for a session it is not holding, which is the property the
    /// binding exists for and would be worth nothing if a test could step
    /// around it.
    fn served(topic: &Topic) -> String {
        crate::with_scope(|| topic.token().expect("a request has a viewer to bind to"))
    }

    #[test]
    fn a_token_only_opens_its_own_topic() {
        crate::with_scope(|| {
            let mine = Topic::new("presence", &(1_u32,));
            let theirs = Topic::new("presence", &(2_u32,));

            assert!(mine.verify(&mine.token().expect("a request mints one")));
            assert!(!mine.verify(&theirs.token().expect("a request mints one")));
            assert!(!mine.verify("0000000000000000"));
        });
    }

    /// The README's first gap, closed. An id and a token that escape a page
    /// together, by a screenshot or a shared profile, are worth nothing to
    /// whoever finds them, because the browser they were served to is part of
    /// what was signed.
    #[test]
    fn a_token_is_no_good_to_the_browser_it_was_not_served_to() {
        let topic = Topic::new("presence", &(1_u32,));
        let stolen = served(&topic);

        assert!(!crate::with_scope(|| topic.verify(&stolen)));
        assert!(crate::with_scope(|| topic.verify(
            &topic.token().expect("a request has a viewer to bind to")
        )));
    }

    /// A browser carrying no session has nothing to verify against, and is
    /// refused rather than waved through: waving it through would be the whole
    /// binding, undone by deleting a cookie.
    #[test]
    fn a_token_without_a_session_verifies_against_nothing() {
        let topic = Topic::new("presence", &(1_u32,));
        let token = served(&topic);

        crate::with_scope(|| {
            // A scope with a session that was never started: the browser sent
            // no cookie, so there is no name behind this request.
            assert!(crate::session().id().is_none());
            assert!(!topic.verify(&token));
        });
    }

    /// Being served a live fragment is what names an anonymous visitor, since
    /// there has to be something to bind the grant to.
    #[test]
    fn rendering_one_names_the_browser() {
        crate::with_scope(|| {
            assert!(crate::session().id().is_none());

            drop(Topic::new("presence", &(1_u32,)).token());

            assert!(crate::session().id().is_some());
        });
    }

    /// A fragment is a name and a way to produce the markup, and until
    /// something asks for the markup it has not been produced. That is what
    /// lets [`publish`] take the topic's lock before the render rather than
    /// after it, and what a fan-out would need to skip the combinations
    /// nobody is watching.
    #[test]
    fn a_fragment_renders_when_it_is_asked_to_and_not_before() {
        let renders = Cell::new(0_usize);

        let fragment = Fragment::new(Topic::new("presence", &(1_u32,)), || {
            renders.set(renders.get() + 1);
            online()
        });

        assert_eq!(renders.get(), 0, "naming it is not free");
        assert_eq!(fragment.markup(), online());
        assert_eq!(renders.get(), 1);

        drop(fragment.to_markup());
        assert_eq!(renders.get(), 2, "and again per publish");
    }

    #[test]
    fn the_wrapper_is_invisible_to_layout_and_carries_the_subscription() {
        let html = crate::with_scope(|| {
            Fragment::new(Topic::new("presence", &(1_u32,)), online)
                .to_markup()
                .into_string()
        });

        assert!(html.contains("style=\"display:contents\""));
        assert!(html.contains("id=\"live-presence-"));
        assert!(html.contains("data-token=\""));
        assert!(html.contains("<span>online</span>"));
    }

    /// What a publish sends, which is rendered outside every request. It names
    /// the fragment and says nothing about the subscription, because there is
    /// nobody there to grant one to. The client keeps the grant it was served.
    #[test]
    fn a_publish_carries_the_name_and_not_the_grant() {
        let html = Fragment::new(Topic::new("presence", &(1_u32,)), online)
            .to_markup()
            .into_string();

        assert!(html.contains("id=\"live-presence-"), "{html}");
        assert!(!html.contains("data-token"), "{html}");
        assert!(html.contains("<span>online</span>"), "{html}");
    }

    /// The same rule one level in. A nested fragment's markup is published to
    /// everybody watching the outer one, so a token in it would be one
    /// viewer's grant handed to all of them. The mask that keeps a fragment's
    /// content viewer-independent is what stops it, during a request as much
    /// as outside one.
    #[test]
    fn a_fragment_inside_a_fragment_carries_no_grant_either() {
        crate::with_scope(|| {
            let inner = crate::detached(|| {
                Fragment::new(Topic::new("presence", &(1_u32,)), Markup::default)
                    .to_markup()
                    .into_string()
            });

            assert!(!inner.contains("data-token"), "{inner}");
        });
    }
}
