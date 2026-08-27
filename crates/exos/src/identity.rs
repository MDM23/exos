//! Who a stream belongs to.
//!
//! A live fragment is addressed by what is on screen: the client reports the
//! topics it is displaying, proves each one, and a publish reaches whoever is
//! watching. That is the right shape for state, and it is why presence dots
//! need no bookkeeping. It cannot say *this person*, because delivery is
//! conditioned on a subscription the DOM derived.
//!
//! An audience is the other half. It is written onto the connection by the
//! server when the stream opens, from the session name the request carried, and
//! nothing the client sends can add one.
//! [`App::identify`](crate::App::identify) is where an application says what a
//! name stands for.
//!
//! # Why the server writes it and the client cannot
//!
//! A subscription is client-claimed and token-proved: the tab says what it is
//! showing and hands back the proof it was served. An audience is
//! server-derived and unforgeable, and the two are kept in separate sets on the
//! connection for exactly that reason. Merged, the difference between them
//! would depend on a token check nobody can see from the type, and the first
//! change that forgot it would let a tab name itself somebody else.
//!
//! # Why the resolver awaits, and why it can fail
//!
//! Because exos holds a session's name and nothing else, so turning a name into
//! whoever it stands for is a database call, and a database call can fail. It
//! runs once per connection rather than once per request, which is what makes
//! that affordable.
//!
//! A resolver that fails refuses the stream. Opening one with no audiences is
//! the silent version of the same failure, and a tab that quietly receives
//! nothing is worse than one that retries, which is what `EventSource` does on
//! its own.
//!
//! # Once per process
//!
//! [`App::identify`](crate::App::identify) goes once, the way the signing key
//! does. An application that never says it has no audiences, which is exactly
//! right for one that only publishes fragments: nothing is resolved and nothing
//! is paid for.

use core::{future::Future, hash::Hash, panic::Location, pin::Pin};
use std::{collections::HashSet, sync::OnceLock};

use crate::{Id, live::Topic};

// -----------------------------------------------------------------------------
//                                   AUDIENCES
// -----------------------------------------------------------------------------

/// Something a connection can be addressed as.
///
/// An implementor is any [`Hash`] type, so an audience is an ordinary value
/// rather than a string to spell: `Viewer(7)` and `Team(3)` are two audiences
/// the compiler keeps apart, and a typo in either is a compile error.
///
/// ```
/// # use exos::Audience;
/// #[derive(Hash)]
/// struct Team(u32);
///
/// impl Audience for Team {
///     const NAME: &'static str = "team";
/// }
/// ```
///
/// The trait is deliberately not sealed, because applications are the ones who
/// implement it. That makes a collision between two [`NAME`](Self::NAME)s
/// theirs to avoid, and the cost of getting it wrong is two types addressing
/// one another's connections.
pub trait Audience: Hash {
    /// What this kind of audience is called.
    ///
    /// It is hashed along with the value, so it is what keeps `Viewer(7)` and
    /// `Team(7)` apart when their fields agree.
    const NAME: &'static str;
}

/// Every audience one connection is addressed by.
///
/// A set rather than one value, because it costs nothing and it is the whole
/// difference between addressing a user and addressing every admin, everyone
/// on a team, or every tab in a workspace.
///
/// ```
/// # use exos::{Audience, Audiences};
/// # #[derive(Hash)]
/// # struct Viewer(u32);
/// # impl Audience for Viewer { const NAME: &'static str = "viewer"; }
/// # #[derive(Hash)]
/// # struct Team(u32);
/// # impl Audience for Team { const NAME: &'static str = "team"; }
/// let both = Audiences::of(&Viewer(7)).and(&Team(3));
/// ```
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[must_use = "an audience set does nothing until a delivery is addressed to it"]
pub struct Audiences(HashSet<String>);

impl Audiences {
    /// Nobody, which is what an unrecognised name resolves to.
    ///
    /// Not a failure. A visitor exos has never heard of has a stream and no
    /// audiences, and the fragments on their page still update.
    pub fn none() -> Self {
        Self::default()
    }

    /// The set holding `audience` alone.
    pub fn of<A: Audience>(audience: &A) -> Self {
        Self::none().and(audience)
    }

