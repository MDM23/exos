//! The exos example: a directory with selection, optimistic updates,
//! drag to reorder and live presence.
//!
//! Everything the browser does is written in Rust here: signal reads and
//! writes, event handlers and calls to the server are typed values rather than
//! strings that have to keep agreeing with something elsewhere.
//!
//! Run it with `cargo run -p files` and open <http://localhost:3000> twice.

use axum::{Json, extract::Path};
use exos::{Effect, Page, data, on_change, publish, view};
use serde::{Deserialize, Serialize};

mod store;
mod view_model;

use crate::{
    store::{Files, Presence},
    view_model::{layout, row, selection_bar},
};

// -----------------------------------------------------------------------------
//                                  ENTRY POINT
// -----------------------------------------------------------------------------

/// The port the example listens on.
const ADDRESS: &str = "127.0.0.1:3000";

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    boot();
    simulate_presence();

    let listener = tokio::net::TcpListener::bind(ADDRESS).await?;
    println!("listening on http://{ADDRESS}");

    axum::serve(listener, exos::app()).await
}

/// Seeds the application data. Separate from `main` so tests can call it.
fn boot() {
    exos::provide(Files::seed());
    exos::provide(Presence::seed());
}

/// Flips presence on a timer, so the example has something to watch.
fn simulate_presence() {
    tokio::spawn(async {
        let mut user = 1_u32;

        loop {
            tokio::time::sleep(core::time::Duration::from_secs(3)).await;
            data::<Presence>().toggle(user);
            publish(&presence(user));
            user = user % 3 + 1;
        }
    });
}

// -----------------------------------------------------------------------------
//                                    MODELS
// -----------------------------------------------------------------------------

/// What the selection bar holds and what a batch action sends.
///
/// Declared once: the handlers below take `Json<Selection>` and the template
/// binds `selection.picked`, so renaming a field breaks both.
#[exos::model]
#[derive(Debug, Default, Deserialize, Serialize)]
pub(crate) struct Selection {
    /// The rows the viewer checked.
    pub(crate) picked: Vec<u32>,
    /// The simulate-a-failure switch, so the example can show a rejected
    /// action correcting the optimistic paint.
    pub(crate) fail: bool,
}

/// What the sortable plugin sends when a drag ends.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub(crate) struct Reorder {
    /// The row ids, in the order the viewer dragged them into.
    pub(crate) order: Vec<String>,
}

// -----------------------------------------------------------------------------
//                                LIVE FRAGMENTS
// -----------------------------------------------------------------------------

/// One user's presence dot, which follows the server on its own.
#[exos::live]
pub(crate) fn presence(user: u32) -> exos::Markup {
    let online = data::<Presence>().online(user);

    view! {
        <span
            class="dot"
            data-online={ online }
            title={ if online { "online" } else { "away" } }
        ></span>
    }
}

/// The whole list, as one live fragment.
#[exos::live]
pub(crate) fn file_list() -> exos::Markup {
    let entries = data::<Files>().snapshot();
    let selection = Selection::signals();

    view! {
        <ul
            id="file-list"
            class="file-list"
            role="list"
            data-sortable="post('/files/reorder', { order: $._order })"
        >
            { entries.iter().map(|entry| row(entry, &selection)).collect::<Vec<_>>() }
        </ul>
    }
}

// -----------------------------------------------------------------------------
//                                     PAGES
// -----------------------------------------------------------------------------

#[exos::get("/")]
async fn index() -> Page {
    let selection = Selection::signals();

    layout(
        "Files",
        "/",
        view! {
            <h1>"Files"</h1>
            <p class="lede">
                "Check rows to reveal the selection bar. Favourite and delete paint
                 instantly. Drag a handle to reorder. The dots are live fragments.
                 Open a second tab."
            </p>

            // One scope over the bar and the list, so they share the selection.
            <section {&selection}>
                <label class="controls">
                    <input
                        type="checkbox"
                        {on_change(|event| selection.fail.set(event.target().checked()))}
                    >
                    "Simulate a server error"
                </label>

                { selection_bar(&selection) }
                { file_list() }
            </section>
        },
    )
}

#[exos::get("/about")]
async fn about() -> Page {
    layout(
        "About",
        "/about",
        view! {
            <h1>"About"</h1>
            <p class="lede">
                "A different document. Getting here morphed the body instead of loading
                 it, so the live connection was never dropped."
            </p>
            <p><a href="/">"Back to the files"</a></p>
        },
    )
}

// -----------------------------------------------------------------------------
//                                   ACTIONS
// -----------------------------------------------------------------------------

#[exos::post("/files/{id}/favorite")]
async fn favorite(Path(id): Path<u32>, Json(selection): Json<Selection>) -> Effect {
    if !selection.fail {
        data::<Files>().update(|entries| {
            store::toggle_favorite(entries, id);
        });
    }

    // Published either way. On success this confirms the speculative write, on
    // failure it undoes it, and the handler does not have to know which.
    publish(&file_list());
    Effect::none()
}

