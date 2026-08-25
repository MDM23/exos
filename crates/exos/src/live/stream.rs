//! Who is watching what.
//!
//! One stream per browser tab carries everything. A tab says which fragments
//! it currently has on screen, and when one of those is published that tab,
//! and only that tab, gets the patch.
//!
//! The set is replaced on every update rather than added to, because the
//! client always knows its whole visible set and a diff would be one more
//! thing to get wrong. Navigating away unsubscribes by simply not mentioning
//! the topic again.
//!
//! # Who names a connection
//!
//! The server does, and it says so in the stream's first event. A client that
//! chose its own id could name somebody else's connection and replace the
//! topics that connection watches, which is a tab silently losing its updates
//! or, since a connection now carries identity, receiving another viewer's. An
//! unguessable id makes that a matter of arithmetic rather than of trust.
//!
//! The id names the node that minted it as well, in front of those unguessable
//! bits. A tab holds its stream to one node and sends every other request
//! wherever the load balancer points, so the node answering a subscription is
//! often not the node holding the connection, and the prefix is what tells a
//! connection that has *gone* from one that was never this node's. Only the
//! first is a browser that should reconnect; the second is a forward over the
//! [bus](crate::bus).
//!
//! # Two sets, and only one of them is the client's
//!
//! A connection holds what it watches and who it is, and they never meet. The
//! topics are client-claimed and token-proved, replaced wholesale by every
//! `/_exos/subscribe`. The [audiences](crate::identity) are server-derived,
//! written once when the stream opens, and nothing a client sends can reach
//! them.
//!
//! They hold the same kind of string, which is the reason to keep them in
//! separate fields rather than one: merged, whether a key was proved or derived
//! would depend on a check nobody can see from the type.

use core::time::Duration;
use std::{
    collections::{HashMap, HashSet},
    convert::Infallible,
    pin::Pin,
    sync::{Mutex, OnceLock, PoisonError},
    task::{Context, Poll},
};

use axum::{
    Json, Router,
    http::StatusCode,
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
    routing::{get, post},
};
use futures_core::Stream;
use serde::Deserialize;
use tokio::sync::broadcast;
use tokio_stream::{StreamExt as _, wrappers::BroadcastStream};

use crate::{
    Id, Step, hex,
    identity::{self, Audience},
    live::{
        Topic,
        bus::{self, Frame, Kind},
    },
};

/// Where the stream and the subscription endpoint are mounted.
///
/// Both start with the same segment an asset URL does, which is what lets the
/// runtime find the [base](crate::base) by looking at its own script URL rather
/// than being told.
const STREAM: &str = "/_exos/live";
const SUBSCRIBE: &str = "/_exos/subscribe";

/// The event the connection id arrives on.
///
/// Outside the [`Step`] vocabulary on purpose: this is the stream saying who it
/// is, not a change to apply to the document, and a name that could collide
/// with a step would make the two indistinguishable to the client's dispatch.
const HELLO: &str = "connection";

/// How far a slow tab may lag before it starts missing messages.
///
/// Missing them is the correct failure: a client that cannot keep up with a
/// live feed should skip ahead rather than stall the publisher.
const CAPACITY: usize = 64;

/// How long a browser waits before reopening a stream that dropped.
///
/// Sent on the greeting, so the interval is the same everywhere rather than
/// whatever each browser happens to default to. Three seconds is roughly what
/// they already pick, and a lost connection is usually a network that needs a
/// moment rather than one that needs asking again immediately.
///
/// A dev build waits half a second, because there a dropped stream is a
/// rebuild and this is most of the delay between saving a file and seeing it.
/// It is not a pause between two attempts either: a watcher kills the process
/// before it starts compiling, so this is how often a tab knocks on a closed
/// port for the length of a build. Shorter buys a fraction of a second and a
/// browser console full of refused connections, which is where the error the
/// developer was actually reading used to be.
const RETRY: Duration = if cfg!(debug_assertions) {
    Duration::from_millis(500)
} else {
    Duration::from_secs(3)
};

/// How long it waits after a stream the server ended on purpose.
///
/// A rotation ends a browser's streams so that they come back as whoever it is
/// now, and every millisecond of that is a tab still showing the old name. The
/// reconnect is expected rather than a symptom, so there is nothing to wait
/// for: this is long enough to be a reconnect and short enough not to be seen.
///
/// It rides out on the last event before the channel closes, and the greeting
/// on the new connection puts [`RETRY`] back, so the impatience lasts exactly
/// one reconnect and a browser that later loses its network still waits.
///
/// The window where that is not true is a server going down in the moment
/// between ending a stream and greeting it again: those tabs would then ask
/// every 150ms rather than every three seconds until it came back. It is a
/// narrow window and a handful of tabs, and the alternative is every rotation
/// paying three seconds of showing the wrong name.
const RETRY_WHEN_RENAMED: Duration = Duration::from_millis(150);

/// A live connection: its outbound channel, who it is, and what it watches.
struct Connection {
    /// Who the server decided this connection belongs to when it opened.
    ///
    /// Never written again, and never from a client request. The module docs
    /// say why this is its own field rather than part of `topics`.
    audiences: HashSet<String>,
    /// What this connection is called on a bus.
    ///
    /// The reduction of its id, held beside it so that a frame naming one
    /// connection is matched the way every other frame is: against a set of
    /// keys rather than against a bearer name.
    key: String,
    sender: broadcast::Sender<Event>,
    /// The session name this stream opened under, so that a rotation can find
    /// it again.
    ///
    /// `None` is a stream opened by a browser carrying no cookie at all. It
    /// stays `None` and is never matched by anything, because a nameless
    /// connection belongs to no browser in particular and treating them as a
    /// group would treat every anonymous visitor as one person.
    session: Option<Id>,
    topics: HashSet<String>,
}