    /// The same set, also addressed as `audience`.
    pub fn and<A: Audience>(mut self, audience: &A) -> Self {
        self.0.insert(key(audience));
        self
    }

    /// The keys a connection is matched on.
    pub(crate) fn into_keys(self) -> HashSet<String> {
        self.0
    }
}

/// What an audience reduces to on a connection.
///
/// The same reduction a fragment topic gets, so there is one rule for how a
/// name and its arguments become a key rather than two that could drift. That
/// an audience and a topic could in principle produce the same string is
/// harmless and deliberately so: they are matched against separate sets, and
/// keeping those apart is what stops a client from claiming an audience.
pub(crate) fn key<A: Audience>(audience: &A) -> String {
    Topic::new(A::NAME, audience).as_str().to_owned()
}

// -----------------------------------------------------------------------------
//                                 THE RESOLVER
// -----------------------------------------------------------------------------

/// What a resolver answers with.
///
/// The error is boxed because exos never inspects one. Any error type an
/// application already has converts into it with `?`, and all exos does with
/// the result is refuse the stream and say why on stderr.
pub type Resolution = Result<Audiences, Box<dyn core::error::Error + Send + Sync>>;

/// Erased so it can be stored, since every async closure has its own type.
type Resolver =
    Box<dyn Fn(Option<Id>) -> Pin<Box<dyn Future<Output = Resolution> + Send>> + Send + Sync>;

/// The resolver, and where it was said, since no two closures can be compared.
static RESOLVER: OnceLock<(&'static Location<'static>, Resolver)> = OnceLock::new();

/// Says who a stream belongs to, for [`App::identify`](crate::App::identify).
///
/// # Panics
///
/// If a resolver was already said somewhere else, which means two parts of the
/// program disagree about who a connection is. The same place saying it again
/// is one application built twice and says nothing new.
pub(crate) fn set<F, Fut>(resolver: F, at: &'static Location<'static>)
where
    F: Fn(Option<Id>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Resolution> + Send + 'static,
{
    let erased: Resolver = Box::new(move |name| Box::pin(resolver(name)));
    let (said, _) = RESOLVER.get_or_init(|| (at, erased));

    assert!(
        *said == at,
        "a resolver is already in place, from {said}; App::identify goes once, \
         before anything is served"
    );
}

/// The audiences `name` stands for, or none at all if nothing was configured.
pub(crate) async fn resolve(name: Option<Id>) -> Resolution {
    match RESOLVER.get() {
        Some((_, resolver)) => resolver(name).await,
        None => Ok(Audiences::none()),
    }
}

// -----------------------------------------------------------------------------
//                                     TESTS
// -----------------------------------------------------------------------------

// Nothing here calls `identify`, which is process-global and can be set once.
// What it does is exercised in `tests/identity.rs`, where a test binary is a
// process of its own and the whole stack is in front of it.

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Hash)]
    struct Viewer(u32);

    impl Audience for Viewer {
        const NAME: &'static str = "viewer";
    }

    #[derive(Hash)]
    struct Team(u32);

    impl Audience for Team {
        const NAME: &'static str = "team";
    }

    #[test]
    fn one_audience_always_keys_the_same_way() {
        assert_eq!(key(&Viewer(7)), key(&Viewer(7)));
        assert_ne!(key(&Viewer(7)), key(&Viewer(8)));
    }

    /// Otherwise a notification for user 7 would reach team 7, which is the
    /// sort of thing that would work in every test and fail in production.
    #[test]
    fn two_kinds_of_audience_are_apart_even_when_their_fields_agree() {
        assert_ne!(key(&Viewer(7)), key(&Team(7)));
    }

    #[test]
    fn a_set_holds_every_audience_it_was_given() {
        let audiences = Audiences::of(&Viewer(7)).and(&Team(3)).into_keys();

        assert!(audiences.contains(&key(&Viewer(7))));
        assert!(audiences.contains(&key(&Team(3))));
        assert_eq!(audiences.len(), 2);
    }

    #[test]
    fn saying_the_same_audience_twice_says_it_once() {
        let audiences = Audiences::of(&Viewer(7)).and(&Viewer(7)).into_keys();

        assert_eq!(audiences.len(), 1);
    }

    #[test]
    fn nobody_is_a_set_with_nothing_in_it() {
        assert!(Audiences::none().into_keys().is_empty());
    }
}
