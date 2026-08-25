//! Reaching the tabs another process is holding.
//!
//! A connection is a socket, so the registry in
//! [stream.rs](../../src/live/stream.rs) belongs to the process that opened it
//! and cannot be shared. What crosses instead is the message. Every node keeps
//! its own map of its own connections, and a [`Frame`] is one delivery
//! addressed the way that map is already keyed.
//!
//! exos ships no broker adapter, the same way it ships no session store: the
//! choice is the application's and exos would learn nothing by being told. What
//! it ships is the two ends.
//!
//! ```ignore
//! exos::bus(move |frame| {
//!     let redis = redis.clone();
//!
//!     async move {
//!         redis.publish("exos", frame.to_bytes()).await?;
//!         Ok(())
//!     }
//! });
//!
//! // The application owns its subscriber loop, and its reconnection.
//! tokio::spawn(async move {
//!     while let Some(message) = subscription.next().await {
//!         if let Some(frame) = exos::Frame::from_bytes(message.payload()) {
//!             exos::deliver(frame);
//!         }
//!     }
//! });
//! ```
//!
//! A closure answering with a future rather than an async closure, and the
//! reason is worth knowing before writing one: the future an async closure
//! returns borrows what the closure captured, and a frame is sent from a
//! spawned task that outlives the call. The clone above is what a client is
//! held by, and every broker client is cheap to clone for exactly this.
//!
//! # With no bus registered, none of this happens
//!
//! No frame is built and nothing is spawned, so a single-node application pays
//! for the cluster path in neither bytes nor time. That is what keeps this from
//! being something every application carries.
//!
//! # What a frame never carries
//!
//! No session name, no connection id, no fragment arguments, no application
//! state and no token. A key is already a hash, and the two things that would
//! otherwise have to cross and are bearer secrets cross as the same reduction
//! [identity.rs](../../src/identity.rs) produces rather than as themselves. A
//! broker's operator, its logs and its backups never hold anything that logs
//! anybody in.

use core::{future::Future, pin::Pin};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use tokio::runtime::Handle;

/// What an adapter answers with.
///
/// The error is boxed so an adapter can use `?` over whatever its client
/// returns. exos logs it and drops the frame: a bus outage costs a cluster its
/// cross-node liveness, and retrying is a choice about the frames that are not
/// safe to deliver twice.
pub type Sent = Result<(), Box<dyn core::error::Error + Send + Sync>>;

/// Which set of a connection a frame's key is matched against.
///
/// Two, and they stay two for the reason the registry holds two sets: merged,
/// whether a key was proved by being served or derived from who somebody is
/// would depend on a check nobody can see, and the first frame that forgot it
/// would let a tab be addressed as somebody.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Kind {
    /// A fragment, watched by whoever was served it.
    Topic,
    /// A person, as [`Audience`](crate::Audience) reduces them.
    Audience,
}

/// One delivery, on its way to the nodes this one is not.
///
/// Built by [`publish`](crate::publish) and [`send`](crate::send) when a bus is
/// registered, and handed back to [`deliver`] by the application's subscriber.
/// The steps cross already framed, as the event name and payload pairs the wire
/// carries, so a receiving node needs to know nothing about what a step is: it
/// pushes what it was handed into a channel and the browser reads the bytes it
/// would have read from the node that sent it. A new step is a new string
/// rather than a new frame version, so adding one does not divide a cluster
/// mid-deploy.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Frame {
    kind: Kind,
    key: String,
    steps: Vec<(String, String)>,
    /// The trace this delivery belongs to, or empty where nothing is sampling.
    ///
    /// Here from the first version on purpose. The codec is exos's rather than
    /// the caller's, so adding a field to it later is a change two nodes can
    /// disagree about, and a codec change halfway through a rolling deploy is
    /// the precise failure this whole module exists to prevent. A trace
    /// context identifies a trace and not a person, so carrying one costs the
    /// rule above nothing.
    trace: String,
}

impl Frame {
    /// One delivery of `steps` to whatever is watching `key`.
    pub(crate) fn new(kind: Kind, key: impl Into<String>, steps: Vec<(String, String)>) -> Self {
        Self {
            kind,
            key: key.into(),
            steps,
            trace: String::new(),
        }
    }

    /// Which set the key is matched against.
    #[must_use]
    pub const fn kind(&self) -> Kind {
        self.kind
    }

    /// What a connection has to be watching to receive this.
    #[must_use]
    pub fn key(&self) -> &str {
        &self.key
    }

    /// The trace this belongs to, or empty.
    #[must_use]
    pub fn trace(&self) -> &str {
        &self.trace
    }

    /// The steps, as the wire frames them.
    pub(crate) fn steps(&self) -> &[(String, String)] {
        &self.steps
    }

    /// The bytes to publish.
    ///
    /// The codec is exos's rather than the adapter's. A frame crosses between
    /// two binaries, so how it is spelled is part of the format and not a
    /// preference, and two nodes configured with different opinions about it
    /// would be a cluster that looks connected and delivers nothing.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_default()
    }

    /// The frame those bytes were, if they are one this build understands.
    ///
    /// `None` rather than an error, and that is the forward compatibility: a
    /// node running an older build ignores a kind it has never heard of
    /// instead of failing, so a cluster mid-rollover delivers what both halves
    /// understand and nothing else.
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        serde_json::from_slice(bytes).ok()
    }
}