type Connections = Mutex<HashMap<String, Connection>>;

fn connections() -> &'static Connections {
    static CONNECTIONS: OnceLock<Connections> = OnceLock::new();
    CONNECTIONS.get_or_init(Connections::default)
}

/// How many streams are open. Useful in a health endpoint, and in tests.
///
/// # Panics
///
/// If the registry lock was poisoned by a panic in another thread while it was
/// held. Nothing here can panic while holding it.
pub fn connection_count() -> usize {
    connections()
        .lock()
        .expect("the registry lock is never held across a panic")
        .len()
}

/// Whether any open stream belongs to `audience`.
///
/// The question a sender asks before choosing between a push and an email. It
/// is a hint and never a guarantee: the last tab can close between the answer
/// and whatever is done about it, so the honest use is to persist first and
/// treat reaching somebody as an accelerator.
///
/// ```
/// # use exos::{Audience, connected};
/// # #[derive(Hash)]
/// # struct Viewer(u32);
/// # impl Audience for Viewer { const NAME: &'static str = "viewer"; }
/// assert!(!connected(&Viewer(7)), "nobody has opened a stream");
/// ```
///
/// # Panics
///
/// If the registry lock was poisoned; see [`connection_count`].
pub fn connected<A: Audience>(audience: &A) -> bool {
    let key = identity::key(audience);

    connections()
        .lock()
        .expect("the registry lock is never held across a panic")
        .values()
        .any(|connection| connection.audiences.contains(&key))
}

/// Sends `effect` to every open stream that belongs to `audience`.
///
/// The other half of [`publish`], and the two are addressed differently on
/// purpose. A fragment is addressed by what is on screen and reaches whoever
/// is watching it. A directed effect is addressed by who a connection is, so
/// it reaches every tab that person has open regardless of what they are
/// looking at, which is what a notification needs and what a subscription
/// derived from the DOM cannot express.
///
/// ```
/// # use exos::{Audience, Effect};
/// # use serde::{Deserialize, Serialize};
/// #[exos::model]
/// #[derive(Debug, Default, Deserialize, Serialize)]
/// struct Toast {
///     message: String,
/// }
///
/// #[derive(Hash)]
/// struct Viewer(u32);
///
/// impl Audience for Viewer {
///     const NAME: &'static str = "viewer";
/// }
///
/// # let user = 7;
/// # let summary = String::from("Ada mentioned you in Q3 planning");
/// exos::send(&Viewer(user), &Effect::set(&Toast::signals().message, summary));
/// ```
///
/// Every step becomes one event, exactly as a publish sends one, so the eight
/// things an [`Effect`](crate::Effect) can say are the eight things this can
/// say. Sending to an audience nobody is connected as is free and silent.
///
/// # It accelerates state, it does not record it
///
/// A patch is state replacement and the next publish repairs a lost one. A
/// directed effect has no fragment to re-render from, so a recipient who is
/// offline, whose tab lagged past the channel's capacity, or who was inside a
/// reconnect gap simply does not get it, and none of the three is fixable by
/// trying harder. Persist first and push second: the record is what the next
/// page load renders, and this is what saves the recipient from waiting for
/// one.
///
/// # Authorizing is the caller's
///
/// A subscription is authorized by construction, since a topic can only be
/// subscribed to by whoever was served it. This inverts that: the server names
/// the recipient, so exos guarantees that only connections whose identity
/// matched receive it and nothing whatever about whether that person should
/// see the content. That check belongs at the call site, in ordinary Rust,
/// where it can be read.
///
/// # Order
///
/// Guaranteed per connection and nowhere else. A connection has one channel
/// and both this and [`publish`] send under the registry lock, so a publish
/// followed by a send arrives in that order at every tab that gets both. Two
/// connections are ordered against each other in no way at all.
///
/// With a [`bus`](crate::bus) registered that holds for a tab on this node and
/// not for one on another: a publish and a send are two keys and therefore two
/// frames, and nothing orders them against each other on the way across.
///
/// # Panics
///
/// If the registry lock was poisoned; see [`connection_count`].
pub fn send<A: Audience>(audience: &A, effect: &crate::Effect) {
    // Framed once rather than per connection, since every recipient gets the
    // same bytes and a fan-out is the shape this is for.
    let steps: Vec<(String, String)> = effect.steps().iter().map(Step::framed).collect();

    if steps.is_empty() {
        return;
    }

    let key = identity::key(audience);

    dispatch(Kind::Audience, &key, &steps);
    bus::cross(|| Frame::delivery(Kind::Audience, key, steps));
}

