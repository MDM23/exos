//! What crosses to the other nodes, with a bus registered.
//!
//! The registry is process-global, so no test process can hold two nodes. What
//! this checks is therefore the two ends rather than the topology: that a
//! publish and a send hand over a frame carrying what a local tab received, and
//! that a frame handed back reaches the connections it names. A real second
//! node adds a broker and nothing else.
//!
//! The bus is registered once for the whole binary, which is also the point: a
//! registration is a startup decision, and every assertion below is made with
//! one in place, so what a bus changes about local delivery is visible here and
//! is nothing.

use core::time::Duration;
use std::sync::{Arc, OnceLock};

use axum::{
    Router,
    body::{Body, to_bytes},
    extract::Path,
    http::{Request, Response, StatusCode, header},
};
use exos::{Audience, Effect, Fragment, Frame, Id, Kind, Markup, Topic, publish, send};
use tokio::{
    sync::Mutex,
    sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel},
    time::timeout,
};
use tokio_stream::StreamExt as _;
use tower::ServiceExt as _;

#[derive(Hash)]
struct Viewer(u32);

impl Audience for Viewer {
    const NAME: &'static str = "viewer";
}

/// Where a frame this node sent turns up, standing in for a broker.
///
/// A channel rather than a list, because a send is spawned: what a test waits
/// for is the frame arriving rather than a moment when it must already have.
static SENT: OnceLock<Mutex<UnboundedReceiver<Frame>>> = OnceLock::new();

/// The broker this node sends into, which is a channel a test reads.
fn broker() -> Arc<UnboundedSender<Frame>> {
    static BROKER: OnceLock<Arc<UnboundedSender<Frame>>> = OnceLock::new();

    Arc::clone(BROKER.get_or_init(|| {
        let (sender, receiver) = unbounded_channel();

        drop(SENT.set(Mutex::new(receiver)));

        Arc::new(sender)
    }))
}

/// This node: the application, with the key and the bus it was started with.
///
/// The bus is written the way an adapter is, holding a client of its own: a
/// closure answering with a future, rather than an async closure, because the
/// future an async closure returns borrows what it captured and a spawned one
/// cannot. This is the file that would notice if that stopped being the shape.
///
/// Built from inside this test's runtime, which is where a bus takes the handle
/// it spawns sends on.
fn node() -> Router {
    let broker = broker();

    exos::app()
        // First, and a bus refuses to be registered without it: a cluster signs
        // with one key, and a random one per process is a token that verifies
        // on the node that minted it and nowhere else.
        .keys(exos::Keys::from_secret("a cluster agrees about this"))
        .bus(move |frame: Frame| {
            let broker = Arc::clone(&broker);

            async move {
                broker.send(frame)?;
                Ok(())
            }
        })
        .into()
}

/// The frames this node sent, with the application in place around them.
async fn bus() -> tokio::sync::MutexGuard<'static, UnboundedReceiver<Frame>> {
    drop(node());

    SENT.get()
        .expect("the receiver is set with the registration")
        .lock()
        .await
}

/// The next frame, with a ceiling so a bus that says nothing fails the test
/// rather than hanging it.
async fn crossed(sent: &mut UnboundedReceiver<Frame>) -> Frame {
    timeout(Duration::from_secs(5), sent.recv())
        .await
        .expect("a frame crosses rather than nothing at all")
        .expect("the channel is open")
}

fn fragment(id: u32) -> Fragment<impl Fn() -> Markup> {
    Fragment::new(Topic::new("presence", &(id,)), move || {
        Markup(format!("<span id=\"presence-{id}\">here</span>"))
    })
}

/// The wire carries the result rather than the request: a topic is a hash of a
/// name and its arguments, so no receiving node could render from one. It is
/// rendered once for the whole cluster, which is also the cheaper shape.
#[tokio::test]
async fn a_publish_crosses_as_the_patch_it_rendered() {
    let mut sent = bus().await;

    publish(fragment(1));

    let frame = crossed(&mut sent).await;

    assert_eq!(frame.kind(), Kind::Topic);
    assert_eq!(frame.key(), Topic::new("presence", &(1_u32,)).as_str());
    assert!(frame.trace().is_empty(), "nothing is sampling yet");

    let bytes = frame.to_bytes();
    let text = String::from_utf8(bytes.clone()).expect("a frame is text");

    assert!(text.contains("presence-1"), "{text}");
    assert!(text.contains("patch"), "{text}");

    // And what crossed is what a node reading it gets back, which is the whole
    // of what a broker is asked to carry.
    assert_eq!(Frame::from_bytes(&bytes), Some(frame));
}