/// How a frame reaches the other nodes.
type Cross = Box<dyn Fn(Frame) -> Pin<Box<dyn Future<Output = Sent> + Send>> + Send + Sync>;

static BUS: OnceLock<(Cross, Handle)> = OnceLock::new();

/// Says how a frame reaches the rest of the cluster.
///
/// Called once, at startup, after [`keys`](crate::keys). A second call is
/// ignored rather than racing the first. Until it is called, exos is a single
/// node and builds no frames at all.
///
/// ```ignore
/// exos::bus(move |frame| {
///     let redis = redis.clone();
///
///     async move {
///         redis.publish("exos", frame.to_bytes()).await?;
///         Ok(())
///     }
/// });
/// ```
///
/// # Panics
///
/// If no signing key is configured. A random key per process is right for
/// `cargo run` and is a broken cluster: a token minted by one node verifies
/// nowhere else, and the symptom is fragments that stop updating after a
/// reconnect, which reads as a network glitch. Registering a bus is the moment
/// exos can know that rather than warn about it, so
/// [`keys`](crate::keys) goes first.
///
/// And if called from outside a tokio runtime. A publish is synchronous and an
/// adapter is not, so the handle to spawn on is taken here, where an
/// application registering the bus from the wrong place finds out at startup
/// rather than from a delivery that silently never left.
pub fn bus<F, U>(cross: F)
where
    F: Fn(Frame) -> U + Send + Sync + 'static,
    U: Future<Output = Sent> + Send + 'static,
{
    assert!(
        crate::keys::configured_by_hand(),
        "a cluster signs with one key, so exos::keys goes before exos::bus; \
         a random key per process is a token that verifies on one node"
    );

    let boxed: Cross = Box::new(move |frame| Box::pin(cross(frame)));

    let handle = Handle::try_current()
        .expect("a bus is registered from inside the runtime that will carry its sends");

    drop(BUS.set((boxed, handle)));
}

/// Hands `frame` to the bus, where there is one.
///
/// The frame is built here rather than by the caller, so that an application
/// running one node never pays for one. Local delivery has already happened by
/// the time this is called: a bus outage should cost a cluster its cross-node
/// liveness and not its liveness, and the failure that gives is the one exos
/// already survives.
pub(crate) fn cross(frame: impl FnOnce() -> Frame) {
    let Some((cross, handle)) = BUS.get() else {
        return;
    };

    let sending = cross(frame());

    // The runtime doing the publishing, where there is one, and the one the bus
    // was registered from otherwise: a publish from a background thread has no
    // current handle and still has somewhere to send.
    let handle = Handle::try_current().unwrap_or_else(|_| handle.clone());

    handle.spawn(async move {
        if let Err(error) = sending.await {
            eprintln!("exos: a frame did not reach the bus: {error}");
        }
    });
}

/// Delivers a frame from another node to this one's connections.
///
/// What an application's subscriber loop calls for every message the broker
/// hands it. Synchronous, because delivery is a registry walk and a channel
/// send, which is what [`publish`](crate::publish) already is.
///
/// A frame addressed to nothing this node holds is free and silent, which is
/// most of them: every node receives every frame until a broker is doing the
/// filtering.
///
/// # Panics
///
/// If the registry lock was poisoned; see
/// [`connection_count`](crate::connection_count).
#[expect(
    clippy::needless_pass_by_value,
    reason = "a delivery is a handover, and a frame an adapter kept would be one it delivered twice"
)]
pub fn deliver(frame: Frame) {
    crate::live::stream::dispatch(frame.kind(), frame.key(), frame.steps());
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

    fn frame() -> Frame {
        Frame::new(
            Kind::Topic,
            "presence-1a2b3c4d",
            vec![(String::from("patch"), String::from("<li id=\"a\">hi</li>"))],
        )
    }

    /// A golden value rather than a round trip, because a round trip passes
    /// against a codec that drifts and the whole point of this one being ours
    /// is that two binaries spell it the same way.
    #[test]
    fn a_frame_is_the_same_bytes_in_every_build() {
        assert_eq!(
            String::from_utf8(frame().to_bytes()).expect("UTF-8"),
            r#"{"kind":"topic","key":"presence-1a2b3c4d","steps":[["patch","<li id=\"a\">hi</li>"]],"trace":""}"#
        );
    }

    #[test]
    fn a_frame_survives_the_crossing() {
        assert_eq!(Frame::from_bytes(&frame().to_bytes()), Some(frame()));
    }

    /// A build that has never heard of a kind ignores the frame rather than
    /// failing, which is what lets a cluster roll over one node at a time.
    #[test]
    fn a_frame_this_build_does_not_understand_is_dropped() {
        let later = br#"{"kind":"session","key":"k","steps":[],"trace":""}"#;

        assert_eq!(Frame::from_bytes(later), None);
        assert_eq!(Frame::from_bytes(b"not a frame at all"), None);
    }

    /// Nothing is built where there is nowhere to send it, which is the whole
    /// of what a single-node application pays for this module.
    #[test]
    fn with_no_bus_registered_no_frame_is_built() {
        let mut built = false;

        cross(|| {
            built = true;
            frame()
        });

        assert!(!built, "the frame was built for a bus nobody registered");
    }
}