/// Renders a fragment and sends it to every connection watching it.
///
/// Publishing a fragment nobody is looking at is free and silent, which is
/// what lets a handler publish unconditionally rather than asking first.
///
/// ```ignore
/// publish(|| presence(user.id));
/// ```
///
/// # Why it renders rather than taking a rendered one
///
/// Because a patch is state replacement, so what has to be true is that the
/// last patch a tab receives is the newest one, and that is a fact about the
/// order of two things rather than about either of them.
///
/// This used to take a `&Fragment`, which meant the caller rendered and then
/// asked to send. Two of those racing is enough to leave a tab wrong forever: a
/// publisher that read the state first can reach the lock second, so the older
/// markup lands last and stays until something publishes that topic again,
/// which for the last write of the day is never. It is invisible in a test, it
/// needs no unusual load, and the symptom is a price or a status that is simply
/// out of date on one screen.
///
/// Taking the render closes it, because the read and the send then happen under
/// one lock. A publisher may still find that another has already sent what it
/// was about to, and that is harmless: both read current state, so whichever
/// goes last is current. The type is what enforces it, since a `Fragment`
/// rendered beforehand can no longer be handed to this.
///
/// That is a fact about serializing two operations in one process. With a
/// [`bus`](crate::bus) registered it holds for the tabs on this node, and two
/// nodes publishing the same topic arrive at a third in whatever order the
/// broker gives.
///
/// # What it costs
///
/// Publishes are serialized against each other, so an expensive fragment holds
/// up an unrelated one. That is the same shape as the registry walk this
/// already does and is fine at the volumes it is fine at; a lock per topic is
/// the refinement, and belongs with the index that would make the walk a
/// lookup.
///
/// # Panics
///
/// If the registry lock was poisoned; see [`connection_count`]. A panic in
/// `render` propagates, and does not poison anything: the ordering lock guards
/// no data, so a fragment that panics costs its own publish and nothing else.
pub fn publish(render: impl FnOnce() -> crate::Fragment) {
    // Held across the render as well as the send, which is the whole point.
    // Recovered rather than propagated on poisoning, because `()` has no
    // invariant a panicking render could have broken, and because that panic
    // is the caller's rather than this module's.
    let _order = order().lock().unwrap_or_else(PoisonError::into_inner);

    let fragment = render();
    let steps = vec![Step::Patch(fragment.to_markup()).framed()];
    let topic = fragment.topic().as_str();

    dispatch(Kind::Topic, topic, &steps);

    // Local first, and the frame carries the rendered patch rather than a
    // request to render one: a topic is a hash of a name and its arguments, so
    // no node can invoke the function from it. Rendering once for the whole
    // cluster is the only shape available and is also the cheaper one.
    bus::cross(|| Frame::delivery(Kind::Topic, topic, steps));
}

/// Replaces what the connection `key` names is watching.
///
/// The other half of a forwarded subscription, on the node that holds the
/// socket. The names arrive proved, since the node that had the cookie checked
/// them, and a frame naming a connection this node does not hold reaches
/// nothing, which is every node but one.
pub(crate) fn resubscribe(key: &str, topics: &[String]) {
    let mut registry = connections()
        .lock()
        .expect("the registry lock is never held across a panic");

    for connection in registry.values_mut() {
        if connection.key == key {
            connection.topics = topics.iter().cloned().collect();
        }
    }
}

/// Pushes one delivery at every connection watching `key`.
///
/// The one walk, whether the delivery was made here or arrived from another
/// node: a frame is keyed by what the registry is already keyed by, so
/// [`deliver`](crate::deliver) is this function and nothing else.
pub(crate) fn dispatch(kind: Kind, key: &str, steps: &[(String, String)]) {
    // Framed once rather than per connection, since every recipient gets the
    // same bytes and a fan-out is the shape this is for.
    let events: Vec<Event> = steps
        .iter()
        .map(|(name, data)| Event::default().event(name.clone()).data(data.clone()))
        .collect();

    let registry = connections()
        .lock()
        .expect("the registry lock is never held across a panic");

    for connection in registry.values() {
        let watching = match kind {
            Kind::Topic => connection.topics.contains(key),
            Kind::Audience => connection.audiences.contains(key),
            // Not a delivery. A frame naming one connection says what it is
            // watching, and [`deliver`](crate::deliver) hands that to
            // [`resubscribe`] instead of here.
            Kind::Connection => false,
        };

        if watching {
            for event in &events {
                // A closed receiver is a tab that went away between the check
                // and this send; the cleanup path removes it.
                drop(connection.sender.send(event.clone()));
            }
        }
    }
}

/// Orders a publish against every other publish, render included.
///
/// Its own lock rather than the registry's, so that arbitrary rendering never
/// runs while the registry is held and the claim every `expect` in this file
/// makes about that lock stays true.
fn order() -> &'static Mutex<()> {
    static ORDER: OnceLock<Mutex<()>> = OnceLock::new();
    ORDER.get_or_init(Mutex::default)
}

/// What this process calls itself, for the length of the process.
///
/// Random rather than configured, because nothing addresses a node by it: it
/// answers one local question, "did I mint this id?", and a forward is fanned
/// out over the bus like everything else. So there is no directory, nothing to
/// configure, and no name that says which machine this is.
///
/// It is in the connection id in the clear, which tells a client only that
/// nodes exist and how many it has been served by. That was an open question
/// between a plain name and a hashed one, and a random name is already the
/// hashed one: it identifies nothing outside this process.
fn node() -> &'static str {
    static NODE: OnceLock<String> = OnceLock::new();

    NODE.get_or_init(|| {
        let mut bytes = [0_u8; 8];

        getrandom::fill(&mut bytes).expect("the operating system provides entropy for a node name");

        hex::encode(&bytes)
    })
}

/// A name no other connection has and no client can guess.
///
/// 128 bits from the operating system, as hex, behind the name of the node
/// that minted it. Guessability is the whole property of the second half: the
/// id is a bearer name for a connection, so anybody holding one can replace
/// what that connection watches. The first half is what lets a node tell a
/// connection that has gone from one that was never its own.
///
/// # Panics
///
/// If the operating system has no entropy to give, which is where
/// [`Keys::random`](crate::Keys::random) already stands and has the same
/// answer: a guessable id is worse than not starting.
fn mint() -> String {
    let mut bytes = [0_u8; 16];

    getrandom::fill(&mut bytes).expect("the operating system provides entropy for a connection id");

    format!("{}-{}", node(), hex::encode(&bytes))
}

/// Whether this node is the one that minted `id`.
///
/// The whole of what the prefix is for. Mine and unknown is a connection that
/// has gone, which the browser is told so that it opens a fresh stream; not
/// mine is a request that landed on the wrong node, which is a forward.
fn minted_here(id: &str) -> bool {
    id.split_once('-')
        .is_some_and(|(prefix, _)| prefix == node())
}

