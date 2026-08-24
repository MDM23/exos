//! Pushing an effect to a person, over a real stream.
//!
//! Its neighbour `identity.rs` checks who a connection is. This checks what
//! reaches it, and it reads the bytes a browser would rather than the registry
//! behind them, because the wire format is the part a client has to agree
//! with.
//!
//! Nothing here waits on a timer. A negative is asserted by sending something
//! else afterwards and checking that it arrives first, which is a fact about
//! ordering rather than about how long a test is willing to wait.

use core::time::Duration;
use std::{
    collections::HashMap,
    sync::{Mutex, MutexGuard, Once},
};

use axum::{
    body::{Body, BodyDataStream, to_bytes},
    extract::Path,
    http::{Request, Response, StatusCode, header},
};
use exos::{Audience, Audiences, Effect, Fragment, Id, Markup, Topic, data, publish, send};
use tokio::time::timeout;
use tokio_stream::StreamExt as _;
use tower::ServiceExt as _;

/// What an application keeps under a session name.
#[derive(Default)]
struct Sessions(Mutex<HashMap<Id, u32>>);

impl Sessions {
    fn bind(&self, id: &Id, user: u32) {
        self.lock().insert(id.clone(), user);
    }

    async fn viewer(&self, id: &Id) -> Option<u32> {
        self.lock().get(id).copied()
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<Id, u32>> {
        self.0.lock().expect("the lock is not poisoned")
    }
}

#[derive(Hash)]
struct Viewer(u32);

impl Audience for Viewer {
    const NAME: &'static str = "viewer";
}

fn seeded() {
    static SEED: Once = Once::new();

    SEED.call_once(|| {
        exos::provide(Sessions::default());

        exos::identify(async |name: Option<Id>| {
            let Some(name) = name else {
                return Ok(Audiences::none());
            };

            Ok(match data::<Sessions>().viewer(&name).await {
                Some(user) => Audiences::of(&Viewer(user)),
                None => Audiences::none(),
            })
        });
    });
}

/// Serves the wrapper for one topic, which is the only way a browser comes by
/// a token: it is minted in a request and bound to the session that request
/// carried. Nothing outside the server can produce one, so a test gets its
/// tokens the way a page does rather than around the side.
#[exos::get("/served/{topic}")]
async fn serve(Path(topic): Path<String>) -> Effect {
    Effect::patch(Fragment::new(Topic::from_raw(&topic), Markup::default()).to_markup())
}

/// One open tab: the browser it belongs to, the connection the server named
/// it, and the events still to come. Dropping it closes the stream, so a test
/// holds one for as long as it expects anything.
struct Tab {
    name: Id,
    connection: String,
    events: BodyDataStream,
}

impl Tab {
    /// Opens a stream as whoever `user` is, and reads the greeting.
    async fn open(user: u32) -> Self {
        seeded();

        let name = Id::random();
        data::<Sessions>().bind(&name, user);

        let response = exos::app()
            .oneshot(
                Request::builder()
                    .uri("/_exos/live")
                    .header(header::COOKIE, format!("exos={name}"))
                    .body(Body::empty())
                    .expect("a valid request"),
            )
            .await
            .expect("the router answers");

        assert_eq!(response.status(), StatusCode::OK);

        let mut events = response.into_body().into_data_stream();

        let greeting = read(&mut events).await;
        assert!(greeting.contains("event: connection"), "{greeting:?}");

        // Line by line, because the greeting carries the reconnection time as
        // well as the name.
        let connection = greeting
            .lines()
            .find_map(|line| line.strip_prefix("data: "))
            .unwrap_or_else(|| panic!("the greeting names the connection, got {greeting:?}"))
            .to_owned();

        Self {
            name,
            connection,
            events,
        }
    }

    /// The next event, as the browser would read it off the wire.
    async fn next(&mut self) -> String {
        read(&mut self.events).await
    }

