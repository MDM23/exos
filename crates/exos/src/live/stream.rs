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

use std::{
    collections::{HashMap, HashSet},
    convert::Infallible,
    pin::Pin,
    sync::{Mutex, OnceLock},
    task::{Context, Poll},
};

use axum::{
    Json, Router,
    extract::Query,
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

use crate::{Step, live::Topic};

/// Where the stream and the subscription endpoint are mounted.
const STREAM: &str = "/_exos/live";
const SUBSCRIBE: &str = "/_exos/subscribe";

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

/// Registers a connection and returns the receiver its response drains.
fn open(id: &str) -> broadcast::Receiver<Event> {
    let (sender, receiver) = broadcast::channel(CAPACITY);

    connections()
        .lock()
        .expect("the registry lock is never held across a panic")
        .insert(
            id.to_owned(),
            Connection {
                sender,
                topics: HashSet::new(),
            },
        );

    receiver
}

/// Forgets a connection when its stream ends.
fn close(id: &str) {
    if let Ok(mut registry) = connections().lock() {
        registry.remove(id);
    }
}

/// What a browser sends to open its one stream.
#[derive(Debug, Deserialize)]
struct Connect {
    connection: String,
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

async fn stream(Query(connect): Query<Connect>) -> impl IntoResponse {
    let events = BroadcastStream::new(open(&connect.connection))
        // A lagged tab skips ahead rather than stalling the publisher; the
        // next publish of anything it watches brings it back in line.
        .filter_map(|event| event.ok().map(Ok::<Event, Infallible>));

    Sse::new(Disconnect {
        id: connect.connection,
        events,
    })
    .keep_alive(KeepAlive::default())
}

/// A stream that deregisters its connection when the browser goes away.
///
/// Without this the registry grows by one entry per tab ever opened, and every
/// publish walks them all.
struct Disconnect<S> {
    id: String,
    events: S,
}

impl<S: Stream + Unpin> Stream for Disconnect<S> {
    type Item = S::Item;

    fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<S::Item>> {
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
mod tests {
    use super::*;

    #[tokio::test]
    async fn subscribing_keeps_only_the_topics_it_can_prove() {
        let _receiver = open("proof");

        let real = Topic::new("presence", &(42_u32,));
        let forged = Topic::new("presence", &(43_u32,));

        let status = subscribe(Json(Subscription {
            connection: String::from("proof"),
            topics: vec![
                (real.as_str().to_owned(), real.token()),
                (forged.as_str().to_owned(), String::from("0000000000000000")),
            ],
        }))
        .await;

        assert_eq!(status, StatusCode::NO_CONTENT);

        let registry = connections().lock().expect("the lock is not poisoned");
        let topics = &registry
            .get("proof")
            .expect("the connection is open")
            .topics;

        assert!(topics.contains(real.as_str()));
        assert!(
            !topics.contains(forged.as_str()),
            "the unproven topic is dropped and the rest of the request still applies"
        );
    }

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