/// What a frame naming one connection is keyed by.
///
/// The same reduction an audience gets, for the same reason: a connection id
/// is a bearer name, so what crosses a bus is what it reduces to and never the
/// id itself.
fn reduction(id: &str) -> String {
    Topic::new("connection", &id).as_str().to_owned()
}

/// Registers a connection under a fresh id, with the receiver its response
/// drains.
///
/// The audiences arrive here rather than being written afterwards, so there is
/// no moment where a connection is reachable and does not yet know who it is.
fn open(session: Option<Id>, audiences: HashSet<String>) -> (String, broadcast::Receiver<Event>) {
    let id = mint();
    let (sender, receiver) = broadcast::channel(CAPACITY);

    connections()
        .lock()
        .expect("the registry lock is never held across a panic")
        .insert(
            id.clone(),
            Connection {
                audiences,
                key: reduction(&id),
                sender,
                session,
                topics: HashSet::new(),
            },
        );

    (id, receiver)
}

/// Ends every stream that opened under `name`, and says how many.
///
/// What a rotation does to the browser it renamed. A session name that has
/// been replaced or taken away leaves that browser's streams identified as
/// somebody who no longer exists, and there is no way to correct one in place:
/// re-resolving it would carry a connection across the boundary rotation
/// exists to draw, so a stolen cookie with a stream open would be upgraded to
/// the new identity rather than cut off by it.
///
/// Ending them is what rotation is for, and the client needs nothing new to
/// cope. `EventSource` reconnects on its own, carrying whatever cookie the
/// browser holds by then, and a greeting that is not the first makes the
/// runtime re-fetch the page, so the tab ends up correctly identified and
/// showing the right markup with no reload.
///
/// A dev build does reload, because there a reconnect means the binary was
/// rebuilt and only a fresh document picks that up; see
/// [`runtime`](crate::runtime). So a rotation costs a reload in development
/// that it does not cost in production, which is the cheaper half of the
/// trade.
///
/// A connection that opened under no name is never matched, whatever `name`
/// is: see [`Connection::session`].
///
/// # Panics
///
/// If the registry lock was poisoned; see [`connection_count`].
pub(crate) fn disconnect(name: &Id) -> usize {
    let mut registry = connections()
        .lock()
        .expect("the registry lock is never held across a panic");

    let before = registry.len();

    registry.retain(|_, connection| {
        if connection.session.as_ref() != Some(name) {
            return true;
        }

        // Said on the way out, because a browser that is not told waits the
        // seconds a broken network deserves, and this is not one: the stream
        // is being ended so it can come back as somebody else. A receiver
        // drains what is already buffered before it sees the end, so this
        // arrives.
        drop(
            connection
                .sender
                .send(Event::default().retry(RETRY_WHEN_RENAMED)),
        );

        // Dropping the entry drops the only sender, which closes the channel
        // and ends the response body the browser is reading.
        false
    });

    before - registry.len()
}

/// Forgets a connection when its stream ends.
fn close(id: &str) {
    if let Ok(mut registry) = connections().lock() {
        registry.remove(id);
    }
}

/// What a browser sends to say what it is displaying.
#[derive(Debug, Deserialize)]
struct Subscription {
    connection: String,
    /// Topic and token pairs. The token is what proves the server rendered
    /// this fragment for this client rather than the client guessing an id.
    topics: Vec<(String, String)>,
}

/// Says what a browser is displaying, wherever the request landed.
///
/// A tab holds its stream to one node and sends this wherever the load
/// balancer points, so the node answering is often not the node holding the
/// connection. The tokens are checked here regardless, because a token is an
/// HMAC under the shared key bound to the session in the cookie this request
/// carried, and this is the node that has the cookie. What crosses afterwards
/// is the proved names, so nothing is verified twice and no token reaches a
/// broker.
///
/// The three answers, and the middle one is the whole of why a connection id
/// carries the node that minted it:
///
/// * held here, so applied here, which is every request in a single process
/// * minted here and gone, so `410`, and the browser opens a fresh stream
/// * minted elsewhere, so forwarded, and `204` because it is on its way
///
/// Without a bus there is nowhere to forward to and the last case is the
/// second: a single node that has never heard of an id is a browser that
/// should reconnect.
async fn subscribe(Json(request): Json<Subscription>) -> StatusCode {
    // An unverifiable topic is dropped rather than failing the whole request:
    // one stale fragment left over from a previous page should not cost a tab
    // its other subscriptions.
    let proved: Vec<String> = request
        .topics
        .into_iter()
        .filter(|(topic, token)| Topic::from_raw(topic).verify(token))
        .map(|(topic, _)| topic)
        .collect();

    let Ok(mut registry) = connections().lock() else {
        return StatusCode::INTERNAL_SERVER_ERROR;
    };

    if let Some(connection) = registry.get_mut(&request.connection) {
        connection.topics = proved.into_iter().collect();

        return StatusCode::NO_CONTENT;
    }

    // Nothing here reads the registry again, and a forward is a spawn, so the
    // lock is let go before either.
    drop(registry);

    if minted_here(&request.connection) || !bus::registered() {
        // The stream died, or the id was invented. Either way the client
        // should reconnect rather than have a connection conjured for it.
        return StatusCode::GONE;
    }

    bus::cross(|| Frame::subscription(reduction(&request.connection), proved));

    StatusCode::NO_CONTENT
}

