//! A name for the browser on the other end, and the cookie that carries it.
//!
//! exos mints an opaque id, puts it in a cookie, and hands it back on every
//! request that carries one. What that id *means* is the application's:
//!
//! ```
//! # fn main() -> Result<(), Box<dyn core::error::Error>> {
//! # exos::with_scope(|| {
//! # let authenticated = 7_u32;
//! # struct Sessions;
//! # impl Sessions { fn bind(&self, _: &exos::Id, _: u32) {} }
//! # let sessions = Sessions;
//! // At sign-in: a new name, then whatever the application keeps under it.
//! let id = exos::session().rotate();
//!
//! sessions.bind(&id, authenticated);
//! # });
//! # Ok(())
//! # }
//! ```
//!
//! # Why exos stops here
//!
//! Because the next step needs to await, and the step after that cannot.
//!
//! An application resolves the id to whatever it means, in a handler, where
//! awaiting a database is legal, and drops the result into
//! [`scope`](crate::scope). Every view underneath then reads it synchronously,
//! which is the job the request scope already exists for. A framework holding
//! the contents would have to load them before the handler runs, because a
//! [`view!`](crate::view) fragment is a plain function and cannot await, and
//! would therefore pay for a session on every request that carried a cookie
//! rather than on the ones that use it.
//!
//! So exos owns the part with no choices in it, and none of the parts with
//! choices: not where sessions are stored, not what is in one, not when one
//! expires, and not what a user is. An application already has a database, and
//! a table it can index, join and expire is worth more than anything reachable
//! through a trait exos invented.
//!
//! Nothing here can fail, which is the argument that the line is in the right
//! place.
//!
//! # What is in the cookie
//!
//! A name, and nothing else.
//!
//! ```text
//! Set-Cookie: exos=<128 random bits, as hex>;
//!             HttpOnly; Max-Age=34560000; Path=/; SameSite=Lax; Secure
//! ```
//!
//! **The `Max-Age` is not the session's lifetime.** It is as long as browsers
//! will accept, deliberately, so that the cookie is never the thing that ends a
//! session: what the id still means is decided by whatever the application
//! keeps, and a cookie naming something it has forgotten is simply anonymous
//! and costs a lookup. A cookie expiring first would sign somebody out while
//! their record was still perfectly good, and exos has no way to know when that
//! is.
//!
//! `Secure` is not configurable. Every browser treats `localhost` and
//! `127.0.0.1` as a secure context, so `cargo run` is unaffected, and anything
//! else serving a session over plain HTTP has a larger problem than this
//! attribute.
//!
//! Nothing is sent until something asks for an id. A crawler, a health check or
//! an anonymous read gets no `Set-Cookie`, and a browser that already holds the
//! cookie is not sent it again.
//!
//! One edge worth knowing: a session started *during* a stream cannot set a
//! cookie, because those headers went out when the stream opened. Streams
//! should read the id and never start one.
//!
//! # Where it cannot be reached
//!
//! [`session`] is built on [`scope`](crate::scope) and inherits both of its
//! rules. Outside a request, asking panics, because a background job reading
//! the request is a mistake made once rather than a condition every caller
//! handles. Inside a live fragment, asking also panics: a fragment renders
//! again from whatever publishes it, so its arguments are its whole input.
//!
//! Under [`with_scope`](crate::with_scope) there is no layer, and [`session`]
//! answers with one that mints and rotates exactly as it would in a request and
//! is thrown away with the scope.

use std::sync::{Arc, Mutex, MutexGuard};

use axum::{
    extract::Request,
    http::{HeaderMap, HeaderValue, header},
    middleware::Next,
    response::Response,
};

use crate::hex;

// -----------------------------------------------------------------------------
//                                  THE SESSION
// -----------------------------------------------------------------------------

/// How much entropy a session id carries.
///
/// 128 bits, the same as a connection id and a live token, and for the same
/// reason: the id is a bearer name, so anybody who can produce one is whoever
/// it names until the application stops honouring it.
const ID_BYTES: usize = 16;