    /// Reports one topic as visible, which is what the runtime does after every
    /// mutation and what a publish is matched against.
    async fn watching(&self, topic: &Topic) {
        let body = format!(
            r#"{{"connection":"{}","topics":[["{}","{}"]]}}"#,
            self.connection,
            topic.as_str(),
            self.served(topic).await
        );

        let response = self
            .request(
                Request::builder()
                    .method("POST")
                    .uri("/_exos/subscribe")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body))
                    .expect("a valid request"),
            )
            .await;

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    /// The token this browser was served for a topic, read out of the markup
    /// exactly as the runtime reads it off the element.
    async fn served(&self, topic: &Topic) -> String {
        let response = self
            .request(
                Request::builder()
                    .uri(format!("/served/{}", topic.as_str()))
                    .body(Body::empty())
                    .expect("a valid request"),
            )
            .await;

        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("the body is whole");

        let markup = String::from_utf8(body.to_vec()).expect("the markup is text");

        markup
            .split_once("data-token=\"")
            .and_then(|(_, rest)| rest.split_once('"'))
            .map(|(token, _)| token.to_owned())
            .unwrap_or_else(|| panic!("a served fragment carries a token, got {markup:?}"))
    }

    /// A request from this browser, which means one carrying its cookie.
    async fn request(&self, mut request: Request<Body>) -> Response<Body> {
        request.headers_mut().insert(
            header::COOKIE,
            format!("exos={}", self.name)
                .parse()
                .expect("a name is a header value"),
        );

        exos::app()
            .oneshot(request)
            .await
            .expect("the router answers")
    }
}

/// One chunk, with a ceiling so a stream that says nothing fails the test
/// rather than hanging it. The timeout is a backstop and never a wait: every
/// assertion below is about something already sent.
async fn read(events: &mut BodyDataStream) -> String {
    let chunk = timeout(Duration::from_secs(5), events.next())
        .await
        .expect("the stream says something rather than nothing at all")
        .expect("the stream is still open")
        .expect("the body does not fail");

    String::from_utf8(chunk.to_vec()).expect("an event is text")
}

/// The whole point: an effect addressed to a person arrives on their stream,
/// framed exactly as a handler's reply would be, so the client parses it with
/// the code it already has.
#[tokio::test]
async fn an_effect_arrives_as_the_events_it_is_made_of() {
    let mut tab = Tab::open(1).await;

    send(
        &Viewer(1),
        &Effect::patch(Markup::from(String::from("<p id=\"toast\">hi</p>"))).focus("#toast"),
    );

    assert_eq!(
        tab.next().await,
        "event: patch\ndata: <p id=\"toast\">hi</p>\n\n"
    );
    assert_eq!(tab.next().await, "event: focus\ndata: #toast\n\n");
}

/// Addressed to a person rather than to a screen, so it does not matter what
/// the tab is displaying and it does not reach anybody else's.
#[tokio::test]
async fn it_reaches_that_person_and_stops_there() {
    let mut theirs = Tab::open(2).await;
    let mut somebody_else = Tab::open(3).await;

    send(&Viewer(2), &Effect::remove("#theirs"));
    send(&Viewer(3), &Effect::remove("#somebody-else"));

    assert_eq!(theirs.next().await, "event: remove\ndata: #theirs\n\n");

    // The first thing the other stream has to say is its own. Had it received
    // the effect above, that would be sitting in front of this one.
    assert_eq!(
        somebody_else.next().await,
        "event: remove\ndata: #somebody-else\n\n"
    );
}

/// The order `send` promises, across the two mechanisms rather than within
/// one: a connection has a single channel and both sends take the registry
/// lock, so what was called first arrives first.
#[tokio::test]
async fn a_publish_and_a_send_arrive_in_the_order_they_were_called() {
    let mut tab = Tab::open(4).await;

    let topic = Topic::new("badge", &(4_u32,));
    tab.watching(&topic).await;

    publish(|| Fragment::new(topic, Markup::from(String::from("<span>2</span>"))));
    send(&Viewer(4), &Effect::remove("#after"));

    assert!(
        tab.next().await.starts_with("event: patch"),
        "the publish was called first"
    );
    assert_eq!(tab.next().await, "event: remove\ndata: #after\n\n");
}

/// A fragment on screen still updates, because a subscription is proved rather
/// than identified. Only the directed half needs to know who anybody is.
#[tokio::test]
async fn a_tab_nobody_is_addressing_still_gets_its_patches() {
    let mut tab = Tab::open(5).await;

    let topic = Topic::new("badge", &(5_u32,));
    tab.watching(&topic).await;

    send(&Viewer(6), &Effect::remove("#not-for-them"));
    publish(|| Fragment::new(topic, Markup::from(String::from("<span>7</span>"))));

    // The patch carries the subscription wrapper the fragment renders with,
    // and the effect addressed to somebody else is not in front of it.
    let patch = tab.next().await;

    assert!(patch.starts_with("event: patch"), "{patch}");
    assert!(patch.contains("<span>7</span>"), "{patch}");
}
