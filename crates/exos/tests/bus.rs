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
use std::sync::{Arc, Once, OnceLock};

use exos::{Audience, Effect, Fragment, Frame, Kind, Markup, Topic, publish, send};
use tokio::{
    sync::Mutex,
    sync::mpsc::{UnboundedReceiver, unbounded_channel},
    time::timeout,
};

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

/// Registers the bus, once, from inside this test's runtime.
///
/// Written the way an adapter is, holding a client of its own: a closure
/// answering with a future, rather than an async closure, because the future an
/// async closure returns borrows what it captured and a spawned one cannot.
/// This is the file that would notice if that stopped being the shape.
async fn bus() -> tokio::sync::MutexGuard<'static, UnboundedReceiver<Frame>> {
    static REGISTERED: Once = Once::new();

    REGISTERED.call_once(|| {
        let (sender, receiver) = unbounded_channel();
        let broker = Arc::new(sender);

        drop(SENT.set(Mutex::new(receiver)));

        // First, and a bus refuses to be registered without it: a cluster
        // signs with one key, and a random one per process is a token that
        // verifies on the node that minted it and nowhere else.
        exos::keys(exos::Keys::from_secret("a cluster agrees about this"));

        exos::bus(move |frame: Frame| {
            let broker = Arc::clone(&broker);

            async move {
                broker.send(frame)?;
                Ok(())
            }
        });
    });

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

fn fragment(id: u32) -> Fragment {
    Fragment::new(
        Topic::new("presence", &(id,)),
        Markup(format!("<span id=\"presence-{id}\">here</span>")),
    )
}

/// The wire carries the result rather than the request: a topic is a hash of a
/// name and its arguments, so no receiving node could render from one. It is
/// rendered once for the whole cluster, which is also the cheaper shape.
#[tokio::test]
async fn a_publish_crosses_as_the_patch_it_rendered() {
    let mut sent = bus().await;

    publish(|| fragment(1));

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
    publish(|| fragment(4));

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