/// A session's name, and the only thing the cookie carries.
///
/// Opaque by construction. It says nothing about who the session belongs to,
/// which is what lets the application change what it keeps under one without
/// the browser ever having to be told.
#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Id(String);

impl Id {
    /// A name no other session has and no browser can guess.
    ///
    /// # Panics
    ///
    /// If the operating system has no entropy to give, which is where
    /// [`Keys::random`](crate::Keys::random) already stands and has the same
    /// answer: a guessable id is worse than not starting.
    #[must_use]
    pub fn random() -> Self {
        let mut bytes = [0_u8; ID_BYTES];

        getrandom::fill(&mut bytes)
            .expect("the operating system provides entropy for a session id");

        Self(hex::encode(&bytes))
    }

    /// Reads an id that arrived in a cookie, if it has the shape of one.
    ///
    /// Deliberately not [`FromStr`](core::str::FromStr): this is the untrusted
    /// direction, and the shape check is the point. Without it a cookie could
    /// name a database row of any length and any content the sender liked.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        hex::is(text, ID_BYTES).then(|| Self(text.to_owned()))
    }

    /// The id, as an application keys on it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl core::fmt::Debug for Id {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // An id in a log line is a hijacked session in a bug report, which is
        // the argument `Keys` already makes about key material. Anything that
        // genuinely needs to print one has `as_str`.
        formatter.debug_tuple("Id").finish_non_exhaustive()
    }
}

impl core::fmt::Display for Id {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// The session of the request being served.
///
/// A handle, not a guard. Cloning it is an [`Arc`] bump and every clone names
/// the same session, so it can be passed around freely within the request it
/// came from.
#[derive(Clone)]
pub struct Session(Arc<State>);

impl Session {
    /// The name the browser sent, if it sent one shaped like a name.
    ///
    /// `None` is an anonymous visit. Asking does not start a session, so a page
    /// that only looks costs no cookie.
    #[must_use]
    pub fn id(&self) -> Option<Id> {
        self.inner().id.clone()
    }

    /// The session's name, starting one if there is none.
    ///
    /// Idempotent: a visitor who already has a name keeps it. This is what an
    /// anonymous shopping cart wants, and it is the wrong call at a sign-in,
    /// where [`rotate`](Self::rotate) is.
    #[must_use]
    pub fn start(&self) -> Id {
        let mut inner = self.inner();

        inner.id.get_or_insert_with(Id::random).clone()
    }

    /// A new name for the session, whatever it was called before.
    ///
    /// This is the session fixation defence, and it is exos's rather than the
    /// application's because it is a cookie operation and because forgetting it
    /// is silent. Call it at every privilege change:
    ///
    /// ```
    /// # fn main() -> Result<(), Box<dyn core::error::Error>> {
    /// # exos::with_scope(|| {
    /// # let user = 7_u32;
    /// # struct Sessions;
    /// # impl Sessions {
    /// #     fn bind(&self, _: &exos::Id, _: u32) {}
    /// #     fn forget(&self, _: &exos::Id) {}
    /// # }
    /// # let sessions = Sessions;
    /// let session = exos::session();
    ///
    /// // Before, because rotating replaces it: an anonymous visit may have
    /// // left a cart under the old name that is worth moving or dropping.
    /// let previous = session.id();
    /// let id = session.rotate();
    ///
    /// sessions.bind(&id, user);
    ///
    /// if let Some(previous) = previous {
    ///     sessions.forget(&previous);
    /// }
    /// # });
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// A session that had no name gets one, because the call site that rotates
    /// is about to need one.
    #[must_use]
    pub fn rotate(&self) -> Id {
        let id = Id::random();

        self.inner().id = Some(id.clone());

        id
    }

    /// Takes the cookie back, so the next request is anonymous.
    ///
    /// exos's half of signing out. Whatever the application stored under the id
    /// is the application's to delete, and it should, because a name the
    /// browser has stopped sending is not a name nobody else has.
    pub fn end(&self) {
        self.inner().id = None;
    }

