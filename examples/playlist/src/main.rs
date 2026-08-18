//! A shared listening room: one queue, several browsers, no polling.
//!
//! Everything the browser does is written in Rust here: signal reads and
//! writes, event handlers and calls to the server are typed values rather than
//! strings that have to keep agreeing with something elsewhere.
//!
//! What it covers, and each of them arises from the room rather than being
//! arranged for:
//!
//! * **The track changes on its own.** A clock moves the mark on and
//!   publishes, so every tab follows something no one of them did. The list
//!   itself never moves: what is playing is a mark on a row rather than a
//!   position in the queue, so only a drag ever reorders anything.
//! * **Optimistic updates.** A heart fills under the pointer and a removed row
//!   goes at once, before the server has answered.
//! * **A correction that is not simulated.** The room will not remove what it
//!   is playing. Ask it to, and watch the row come back. That is a rule the
//!   room has, so a listener can trigger the correction deliberately and
//!   understand it, which is what a switch labelled "simulate a server error"
//!   never manages.
//! * **Selection with a batch action**, ticking rows into one `Vec<u32>`.
//! * **Drag to reorder**, deciding what plays next.
//! * **A real listener count**, from the streams exos is already holding.
//!
//! Run it with `cargo run -p playlist` and open <http://localhost:3000> twice.

use core::time::Duration;

mod page;
mod room;
mod selection;
mod store;
mod track;

use crate::{room::publish_room, store::Room};

/// The address the example listens on.
const ADDRESS: &str = "127.0.0.1:3000";

/// How long each track plays.
///
/// Short, because the point of it is to be seen changing rather than to be
/// listened to.
const TRACK_TIME: Duration = Duration::from_secs(15);

/// How often the clock looks up.
const TICK: Duration = Duration::from_secs(1);

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    boot();
    play();

    let listener = tokio::net::TcpListener::bind(ADDRESS).await?;
    println!("listening on http://{ADDRESS}");

    axum::serve(listener, exos::app()).await
}

/// Seeds the room. Separate from `main` so tests can call it.
fn boot() {
    exos::provide(Room::seed());
}

/// Moves the mark on, and keeps the listener count honest.
///
/// The one background job in the example, and it publishes for two reasons
/// rather than one. A track ending is the room changing, and a tab opening or
/// closing changes what the room says about itself, and neither is anything a
/// request could answer.
fn play() {
    tokio::spawn(async {
        let mut elapsed = Duration::ZERO;
        let mut listeners = exos::connection_count();

        loop {
            tokio::time::sleep(TICK).await;
            elapsed += TICK;

            let over = elapsed >= TRACK_TIME;
            let now = exos::connection_count();

            if over {
                elapsed = Duration::ZERO;
                exos::data::<Room>().update(store::advance);
            }

            // Nothing to say when nothing moved, which is most seconds.
            if over || now != listeners {
                listeners = now;
                publish_room();
            }
        }
    });
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "a failing assertion is the point of a test"
)]
mod tests {
    use std::sync::Once;

    use axum::{
        Router,
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt as _;

    use super::*;

    /// The room is global, so these tests only ever read it through the
    /// router. Every operation that changes the queue is a free function over
    /// a slice and is tested in [`store`] against a local `Vec`, which keeps
    /// those tests independent of each other and of their order.
    pub(crate) fn seeded() {
        static BOOT: Once = Once::new();
        BOOT.call_once(boot);
    }

    fn app() -> Router {
        seeded();
        exos::app()
    }

    async fn body(response: axum::response::Response) -> String {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("the body is readable");

        String::from_utf8(bytes.to_vec()).expect("the body is UTF-8")
    }

    pub(crate) async fn get(uri: &str) -> String {
        let response = app()
            .oneshot(
                Request::builder()
                    .uri(uri)
                    .body(Body::empty())
                    .expect("a valid request"),
            )
            .await
            .expect("the router answers");

        assert_eq!(response.status(), StatusCode::OK);
        body(response).await
    }

    pub(crate) async fn post(uri: &str, payload: &str) -> String {
        let response = app()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(uri)
                    .header("content-type", "application/json")
                    .body(Body::from(payload.to_owned()))
                    .expect("a valid request"),
            )
            .await
            .expect("the router answers");

        assert_eq!(response.status(), StatusCode::OK);
        body(response).await
    }

    #[tokio::test]
    async fn handlers_compile_to_javascript() {
        let html = get("/").await;
        let going = crate::selection::Selection::signals().going;

        // A heart is a speculative attribute write followed by a typed call.
        // Nothing mirrors it into a signal, so the assertion names the shape
        // rather than a generated key.
        assert!(
            html.contains("attr(&quot;data-hearted&quot;"),
            "{html:.900}"
        );

        // A removal writes the model instead, because the server can refuse it
        // and therefore has to be able to undo it.
        assert!(
            html.contains(&format!("$.{0} = [...$.{0}, ", going.name())),
            "{html:.1200}"
        );
        assert!(html.contains("post(&quot;/tracks/"), "{html:.1200}");
    }

    /// An action's body is between this server and the client it generated. A
    /// request written by hand against the field names is refused, which is
    /// what leaves the shape free to change later.
    #[tokio::test]
    async fn a_hand_written_body_does_not_reach_an_action() {
        let response = app()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/tracks/remove")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"picked":[2],"note":""}"#))
                    .expect("a valid request"),
            )
            .await
            .expect("the router answers");

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    /// The whole point of the example, end to end: the row goes, the room
    /// refuses, and the reply says so rather than leaving a listener to guess.
    ///
    /// And the row comes back where it was, not somewhere else, which is the
    /// difference between a correction and a surprise.
    #[tokio::test]
    async fn the_room_refuses_to_remove_what_it_is_playing() {
        seeded();

        let before = exos::data::<Room>().snapshot();
        let playing = before.playing().expect("something is on").clone();

        let stream = post(&format!("/tracks/{}/remove", playing.id), "").await;

        assert!(stream.contains("event: signals"));
        assert!(stream.contains("is playing, so it stayed"), "{stream}");

        // The row hid itself on the click, and nothing but this puts it back:
        // a patch re-renders the row but leaves the signals alone, so the reply
        // has to say the row is no longer on its way out. This is the bug the
        // example shipped with, and it was invisible until a refusal happened.
        let going = crate::selection::Selection::signals().going;
        assert!(
            stream.contains(&format!("\"{}\":[]", going.name())),
            "the row is shown again: {stream}"
        );

        let after = exos::data::<Room>().snapshot();

        assert_eq!(
            after.playing().expect("something is still on").id,
            playing.id
        );
        assert_eq!(
            after
                .tracks
                .iter()
                .map(|track| track.id)
                .collect::<Vec<_>>(),
            before
                .tracks
                .iter()
                .map(|track| track.id)
                .collect::<Vec<_>>(),
            "and nothing moved"
        );
    }
}