/// Opens the one stream a tab has, having first settled who it belongs to.
///
/// The identity is resolved before the connection is registered, which is what
/// makes a stream that carries no identity impossible rather than merely
/// unlikely. Reading the session is all this does with it: a stream must never
/// *start* one, because its response headers go out here and there would be no
/// second chance to set the cookie.
async fn stream() -> Response {
    // Kept as well as resolved, because a rotation has to be able to find the
    // streams a name opened, and audiences cannot be worked backwards.
    let session = crate::session().id();

    let audiences = match identity::resolve(session.clone()).await {
        Ok(audiences) => audiences.into_keys(),
        Err(error) => {
            // Refusing is the loud version of what opening anyway would do
            // silently. `EventSource` retries on its own, so a resolver that
            // fails because a database blinked costs a delay rather than a tab.
            eprintln!("exos: a stream was refused because identifying it failed: {error}");

            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let (id, receiver) = open(session, audiences);

    let events = BroadcastStream::new(receiver)
        // A lagged tab skips ahead rather than stalling the publisher; the
        // next publish of anything it watches brings it back in line.
        .filter_map(|event| event.ok().map(Ok::<Event, Infallible>));

    Sse::new(Disconnect {
        greeting: Some(Event::default().retry(RETRY).event(HELLO).data(&id)),
        id,
        events,
    })
    .keep_alive(KeepAlive::default())
    .into_response()
}

/// A stream that introduces its connection and then deregisters it when the
/// browser goes away.
///
/// The greeting rides here rather than being pushed through the channel so that
/// it cannot be dropped by the lag filter above, and so that it is first by
/// construction rather than by a race with the first publish. Without the
/// deregistration the registry grows by one entry per tab ever opened, and
/// every publish walks them all.
struct Disconnect<S> {
    greeting: Option<Event>,
    id: String,
    events: S,
}

impl<S> Stream for Disconnect<S>
where
    S: Stream<Item = Result<Event, Infallible>> + Unpin,
{
    type Item = S::Item;

    fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<S::Item>> {
        if let Some(greeting) = self.greeting.take() {
            return Poll::Ready(Some(Ok(greeting)));
        }

        Pin::new(&mut self.events).poll_next(context)
    }
}

impl<S> Drop for Disconnect<S> {
    fn drop(&mut self) {
        close(&self.id);
    }
}

/// Mounts the stream and the subscription endpoint.
///
/// At the root of whatever router these end up in. Nesting is what puts an
/// application under a prefix, and the client finds that prefix for itself.
pub(crate) fn routes() -> Router {
    Router::new()
        .route(STREAM, get(stream))
        .route(SUBSCRIBE, post(subscribe))
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "a failing assertion is the point of a test"
)]
mod tests {
    use axum::{
        body::Body,
        http::{Request, header},
    };
    use tower::ServiceExt as _;

    use super::*;
    use crate::Markup;

    /// The routes with the layers [`app`](crate::app) puts around them.
    ///
    /// The stream reads the session, which lives in the request scope, so the
    /// bare router is not a thing that can answer. Mounting the same two layers
    /// here means these tests exercise the composition rather than a handler
    /// lifted out of it.
    fn served() -> Router {
        routes()
            .layer(axum::middleware::from_fn(crate::session::layer))
            .layer(axum::middleware::from_fn(crate::scope::layer))
    }

    /// What one browser was served: the name it was given, and a token for
    /// each topic it was shown.
    ///
    /// Minting happens in a request, because a token is bound to whoever the
    /// request was for, and the name has to reach the next request exactly as
    /// a cookie carries it. There is deliberately no shortcut: a token a test
    /// could mint for a session it is not holding is one a client could mint
    /// too.
    fn browser(topics: &[&Topic]) -> (Id, Vec<(String, String)>) {
        crate::with_scope(|| {
            // Named first, so that a browser which has been shown no fragment
            // at all is still a browser rather than a panic.
            let name = crate::session().start();

            let watching = topics
                .iter()
                .map(|topic| {
                    let token = topic.token().expect("a request has a viewer to bind to");

                    (topic.as_str().to_owned(), token)
                })
                .collect();

            (name, watching)
        })
    }

    /// Says what a browser is displaying, the way the runtime does after a
    /// mutation: one request, carrying the cookie the tokens were minted for.
    async fn subscribing(name: &Id, connection: &str, topics: &[(String, String)]) -> StatusCode {
        let body = serde_json::json!({ "connection": connection, "topics": topics }).to_string();

        served()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(SUBSCRIBE)
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(header::COOKIE, format!("exos={name}"))
                    .body(Body::from(body))
                    .expect("a valid request"),
            )
            .await
            .expect("the router answers")
            .status()
    }

    /// Opens a real stream through the router and reads what it says first,
    /// which is the wire format the client parses rather than a stand-in for
    /// it. The body is handed back because dropping it closes the connection.
    async fn greeted() -> (String, Body) {
        let response = served()
            .oneshot(
                Request::builder()
                    .uri(STREAM)
                    .body(Body::empty())
                    .expect("a valid request"),
            )
            .await
            .expect("the router answers");

        assert_eq!(response.status(), StatusCode::OK);

        let mut body = response.into_body().into_data_stream();

        let first = body
            .next()
            .await
            .expect("the stream says something")
            .expect("the body does not fail");

        let text = String::from_utf8(first.to_vec()).expect("the greeting is text");

        assert!(text.contains("event: connection"), "{text:?}");

        // Read line by line rather than off the front, because the greeting
        // carries the reconnection time as well as the name.
        let id = text
            .lines()
            .find_map(|line| line.strip_prefix("data: "))
            .unwrap_or_else(|| panic!("the greeting names the connection, got {text:?}"))
            .to_owned();

        (id, Body::new(body))
    }

    #[tokio::test]
    async fn the_stream_names_the_connection_before_anything_else() {
        let (id, _body) = greeted().await;
        let (prefix, random) = id.split_once('-').expect("the node, then the name");

        assert_eq!(prefix, node(), "the node that minted it");
        assert_eq!(random.len(), 32, "128 bits as hex");
        assert!(
            random
                .chars()
                .all(|character| character.is_ascii_hexdigit())
        );
    }

    /// The whole of what the prefix is for: a node can tell a connection that
    /// has gone from one it never had, and only the first is a browser that
    /// should reconnect.
    #[test]
    fn a_node_knows_which_connections_it_minted() {
        assert!(minted_here(&mint()));

        assert!(!minted_here("0123456789abcdef-c0ffee"), "another node's");
        assert!(
            !minted_here("invented"),
            "and one that names no node at all"
        );
    }

    /// The point of the whole handshake: the only id that works is one the
    /// server handed out, so a client cannot name a connection it was not given.
    #[tokio::test]
    async fn the_id_the_server_gave_is_the_one_that_subscribes() {
        let (id, _body) = greeted().await;

        let topic = Topic::new("presence", &(1_u32,));
        let (name, watching) = browser(&[&topic]);

        assert_eq!(
            subscribing(&name, &id, &watching).await,
            StatusCode::NO_CONTENT
        );

        let registry = connections().lock().expect("the lock is not poisoned");
        assert!(
            registry
                .get(&id)
                .expect("the connection is open")
                .topics
                .contains(topic.as_str())
        );
    }

    #[tokio::test]
    async fn two_streams_are_never_named_the_same() {
        let (first, _first) = greeted().await;
        let (second, _second) = greeted().await;

        assert_ne!(first, second);
    }

    #[tokio::test]
    async fn subscribing_keeps_only_the_topics_it_can_prove() {
        let (id, _receiver) = open(None, HashSet::new());

        let real = Topic::new("presence", &(42_u32,));
        let forged = Topic::new("presence", &(43_u32,));

        let (name, mut watching) = browser(&[&real]);
        watching.push((forged.as_str().to_owned(), String::from("0000000000000000")));

        assert_eq!(
            subscribing(&name, &id, &watching).await,
            StatusCode::NO_CONTENT
        );

        let registry = connections().lock().expect("the lock is not poisoned");
        let topics = &registry.get(&id).expect("the connection is open").topics;

        assert!(topics.contains(real.as_str()));
        assert!(
            !topics.contains(forged.as_str()),
            "the unproven topic is dropped and the rest of the request still applies"
        );
    }

    /// A token proves the browser presenting it was served the fragment, so a
    /// pair lifted out of somebody else's page, by a screenshot or a shared
    /// profile, subscribes to nothing at all.
    #[tokio::test]
    async fn a_topic_somebody_else_was_served_is_dropped() {
        let (id, _receiver) = open(None, HashSet::new());

        let topic = Topic::new("presence", &(44_u32,));
        let (_theirs, stolen) = browser(&[&topic]);
        let (mine, _nothing) = browser(&[]);

        assert_eq!(
            subscribing(&mine, &id, &stolen).await,
            StatusCode::NO_CONTENT
        );

        let registry = connections().lock().expect("the lock is not poisoned");
        assert!(
            !registry
                .get(&id)
                .expect("the connection is open")
                .topics
                .contains(topic.as_str())
        );
    }

    /// A guessed id is indistinguishable from a stream that has since dropped,
    /// and both get the same answer rather than a connection conjured for them.
    #[tokio::test]
    async fn an_unknown_connection_is_told_to_reconnect() {
        let (name, watching) = browser(&[]);

        assert_eq!(
            subscribing(&name, "never-opened", &watching).await,
            StatusCode::GONE
        );
    }

    // ---- who a connection is ------------------------------------------------

    /// The registry is process-global and these run in parallel, so each test
    /// below owns a number nothing else uses. That is the discipline
    /// [`provide`](crate::provide) asks of its callers, for the same reason.
    #[derive(Hash)]
    struct Viewer(u32);

    impl Audience for Viewer {
        const NAME: &'static str = "viewer";
    }

    #[tokio::test]
    async fn a_connection_is_addressable_as_whoever_opened_it() {
        let (_id, _receiver) = open(None, identity::Audiences::of(&Viewer(1)).into_keys());

        assert!(connected(&Viewer(1)));
        assert!(!connected(&Viewer(2)), "and as nobody else");
    }

    /// The whole reason the two sets are separate fields. A tab can say what it
    /// is displaying, and saying it in the shape of an audience key must not be
    /// a way to become somebody: the topic is proved, the audience is derived,
    /// and nothing arriving from a client reaches the second.
    #[tokio::test]
    async fn a_client_cannot_talk_its_way_into_an_audience() {
        let (id, _receiver) = open(None, HashSet::new());

        // The exact string the server would have written had it decided this
        // connection was viewer 3, handed back as a topic with a real token.
        let claimed = identity::key(&Viewer(3));
        let topic = Topic::from_raw(&claimed);
        let (name, watching) = browser(&[&topic]);

        assert_eq!(
            subscribing(&name, &id, &watching).await,
            StatusCode::NO_CONTENT
        );
        assert!(!connected(&Viewer(3)));

        let registry = connections().lock().expect("the lock is not poisoned");
        let connection = registry.get(&id).expect("the connection is open");

        assert!(connection.topics.contains(&claimed), "it is watching it");
        assert!(connection.audiences.is_empty(), "and is still nobody");
    }

    /// Identity lasts exactly as long as the stream carrying it, which is what
    /// makes `connected` a hint about now rather than a record of who has ever
    /// visited.
    #[tokio::test]
    async fn nobody_is_connected_once_the_stream_is_gone() {
        let audiences = identity::Audiences::of(&Viewer(4)).into_keys();

        {
            let (id, _receiver) = open(None, audiences);
            assert!(connected(&Viewer(4)));

            close(&id);
        }

        assert!(!connected(&Viewer(4)));
    }

    // ---- sending to a person ------------------------------------------------

    /// How many events are waiting. `sse::Event` cannot be read back, so a unit
    /// test counts and [`tests/directed.rs`](../../tests/directed.rs) reads the
    /// wire, where a real stream frames it.
    fn received(receiver: &mut broadcast::Receiver<Event>) -> usize {
        let mut count = 0;

        while receiver.try_recv().is_ok() {
            count += 1;
        }

        count
    }

    /// Every tab, which is the answer to the open question: a toast in six tabs
    /// is six toasts, and the page is where the decision to show one belongs.
    #[tokio::test]
    async fn an_effect_reaches_every_stream_in_its_audience_and_no_other() {
        let audience = identity::Audiences::of(&Viewer(5)).into_keys();

        let (_id, mut tab) = open(None, audience.clone());
        let (_id, mut other_tab) = open(None, audience);
        let (_id, mut somebody_else) = open(None, identity::Audiences::of(&Viewer(6)).into_keys());

        send(&Viewer(5), &crate::Effect::reload());

        assert_eq!(received(&mut tab), 1);
        assert_eq!(received(&mut other_tab), 1, "the same person's other tab");
        assert_eq!(received(&mut somebody_else), 0);
    }

    #[tokio::test]
    async fn every_step_becomes_one_event() {
        let (_id, mut tab) = open(None, identity::Audiences::of(&Viewer(7)).into_keys());

        let effect = crate::Effect::patch(Markup(String::from("<p id=\"x\"></p>")))
            .focus("#x")
            .scroll("#x");

        send(&Viewer(7), &effect);

        assert_eq!(received(&mut tab), effect.steps().len());
    }

    /// Sending to whoever is not there is the ordinary case for a notification,
    /// so it costs nothing and says nothing.
    #[tokio::test]
    async fn sending_nothing_or_to_nobody_is_silent() {
        let (_id, mut tab) = open(None, identity::Audiences::of(&Viewer(8)).into_keys());

        send(&Viewer(8), &crate::Effect::none());
        send(&Viewer(9), &crate::Effect::reload());

        assert_eq!(received(&mut tab), 0);
    }

    /// What makes the last patch a tab receives the newest one.
    ///
    /// The behaviour is asserted over real streams in
    /// [`tests/directed.rs`](../../tests/directed.rs); this pins the mechanism,
    /// because the mechanism is the only reason the behaviour holds and it is
    /// invisible from outside. A render that runs before the lock is a render
    /// whose result can be overtaken.
    #[tokio::test]
    async fn a_fragment_is_rendered_while_the_order_is_held() {
        let mut rendered = false;

        publish(|| {
            assert!(
                order().try_lock().is_err(),
                "the render has to happen inside the lock, or a publisher that \
                 read the state first can still send second"
            );

            rendered = true;
            crate::Fragment::new(Topic::new("order", &(1_u32,)), Markup::default())
        });

        assert!(rendered, "and the closure is what produced the fragment");
        assert!(order().try_lock().is_ok(), "and the lock is let go after");
    }

    /// The other direction of the rule that keeps the two sets apart. Watching
    /// a topic that spells an audience exactly is watching a topic, and a
    /// directed effect does not follow it, even though a publish of the same
    /// key does.
    #[tokio::test]
    async fn a_topic_is_not_a_way_into_an_audience() {
        let (id, mut receiver) = open(None, HashSet::new());

        let claimed = identity::key(&Viewer(10));
        let topic = Topic::from_raw(&claimed);
        let (name, watching) = browser(&[&topic]);

        assert_eq!(
            subscribing(&name, &id, &watching).await,
            StatusCode::NO_CONTENT
        );

        send(&Viewer(10), &crate::Effect::reload());
        assert_eq!(received(&mut receiver), 0);

        // And the control, so the silence above is the rule rather than a tab
        // that was never going to receive anything.
        publish(|| crate::Fragment::new(topic, Markup::default()));
        assert_eq!(received(&mut receiver), 1);
    }

    // ---- a delivery from another node ---------------------------------------

    /// One step, framed the way a frame carries them.
    fn crossed() -> Vec<(String, String)> {
        vec![(String::from("patch"), String::from("<p id=\"x\"></p>"))]
    }

    /// A frame is keyed by what the registry is already keyed by, so a
    /// delivery from elsewhere reaches exactly what a local publish would.
    #[tokio::test]
    async fn a_frame_reaches_the_connection_watching_its_key() {
        let (id, mut watching) = open(None, HashSet::new());
        let (_id, mut elsewhere) = open(None, HashSet::new());

        let topic = Topic::new("presence", &(20_u32,));
        let (name, proof) = browser(&[&topic]);

        assert_eq!(
            subscribing(&name, &id, &proof).await,
            StatusCode::NO_CONTENT
        );

        crate::deliver(Frame::delivery(Kind::Topic, topic.as_str(), crossed()));

        assert_eq!(received(&mut watching), 1);
        assert_eq!(received(&mut elsewhere), 0, "and nothing else at all");
    }

    /// The separation the two sets exist for, one level out. A key that spells
    /// an audience exactly reaches the person when it arrives as an audience
    /// and the tab watching it when it arrives as a topic, and never the other
    /// way round, so a frame cannot address a tab as somebody.
    #[tokio::test]
    async fn a_frame_reaches_the_set_its_kind_names_and_no_other() {
        let key = identity::key(&Viewer(21));
        let (_id, mut person) = open(None, identity::Audiences::of(&Viewer(21)).into_keys());
        let (id, mut tab) = open(None, HashSet::new());

        let topic = Topic::from_raw(&key);
        let (name, proof) = browser(&[&topic]);

        assert_eq!(
            subscribing(&name, &id, &proof).await,
            StatusCode::NO_CONTENT
        );

        crate::deliver(Frame::delivery(Kind::Audience, key.clone(), crossed()));

        assert_eq!(received(&mut person), 1);
        assert_eq!(received(&mut tab), 0, "watching it is not being it");

        crate::deliver(Frame::delivery(Kind::Topic, key, crossed()));

        assert_eq!(received(&mut tab), 1);
        assert_eq!(received(&mut person), 0, "and being it is not watching it");
    }

    /// A subscription that landed on the wrong node is applied by the node
    /// holding the connection, and what proves it is that a publish then
    /// reaches that tab. Nothing is verified here: the node that had the
    /// cookie proved the names before they crossed.
    #[tokio::test]
    async fn a_forwarded_subscription_is_what_the_connection_watches() {
        let (id, mut tab) = open(None, HashSet::new());
        let (_id, mut elsewhere) = open(None, HashSet::new());

        let topic = Topic::new("presence", &(22_u32,));
        let watching = vec![topic.as_str().to_owned()];

        crate::deliver(Frame::subscription(reduction(&id), watching));

        publish(|| crate::Fragment::new(topic.clone(), Markup::default()));

        assert_eq!(received(&mut tab), 1);
        assert_eq!(received(&mut elsewhere), 0, "and only the one it names");

        // Replaced wholesale rather than added to, which is the rule the
        // endpoint follows and therefore the rule a forward has to keep.
        crate::deliver(Frame::subscription(reduction(&id), Vec::new()));

        publish(|| crate::Fragment::new(topic, Markup::default()));
        assert_eq!(received(&mut tab), 0);
    }

    // ---- what a rotation does to a stream -----------------------------------

    #[tokio::test]
    async fn a_rotation_ends_the_streams_that_opened_under_the_old_name() {
        let name = Id::random();
        let (_id, _receiver) = open(
            Some(name.clone()),
            identity::Audiences::of(&Viewer(11)).into_keys(),
        );

        assert!(connected(&Viewer(11)));
        assert_eq!(disconnect(&name), 1);
        assert!(!connected(&Viewer(11)));
    }

    /// A session name is one browser, so a rotation reaches that browser's
    /// tabs and stops there.
    #[tokio::test]
    async fn a_rotation_leaves_every_other_browser_alone() {
        let mine = Id::random();
        let theirs = Id::random();

        let (_id, _mine) = open(
            Some(mine.clone()),
            identity::Audiences::of(&Viewer(12)).into_keys(),
        );
        let (_id, _theirs) = open(
            Some(theirs),
            identity::Audiences::of(&Viewer(13)).into_keys(),
        );

        assert_eq!(disconnect(&mine), 1);

        assert!(!connected(&Viewer(12)));
        assert!(connected(&Viewer(13)));
    }

    /// The rule a plausible implementation gets wrong.
    ///
    /// A stream opened by a browser carrying no cookie belongs to no browser in
    /// particular, and every such browser looks identical from here. Matching
    /// them as a group would let one visitor's sign-in end the streams of every
    /// anonymous visitor in the process.
    #[tokio::test]
    async fn a_stream_that_opened_with_no_name_is_never_ended_by_a_rotation() {
        let (_id, _receiver) = open(None, identity::Audiences::of(&Viewer(14)).into_keys());

        assert_eq!(disconnect(&Id::random()), 0);
        assert!(connected(&Viewer(14)));
    }

    /// The whole point, on the wire. The body has to *end*, because that is
    /// what makes `EventSource` reconnect and carry the new cookie; a stream
    /// that merely went quiet would leave the tab hanging on a dead identity.
    ///
    /// And it has to say how long to wait on the way out, because the browser
    /// would otherwise wait the seconds a broken network deserves while the
    /// tab shows a name that is no longer this browser's.
    #[tokio::test]
    async fn an_ended_stream_says_come_straight_back_and_then_closes() {
        let name = Id::random();

        let response = served()
            .oneshot(
                Request::builder()
                    .uri(STREAM)
                    .header(axum::http::header::COOKIE, format!("exos={name}"))
                    .body(Body::empty())
                    .expect("a valid request"),
            )
            .await
            .expect("the router answers");

        let mut body = response.into_body().into_data_stream();

        let greeting = read(&mut body).await;
        assert!(
            greeting.contains(&format!("retry: {}", RETRY.as_millis())),
            "an ordinary stream sets an ordinary wait: {greeting:?}"
        );

        assert_eq!(disconnect(&name), 1, "the cookie named this stream");

        let parting = read(&mut body).await;
        assert_eq!(
            parting,
            format!("retry: {}\n\n", RETRY_WHEN_RENAMED.as_millis()),
            "and nothing else, since there is nothing to apply"
        );

        assert!(body.next().await.is_none(), "and then the body is over");
    }

    /// One chunk of the body, as text.
    async fn read(body: &mut axum::body::BodyDataStream) -> String {
        let chunk = body
            .next()
            .await
            .expect("the stream says something")
            .expect("the body does not fail");

        String::from_utf8(chunk.to_vec()).expect("an event is text")
    }

    /// An application that never calls `identify` gets a stream that works and
    /// an identity that is empty, rather than a refusal or a warning.
    #[tokio::test]
    async fn a_stream_opens_with_no_resolver_configured() {
        let (id, _body) = greeted().await;

        let registry = connections().lock().expect("the lock is not poisoned");

        assert!(
            registry
                .get(&id)
                .expect("the connection is open")
                .audiences
                .is_empty()
        );
    }
}