    fn inner(&self) -> MutexGuard<'_, Inner> {
        self.0
            .0
            .lock()
            .expect("the session lock is never held across a panic")
    }
}

impl core::fmt::Debug for Session {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Never the id, which is a bearer name.
        formatter
            .debug_struct("Session")
            .field("anonymous", &self.inner().id.is_none())
            .finish()
    }
}

/// The session of the request being served.
///
/// # Panics
///
/// If there is no request, or if the caller is inside a live fragment. Both are
/// programming mistakes rather than conditions to handle, and
/// [`scope`](crate::scope) says why.
#[must_use]
pub fn session() -> Session {
    let scope = crate::scope();

    if let Some(state) = scope.get::<State>() {
        return Session(state);
    }

    // Only reachable under `with_scope`, because the layer puts a state in
    // every request it serves. A test that renders a view gets a session that
    // behaves like one and that goes away with the scope.
    scope.set(State::default());

    Session(scope.get::<State>().expect("it was set on the line above"))
}

/// One request's session while it is being served.
///
/// Lives in the request [scope](crate::scope), which is what gives [`session`]
/// its two panics for free and keeps two requests in flight from naming each
/// other's viewer.
#[derive(Debug, Default)]
struct State(Mutex<Inner>);

#[derive(Debug, Default)]
struct Inner {
    /// The name the browser sent, whether or not it still names anything.
    ///
    /// Kept apart from `id` because the pair is what the response is decided
    /// from: a name with no cookie behind it has to be sent, and a cookie with
    /// no name behind it has to be taken back.
    cookie: Option<Id>,

    /// The session's name now: minted by `start`, replaced by `rotate`, taken
    /// away by `end`.
    id: Option<Id>,
}

// -----------------------------------------------------------------------------
//                                  THE COOKIE
// -----------------------------------------------------------------------------

/// The cookie's name, which is also the crate's.
const COOKIE: &str = "exos";

/// How long the browser is asked to keep the name.
///
/// Four hundred days, which is as long as browsers will accept, because this
/// is not the session's lifetime and should never be mistaken for one. The
/// module docs say why the generous direction is the safe one.
const MAX_AGE: u32 = 400 * 24 * 60 * 60;

/// Reads the name on the way in and writes it on the way out.
///
/// Mounted by [`app`](crate::app) inside the request scope. It awaits nothing
/// of its own: there is no store to reach, so a request that neither carries a
/// name nor asks for one costs a header lookup that finds nothing.
pub(crate) async fn layer(request: Request, next: Next) -> Response {
    let arriving = arriving(request.headers());

    let scope = crate::scope();
    scope.set(State(Mutex::new(Inner {
        cookie: arriving.clone(),
        id: arriving,
    })));

    let state = scope
        .get::<State>()
        .expect("it was set on the line above; nothing else writes this type");

    let mut response = next.run(request).await;

    if let Some(cookie) = pending(&state) {
        response.headers_mut().append(header::SET_COOKIE, cookie);
    }

    response
}

/// What the browser has to be told, if anything.
fn pending(state: &State) -> Option<HeaderValue> {
    let inner = state
        .0
        .lock()
        .expect("the session lock is never held across a panic");

    match (inner.id.as_ref(), inner.cookie.as_ref()) {
        // Ended, so the browser should stop sending it.
        (None, Some(_)) => Some(expired()),
        // Started or rotated. A name the browser already has is not sent again.
        (Some(id), sent) if Some(id) != sent => Some(issued(id)),
        _ => None,
    }
}

/// The session name the request carried, if it carried one shaped like a name.
///
/// `Cookie` may arrive more than once and each header may hold several pairs,
/// so both are walked rather than assumed.
fn arriving(headers: &HeaderMap) -> Option<Id> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .filter_map(|pair| pair.split_once('='))
        .find(|(name, _)| name.trim() == COOKIE)
        .and_then(|(_, id)| Id::parse(id.trim()))
}