#[exos::post("/files/{id}/delete")]
async fn delete_file(Path(id): Path<u32>, Json(selection): Json<Selection>) -> Effect {
    if !selection.fail {
        data::<Files>().update(|entries| store::delete(entries, id));
    }

    publish(&file_list());
    Effect::none()
}

#[exos::post("/files/archive")]
async fn archive(Json(selection): Json<Selection>) -> Effect {
    if !selection.fail {
        data::<Files>().update(|entries| store::archive(entries, &selection.picked));
    }

    publish(&file_list());

    // Whether the batch applied or not, the selection no longer refers to
    // anything the viewer can see.
    Effect::signals(serde_json::json!({ "picked": [] })).scroll("#file-list")
}

#[exos::post("/files/reorder")]
async fn reorder(Json(body): Json<Reorder>) -> Effect {
    data::<Files>().update(|entries| store::reorder(entries, &body.order));

    publish(&file_list());
    Effect::none()
}

#[exos::post("/users/{id}/presence")]
async fn toggle_presence(Path(id): Path<u32>) -> Effect {
    data::<Presence>().toggle(id);

    // One line, and every tab showing that user's dot updates, whether it is
    // showing one file of theirs or twenty.
    publish(&presence(id));
    Effect::none()
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
    use axum::{
        Router,
        body::Body,
        http::{Request, StatusCode},
    };
    use std::sync::Once;
    use tower::ServiceExt as _;

    /// The application data is global, so these tests only ever read it. Every
    /// operation that changes the directory is a free function over a slice
    /// and is tested in [`store`] against a local `Vec`, which keeps those
    /// tests independent of each other and of their order.
    fn app() -> Router {
        static BOOT: Once = Once::new();
        BOOT.call_once(boot);

        exos::app()
    }

    async fn body(response: axum::response::Response) -> String {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("the body is readable");

        String::from_utf8(bytes.to_vec()).expect("the body is UTF-8")
    }

    async fn get(uri: &str) -> String {
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

    #[tokio::test]
    async fn the_page_ships_one_stylesheet_and_one_script() {
        let html = get("/").await;

        assert!(html.starts_with("<!DOCTYPE html>"));
        assert_eq!(html.matches("rel=\"stylesheet\"").count(), 1);
        assert_eq!(html.matches("<script").count(), 1);
    }

    #[tokio::test]
    async fn handlers_compile_to_javascript() {
        let html = get("/").await;

        // Written in Rust as a signal write followed by a typed call.
        assert!(
            html.contains("$._gone = true; post(&quot;/files/1/delete&quot;"),
            "{html:.900}"
        );
    }

    #[tokio::test]
    async fn a_typed_call_sends_exactly_the_model_fields() {
        let html = get("/").await;

        // The handle compiled into an object of signal reads: the same fields
        // Json<Selection> deserializes.
        assert!(html.contains("&quot;picked&quot;: $.picked"));
        assert!(html.contains("&quot;fail&quot;: $.fail"));
    }

    #[tokio::test]
    async fn the_selection_bar_derives_from_the_signal() {
        let html = get("/").await;

        assert!(html.contains("data-show=\"$.picked.length &gt; 0\""));
        assert!(html.contains("data-text=\"$.picked.length\""));
        assert!(html.contains("data-bind=\"picked\""));
    }

    #[tokio::test]
    async fn favourite_state_has_one_source_of_truth() {
        let html = get("/").await;

        // The server renders data-favorite and the click writes it
        // speculatively. No binding recomputes it from a signal, which is what
        // would let the two drift apart.
        assert!(html.contains("data-favorite=\""));
        assert!(!html.contains("data-attr="));
        assert!(html.contains("attr(&quot;data-favorite&quot;"));
    }

    #[tokio::test]
    async fn a_live_fragment_carries_a_server_chosen_topic() {
        let html = get("/").await;

        assert!(html.contains("<exos-live style=\"display:contents\" id=\"live-presence-"));
        assert!(html.contains("data-token=\""));
    }

    #[tokio::test]
    async fn interpolated_values_are_escaped() {
        let selection = Selection::signals();
        let entry = store::Entry {
            id: 9,
            name: String::from(r#"<img src=x onerror="alert(1)">"#),
            favorite: false,
            owner: 1,
        };

        let html = row(&entry, &selection).into_string();

        assert!(html.contains("&lt;img src=x onerror=&quot;alert(1)&quot;&gt;"));
        assert!(!html.contains("<img"));
    }

    #[tokio::test]
    async fn an_effect_answers_as_an_event_stream() {
        let response = app()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/files/archive")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"picked":[],"fail":true}"#))
                    .expect("a valid request"),
            )
            .await
            .expect("the router answers");

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get("content-type")
                .expect("a content type"),
            "text/event-stream"
        );

        let stream = body(response).await;
        assert!(stream.contains("event: signals"));
        assert!(stream.contains("event: scroll"));
    }
}
