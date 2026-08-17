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
//! or, once a connection carries identity, receiving another viewer's. An
//! unguessable id makes that a matter of arithmetic rather than of trust.

use std::{
    collections::{HashMap, HashSet},
    convert::Infallible,
    pin::Pin,
    sync::{Mutex, OnceLock},
    task::{Context, Poll},
};

use axum::{
    Json, Router,
    http::StatusCode,
    response::{
        IntoResponse,
        sse::{Event, KeepAlive, Sse},
    },
    routing::{get, post},
};
use futures_core::Stream;
use serde::Deserialize;
use tokio::sync::broadcast;
use tokio_stream::{StreamExt as _, wrappers::BroadcastStream};

use crate::{Step, hex, live::Topic};

/// Where the stream and the subscription endpoint are mounted.
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

/// A live connection: its outbound channel and the topics it watches.
struct Connection {
    sender: broadcast::Sender<Event>,
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
#[must_use]
pub fn connection_count() -> usize {
    connections()
        .lock()
        .expect("the registry lock is never held across a panic")
        .len()
}

/// Re-renders nothing, since the caller already did, and sends the fragment to
/// every connection watching it.
///
/// Publishing a fragment nobody is looking at is free and silent, which is
/// what lets a handler publish unconditionally rather than asking first.
///
/// # Panics
///
/// If the registry lock was poisoned; see [`connection_count`].
pub fn publish(fragment: &crate::Fragment) {
    let event = Event::from(Step::Patch(fragment.to_markup()));
    let topic = fragment.topic().as_str();

    let registry = connections()
        .lock()
        .expect("the registry lock is never held across a panic");

    for connection in registry.values() {
        if connection.topics.contains(topic) {
            // A closed receiver is a tab that went away between the check and
            // this send; the cleanup path removes it.
            drop(connection.sender.send(event.clone()));
        }
    }
}

/// A name no other connection has and no client can guess.
///
/// 128 bits from the operating system, as hex. Guessability is the whole
/// property: the id is a bearer name for a connection, so anybody holding one
/// can replace what that connection watches.
///
/// # Panics
///
/// If the operating system has no entropy to give, which is where
/// [`Keys::random`](crate::Keys::random) already stands and has the same
/// answer: a guessable id is worse than not starting.
fn mint() -> String {
    let mut bytes = [0_u8; 16];

    getrandom::fill(&mut bytes).expect("the operating system provides entropy for a connection id");

    hex::encode(&bytes)
}

/// Registers a connection under a fresh id, with the receiver its response
/// drains.
fn open() -> (String, broadcast::Receiver<Event>) {
    let id = mint();
    let (sender, receiver) = broadcast::channel(CAPACITY);

    connections()
        .lock()
        .expect("the registry lock is never held across a panic")
        .insert(
            id.clone(),
            Connection {
                sender,
                topics: HashSet::new(),
            },
        );

    (id, receiver)
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

async fn subscribe(Json(request): Json<Subscription>) -> StatusCode {
    let Ok(mut registry) = connections().lock() else {
        return StatusCode::INTERNAL_SERVER_ERROR;
    };

    let Some(connection) = registry.get_mut(&request.connection) else {
        // The stream died, or the id was invented. Either way the client
        // should reconnect rather than have a connection conjured for it.
        return StatusCode::GONE;
    };

    // An unverifiable topic is dropped rather than failing the whole request:
    // one stale fragment left over from a previous page should not cost a tab
    // its other subscriptions.
    connection.topics = request
        .topics
        .into_iter()
        .filter(|(topic, token)| Topic::from_raw(topic).verify(token))
        .map(|(topic, _)| topic)
        .collect();

    StatusCode::NO_CONTENT
}

async fn stream() -> impl IntoResponse {
    let (id, receiver) = open();

    let events = BroadcastStream::new(receiver)
        // A lagged tab skips ahead rather than stalling the publisher; the
        // next publish of anything it watches brings it back in line.
        .filter_map(|event| event.ok().map(Ok::<Event, Infallible>));

    Sse::new(Disconnect {
        greeting: Some(Event::default().event(HELLO).data(&id)),
        id,
        events,
    })
    .keep_alive(KeepAlive::default())
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
    use axum::{body::Body, http::Request};
    use tower::ServiceExt as _;

    use super::*;

    /// Opens a real stream through the router and reads what it says first,
    /// which is the wire format the client parses rather than a stand-in for
    /// it. The body is handed back because dropping it closes the connection.
    async fn greeted() -> (String, Body) {
        let response = routes()
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

        let id = text
            .strip_prefix("event: connection\ndata: ")
            .and_then(|rest| rest.split('\n').next())
            .unwrap_or_else(|| panic!("the greeting names the connection, got {text:?}"))
            .to_owned();

        (id, Body::new(body))
    }

    #[tokio::test]
    async fn the_stream_names_the_connection_before_anything_else() {
        let (id, _body) = greeted().await;

        assert_eq!(id.len(), 32, "128 bits as hex");
        assert!(id.chars().all(|character| character.is_ascii_hexdigit()));
    }

    /// The point of the whole handshake: the only id that works is one the
    /// server handed out, so a client cannot name a connection it was not given.
    #[tokio::test]
    async fn the_id_the_server_gave_is_the_one_that_subscribes() {
        let (id, _body) = greeted().await;

        let topic = Topic::new("presence", &(1_u32,));

        let status = subscribe(Json(Subscription {
            connection: id.clone(),
            topics: vec![(topic.as_str().to_owned(), topic.token())],
        }))
        .await;

        assert_eq!(status, StatusCode::NO_CONTENT);

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
        let (id, _receiver) = open();

        let real = Topic::new("presence", &(42_u32,));
        let forged = Topic::new("presence", &(43_u32,));

        let status = subscribe(Json(Subscription {
            connection: id.clone(),
            topics: vec![
                (real.as_str().to_owned(), real.token()),
                (forged.as_str().to_owned(), String::from("0000000000000000")),
            ],
        }))
        .await;

        assert_eq!(status, StatusCode::NO_CONTENT);

        let registry = connections().lock().expect("the lock is not poisoned");
        let topics = &registry.get(&id).expect("the connection is open").topics;

        assert!(topics.contains(real.as_str()));
        assert!(
            !topics.contains(forged.as_str()),
            "the unproven topic is dropped and the rest of the request still applies"
        );
    }

    /// A guessed id is indistinguishable from a stream that has since dropped,
    /// and both get the same answer rather than a connection conjured for them.
    #[tokio::test]
    async fn an_unknown_connection_is_told_to_reconnect() {
        let status = subscribe(Json(Subscription {
            connection: String::from("never-opened"),
            topics: Vec::new(),
        }))
        .await;

        assert_eq!(status, StatusCode::GONE);
    }
}