/// The feature that wanted a cluster in the first place: a person with tabs on
/// three nodes is one send and three deliveries.
#[tokio::test]
async fn a_send_crosses_as_the_steps_of_its_effect() {
    let mut sent = bus().await;

    send(&Viewer(2), &Effect::reload().focus("#x"));

    let frame = crossed(&mut sent).await;

    assert_eq!(frame.kind(), Kind::Audience);
    // The reduction an audience and a topic share, spelled out here so that
    // what a frame is keyed by is asserted rather than taken from the code
    // that produced it.
    assert_eq!(frame.key(), Topic::new(Viewer::NAME, &Viewer(2)).as_str());

    let text = String::from_utf8(frame.to_bytes()).expect("a frame is text");

    assert!(text.contains("reload"), "{text}");
    assert!(text.contains("focus"), "{text}");
}

/// A send to nobody is silent locally and has nothing to say to a cluster
/// either: an audience nobody is connected as is the ordinary case, and it is
/// the local registry that cannot answer for the other nodes.
#[tokio::test]
async fn an_effect_with_no_steps_crosses_nothing() {
    let mut sent = bus().await;

    send(&Viewer(3), &Effect::none());
    publish(fragment(4));

    // The publish is what proves the silence above: had the empty send
    // crossed, it would be first.
    assert_eq!(
        crossed(&mut sent).await.key(),
        Topic::new("presence", &(4_u32,)).as_str()
    );
}

/// The other end, as an adapter has it: bytes off a broker become a frame and
/// a frame is what `deliver` takes. Addressed at nothing this node holds, which
/// is most frames until a broker is doing the filtering, so it is free and
/// silent rather than an error.
#[tokio::test]
async fn a_frame_off_the_wire_is_one_deliver_takes() {
    let key = Topic::new(Viewer::NAME, &Viewer(5)).as_str().to_owned();

    let bytes = serde_json::json!({
        "kind": "audience",
        "key": key,
        "steps": [["reload", "reload"]],
        "trace": "",
    })
    .to_string()
    .into_bytes();

    let frame = Frame::from_bytes(&bytes).expect("a frame this build understands");

    assert_eq!(frame.kind(), Kind::Audience);
    assert_eq!(frame.key(), key);

    exos::deliver(frame);
}

// ---- a subscription that landed on the wrong node ---------------------------

/// Serves the wrapper for one topic, which is the only way a browser comes by
/// a token: it is minted in a request and bound to the session that request
/// carried. A test gets its tokens the way a page does rather than around the
/// side.
#[exos::get("/served/{topic}")]
async fn serve(Path(topic): Path<String>) -> Effect {
    Effect::patch(Fragment::new(Topic::from_raw(&topic), Markup::default).to_markup())
}

/// A request from one browser, which means one carrying its cookie.
async fn from(name: &Id, request: axum::http::request::Builder, body: Body) -> Response<Body> {
    node()
        .oneshot(
            request
                // What the runtime sends and what exos refuses an unsafe
                // request without: this stands in for a browser running it.
                .header("x-exos", "true")
                .header(header::COOKIE, format!("exos={name}"))
                .body(body)
                .expect("a valid request"),
        )
        .await
        .expect("the router answers")
}

/// The token this browser was served for a topic, read out of the markup
/// exactly as the runtime reads it off the element.
async fn served(name: &Id, topic: &Topic) -> String {
    let response = from(
        name,
        Request::builder().uri(format!("/served/{}", topic.as_str())),
        Body::empty(),
    )
    .await;

    let markup = String::from_utf8(
        to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("the body is whole")
            .to_vec(),
    )
    .expect("the markup is text");

    markup
        .split_once("data-token=\"")
        .and_then(|(_, rest)| rest.split_once('"'))
        .map(|(token, _)| token.to_owned())
        .unwrap_or_else(|| panic!("a served fragment carries a token, got {markup:?}"))
}

/// Says what a browser is displaying, naming whichever connection it is told
/// to, which is how a test stands in for a request that landed elsewhere.
async fn subscribing(name: &Id, connection: &str, topic: &Topic) -> StatusCode {
    let body = serde_json::json!({
        "connection": connection,
        "topics": [[topic.as_str(), served(name, topic).await]],
    })
    .to_string();

    from(
        name,
        Request::builder()
            .method("POST")
            .uri("/_exos/subscribe")
            .header(header::CONTENT_TYPE, "application/json"),
        Body::from(body),
    )
    .await
    .status()
}

/// The connection id of a real stream on this node, and the body that keeps it
/// open for as long as the test holds it.
///
/// Opened with the cookie, because that is what a stream carries and what
/// tells a rotation which streams are this browser's.
async fn opened(name: &Id) -> (String, Body) {
    let response = node()
        .oneshot(
            Request::builder()
                .uri("/_exos/live")
                .header(header::COOKIE, format!("exos={name}"))
                .body(Body::empty())
                .expect("a valid request"),
        )
        .await
        .expect("the router answers");

    let mut events = response.into_body().into_data_stream();

    let greeting = String::from_utf8(
        events
            .next()
            .await
            .expect("the stream says something")
            .expect("the body does not fail")
            .to_vec(),
    )
    .expect("the greeting is text");

    let id = greeting
        .lines()
        .find_map(|line| line.strip_prefix("data: "))
        .unwrap_or_else(|| panic!("the greeting names the connection, got {greeting:?}"))
        .to_owned();

    (id, Body::new(events))
}