/// The header that names the session.
fn issued(id: &Id) -> HeaderValue {
    let cookie =
        format!("{COOKIE}={id}; HttpOnly; Max-Age={MAX_AGE}; Path=/; SameSite=Lax; Secure");

    HeaderValue::try_from(cookie).expect("an id is hex and every attribute is a literal")
}

/// The header that takes it away.
fn expired() -> HeaderValue {
    HeaderValue::from_static("exos=; HttpOnly; Max-Age=0; Path=/; SameSite=Lax; Secure")
}

// -----------------------------------------------------------------------------
//                                     TESTS
// -----------------------------------------------------------------------------

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "a failing assertion is the point of a test"
)]
mod tests {
    use super::*;
    use crate::{detached, with_scope};

    /// The handle a handler would be given for a request carrying `cookie`, and
    /// what the layer decides from it afterwards. Together these are the layer
    /// either side of a handler, which is what lets one test name one rule.
    fn serving(cookie: Option<Id>) -> Session {
        Session(Arc::new(State(Mutex::new(Inner {
            cookie: cookie.clone(),
            id: cookie,
        }))))
    }

    fn owed(session: &Session) -> Option<String> {
        pending(&session.0).map(|header| header.to_str().expect("a cookie is text").to_owned())
    }

    fn headers(cookie: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_str(cookie).expect("valid"),
        );
        headers
    }

    // ---- naming a session ---------------------------------------------------

    #[test]
    fn a_visit_that_asks_for_nothing_has_no_name() {
        with_scope(|| {
            let session = session();

            assert!(session.id().is_none());
            assert!(session.id().is_none(), "asking does not start one");
        });
    }

    #[test]
    fn starting_is_the_same_answer_every_time() {
        with_scope(|| {
            let session = session();

            let first = session.start();

            assert_eq!(session.start(), first);
            assert_eq!(session.id().as_ref(), Some(&first));
        });
    }

    #[test]
    fn rotating_always_gives_a_different_name() {
        with_scope(|| {
            let session = session();

            let first = session.start();
            let second = session.rotate();

            assert_ne!(second, first);
            assert_ne!(session.rotate(), second);
            assert_eq!(session.id().as_ref(), Some(&session.start()));
        });
    }

    /// The call site that rotates is a sign-in, which is about to need a name
    /// whether or not the visitor arrived with one.
    #[test]
    fn rotating_an_anonymous_session_still_gives_a_name() {
        with_scope(|| {
            let session = session();

            let id = session.rotate();

            assert_eq!(session.id().as_ref(), Some(&id));
        });
    }

    #[test]
    fn ending_takes_the_name_away() {
        with_scope(|| {
            let session = session();
            let id = session.start();

            session.end();

            assert!(session.id().is_none());
            assert_ne!(session.start(), id, "and starting again is a new one");
        });
    }

    /// Every handle names the same session, which is what makes reaching for it
    /// again three frames down cheap rather than wrong.
    #[test]
    fn two_handles_in_one_request_name_one_session() {
        with_scope(|| {
            let first = session();
            let id = session().start();

            assert_eq!(first.id(), Some(id));
        });
    }

    /// A name in a log line is a hijacked session in a bug report.
    #[test]
    fn neither_a_session_nor_a_name_prints_itself() {
        with_scope(|| {
            let session = session();
            let id = session.start();

            let printed = format!("{session:?}");

            assert_eq!(printed, "Session { anonymous: false }");
            assert!(!printed.contains(id.as_str()));
            assert_eq!(format!("{id:?}"), "Id(..)");

            // And what genuinely has to print one says so.
            assert_eq!(format!("{id}"), id.as_str());
        });
    }

    #[test]
    #[should_panic(expected = "no request scope")]
    fn asking_outside_a_request_is_a_panic() {
        let _ = session();
    }

    #[test]
    #[should_panic(expected = "a live fragment cannot read the request scope")]
    fn a_live_fragment_cannot_reach_it() {
        with_scope(|| detached(session));
    }

    // ---- what a request owes on the way out ---------------------------------

    /// A visit that asks for no name leaves no trace on the response.
    #[test]
    fn an_anonymous_request_owes_nothing() {
        assert!(owed(&serving(None)).is_none());
    }

    #[test]
    fn starting_a_session_sends_the_name_it_minted() {
        let session = serving(None);
        let id = session.start();

        let owed = owed(&session).expect("the browser is told the name");

        assert!(owed.starts_with(&format!("exos={id}")));
    }

    /// Otherwise every request would carry a header the browser already has.
    #[test]
    fn a_name_the_browser_sent_is_not_sent_back() {
        let arrived = Id::random();
        let session = serving(Some(arrived.clone()));

        assert_eq!(session.start(), arrived, "and starting is a no-op");
        assert!(owed(&session).is_none());
    }

    #[test]
    fn rotating_sends_the_new_name() {
        let session = serving(Some(Id::random()));
        let id = session.rotate();

        let owed = owed(&session).expect("the browser is told the new name");

        assert!(owed.starts_with(&format!("exos={id}")));
    }

    #[test]
    fn ending_takes_the_cookie_back() {
        let session = serving(Some(Id::random()));
        session.end();

        let owed = owed(&session).expect("the browser is told to drop it");

        assert!(owed.starts_with("exos=;"));
        assert!(owed.contains("Max-Age=0"));
    }

    /// Ending something that was never there is not a reason to send a header.
    #[test]
    fn ending_an_anonymous_session_owes_nothing() {
        let session = serving(None);
        session.end();

        assert!(owed(&session).is_none());
    }

    // ---- the cookie ---------------------------------------------------------

    #[test]
    fn two_names_are_never_the_same() {
        assert_ne!(Id::random(), Id::random());
        assert_eq!(Id::random().as_str().len(), ID_BYTES * 2);
    }

    #[test]
    fn the_name_is_read_out_of_whatever_the_browser_sent() {
        let id = Id::random();

        assert_eq!(
            arriving(&headers(&format!("exos={id}"))).as_ref(),
            Some(&id)
        );
        assert_eq!(
            arriving(&headers(&format!("theme=dark; exos={id}; a=1"))).as_ref(),
            Some(&id),
            "one header, several pairs"
        );
    }

    #[test]
    fn anything_not_shaped_like_a_name_is_no_name_at_all() {
        assert!(arriving(&HeaderMap::new()).is_none());
        assert!(arriving(&headers("theme=dark")).is_none());
        assert!(arriving(&headers("exos=")).is_none());
        assert!(arriving(&headers("exos=../../etc/passwd")).is_none());
        assert!(arriving(&headers("exosx=00")).is_none());

        let id = Id::random();
        assert!(
            Id::parse(&id.as_str().to_uppercase()).is_none(),
            "one name has one spelling, or two cookies would name one session"
        );
        assert!(Id::parse(&"a".repeat(4096)).is_none(), "any length at all");
    }

    #[test]
    fn the_cookie_is_locked_down_and_carries_only_the_name() {
        let issued = issued(&Id::random());
        let issued = issued.to_str().expect("a cookie is text");

        for attribute in ["HttpOnly", "Path=/", "SameSite=Lax", "Secure"] {
            assert!(issued.contains(attribute), "{issued}");
        }
    }

    /// The cookie must never be the thing that ends a session, because exos has
    /// no idea when the application's record does.
    #[test]
    fn the_cookie_outlasts_anything_an_application_would_keep() {
        let issued = issued(&Id::random());
        let issued = issued.to_str().expect("a cookie is text");

        // Four hundred days is the cap every browser applies, so this is the
        // longest a cookie can ask for and therefore says plainly that exos is
        // not the thing counting down.
        assert!(issued.contains(&format!("Max-Age={MAX_AGE}")));
        assert_eq!(MAX_AGE, 400 * 24 * 60 * 60);
    }
}