/// The loop this stage exists to close. A tab holds its stream to one node and
/// subscribes wherever the load balancer points, so the answer to an id this
/// node never minted is a forward and a `204`, not the `410` that would tear a
/// working stream down.
#[tokio::test]
async fn a_subscription_for_another_node_is_forwarded_rather_than_refused() {
    let mut sent = bus().await;

    let name = Id::random();
    let topic = Topic::new("presence", &(6_u32,));
    let elsewhere = "0123456789abcdef-00112233445566778899aabbccddeeff";

    assert_eq!(
        subscribing(&name, elsewhere, &topic).await,
        StatusCode::NO_CONTENT
    );

    let frame = crossed(&mut sent).await;

    assert_eq!(frame.kind(), Kind::Connection);
    assert_eq!(frame.key(), Topic::new("connection", &elsewhere).as_str());

    // The names, proved here and crossing without their tokens: the node with
    // the cookie is the node that can check one, and it did.
    let text = String::from_utf8(frame.to_bytes()).expect("a frame is text");

    assert!(text.contains(topic.as_str()), "{text}");
    assert!(!text.contains("data-token"), "{text}");
    assert!(
        !text.contains(&name.to_string()),
        "and no session name: {text}"
    );
}

/// And the case the forward must not swallow: an id this node minted and has
/// no connection for is a stream that has gone, which the browser is told so
/// that it opens a fresh one.
#[tokio::test]
async fn a_connection_this_node_minted_and_lost_is_still_gone() {
    let mut sent = bus().await;

    let name = Id::random();
    let (id, _open) = opened(&name).await;
    let topic = Topic::new("presence", &(7_u32,));

    // The same node, a name it never handed out.
    let stale = format!("{}-{}", id.split_once('-').expect("a prefix").0, "c0ffee");

    assert_eq!(subscribing(&name, &stale, &topic).await, StatusCode::GONE);

    // Nothing crossed for it, which the next frame is what proves: had the
    // refusal forwarded, it would be first.
    publish(fragment(8));

    assert_eq!(
        crossed(&mut sent).await.key(),
        Topic::new("presence", &(8_u32,)).as_str()
    );
}

// ---- a rotation, which is the one frame that ends something -----------------

/// exos's half of signing out. What the application kept under the name is its
/// own to delete; what the streams still holding that name do is exos's.
#[exos::post("/sign-out")]
async fn sign_out() -> Effect {
    exos::session().end();

    Effect::none()
}

/// The hole this stage closes. A browser is one cookie and many tabs, and the
/// tabs it did not sign out in may be streaming from anywhere, so the decision
/// has to reach every node rather than the one that took the request.
#[tokio::test]
async fn signing_out_crosses_as_the_browser_it_ended() {
    let mut sent = bus().await;

    let name = Id::random();

    from(
        &name,
        Request::builder().method("POST").uri("/sign-out"),
        Body::empty(),
    )
    .await;

    let frame = crossed(&mut sent).await;

    assert_eq!(frame.kind(), Kind::Session);
    // The reduction, spelled out rather than taken from the code that produced
    // it: this is the one string the two nodes have to agree on.
    assert_eq!(frame.key(), Topic::new("session", &name).as_str());

    let text = String::from_utf8(frame.to_bytes()).expect("a frame is text");

    assert!(
        !text.contains(&name.to_string()),
        "a cookie is the last thing to travel to say it is worthless: {text}"
    );
}

/// The other end, and the whole point of it: a stream this node holds ends
/// because a browser signed out somewhere else. The body has to be *over*,
/// because that is what makes `EventSource` come back carrying whatever cookie
/// the browser has by then.
#[tokio::test]
async fn a_revocation_off_the_wire_ends_this_node_s_streams() {
    let name = Id::random();
    let (_id, body) = opened(&name).await;
    let (_id, elsewhere) = opened(&Id::random()).await;

    let bytes = serde_json::json!({
        "kind": "session",
        "key": Topic::new("session", &name).as_str(),
        "trace": "",
    })
    .to_string()
    .into_bytes();

    let frame = Frame::from_bytes(&bytes).expect("a frame this build understands");

    assert_eq!(frame.kind(), Kind::Session);

    exos::deliver(frame);

    let mut ended = body.into_data_stream();
    let parting = ended.next().await.expect("the stream says goodbye");

    assert!(
        String::from_utf8(parting.expect("the body does not fail").to_vec())
            .expect("an event is text")
            .contains("retry:"),
        "and says how long to wait before coming back"
    );
    assert!(ended.next().await.is_none(), "and then the body is over");

    // The rule a plausible implementation gets wrong, one node further out: a
    // revocation names one browser, not every browser this node is holding.
    assert!(
        timeout(
            Duration::from_millis(50),
            elsewhere.into_data_stream().next()
        )
        .await
        .is_err(),
        "another browser's stream is still open and still quiet"
    );
}
