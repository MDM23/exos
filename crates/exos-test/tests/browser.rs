//! The browser against a real application, which is the only way to check it.
//!
//! Everything here is written the way an application's own tests would be
//! written, because that is what this crate exists to make possible: no
//! request builders, no cookie handling, no subscription handshake.
//!
//! Fragments are named per test. The registry is process-wide and cargo runs
//! these in parallel, so a shared topic would have every test watching every
//! other test's publishes.

use axum::{Router, extract::Path, http::StatusCode};
use exos::{Effect, Markup, Model, Page, Step, publish, view};
use exos_test::Browser;
use serde::{Deserialize, Serialize};

#[exos::model]
#[derive(Debug, Default, Deserialize, Serialize)]
struct Draft {
    error: String,
    title: String,
}

#[exos::live]
fn board(id: u32) -> Markup {
    view! { <p>{ format!("board {id}") }</p> }
}

#[exos::get("/boards/{id}")]
async fn show(Path(id): Path<u32>) -> Page {
    Page(view! {
        <!DOCTYPE html>
        <html>
            <body>{ board(id) }</body>
        </html>
    })
}

#[exos::post("/boards/{id}/touch")]
async fn touch(Path(id): Path<u32>) -> Effect {
    publish(board(id));
    Effect::none()
}

/// A whole document handed over by an action, which is a navigation that saved
/// a fetch.
#[exos::post("/boards/{id}/handover")]
async fn handover(Path(id): Path<u32>) -> Effect {
    Effect::page(view! {
        <!DOCTYPE html>
        <html>
            <body>{ board(id) }</body>
        </html>
    })
}

/// And a navigation that did not.
#[exos::post("/boards/{id}/elsewhere")]
async fn elsewhere(Path(id): Path<u32>) -> Effect {
    Effect::navigate(format!("/boards/{id}"))
}

/// A session replaced, which ends every stream the browser had open so that it
/// comes back as whoever it is now. The application answers with a reload; what
/// matters here is that the stream went.
#[exos::post("/rotate")]
async fn rotate() -> Effect {
    drop(exos::session().rotate());
    Effect::reload()
}

#[exos::post("/drafts")]
async fn save(Model(draft): Model<Draft>) -> Result<Effect, (StatusCode, Effect)> {
    if draft.title.trim().is_empty() {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Effect::set(&Draft::signals().error, String::from("A title is needed."))
                .focus("#title"),
        ));
    }

    Ok(Effect::patch(view! { <li id="drafts">{ &draft.title }</li> }).scroll("#drafts"))
}

fn app() -> Router {
    exos::app().into()
}

#[tokio::test]
async fn a_page_is_served_and_read_as_it_arrived() {
    let mut tab = Browser::new(app());
    let page = tab.get("/boards/1").await;

    assert_eq!(page.status(), StatusCode::OK);
    assert!(page.body().contains("board 1"), "{}", page.body());
}

/// The handshake, which is the whole reason this is a browser and not a
/// request helper: rendering a fragment mints a token bound to a session, the
/// session arrives as a cookie, and the tab says what it is watching under it.
#[tokio::test]
async fn opening_a_page_subscribes_to_the_fragments_on_it() {
    let mut tab = Browser::new(app());
    tab.get("/boards/2").await;

    assert_eq!(tab.watching(), [board(2).topic().clone()]);
    assert!(tab.cookie("exos").is_some(), "a session was started");
}

/// The half an application cannot test without holding a stream open: a
/// publish from somewhere else entirely, arriving at the tab watching it.
#[tokio::test]
async fn a_publish_reaches_the_tab_watching_it() {
    let mut tab = Browser::new(app());
    tab.get("/boards/3").await;

    let answer = tab.call("POST", "/boards/3/touch").await;
    assert_eq!(answer.status(), StatusCode::OK);

    let Step::Patch(markup) = tab.next().await else {
        panic!("a publish arrives as a patch")
    };

    assert!(markup.as_str().contains("board 3"), "{markup}");
}

/// And only the tab watching it. Watching nothing is watching nothing in
/// particular, which is the bug a broadcast makes easy.
#[tokio::test]
async fn a_publish_of_another_topic_does_not() {
    let mut tab = Browser::new(app());
    tab.get("/boards/4").await;

    tab.call("POST", "/boards/5/touch").await;
    tab.call("POST", "/boards/4/touch").await;

    // Ordering rather than a timer: what proves the first did not arrive is
    // the second arriving first.
    let Step::Patch(markup) = tab.next().await else {
        panic!("a publish arrives as a patch")
    };

    assert!(markup.as_str().contains("board 4"), "{markup}");
}

/// A model posted, and the bound that makes it one: this does not compile
/// against a struct the `#[model]` attribute was not put on.
#[tokio::test]
async fn an_action_answers_with_steps() {
    let mut tab = Browser::new(app());

    let answer = tab
        .post(
            "/drafts",
            &Draft {
                title: String::from("Q3 planning"),
                ..Draft::default()
            },
        )
        .await;

    assert_eq!(answer.status(), StatusCode::OK);
    assert_eq!(
        answer.steps(),
        [
            Step::Patch(Markup(String::from("<li id=\"drafts\">Q3 planning</li>"))),
            Step::Scroll(String::from("#drafts")),
        ]
    );
}

/// A refusal says no and says what to do about it, and both halves are
/// readable from one answer.
#[tokio::test]
async fn a_refusal_carries_a_status_and_an_effect() {
    let mut tab = Browser::new(app());
    let answer = tab.post("/drafts", &Draft::default()).await;

    assert_eq!(answer.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(answer.steps().len(), 2);
    assert!(matches!(answer.steps()[0], Step::Signals(_)));
    assert_eq!(answer.steps()[1], Step::Focus(String::from("#title")));
}

/// The header the runtime sends goes on everything, so an action is served
/// rather than refused as somebody else's page asking.
#[tokio::test]
async fn an_action_is_not_refused_as_a_forgery() {
    let mut tab = Browser::new(app());

    assert_ne!(
        tab.call("POST", "/boards/6/touch").await.status(),
        StatusCode::FORBIDDEN
    );
}

/// Navigating away unsubscribes by not mentioning the topic again.
#[tokio::test]
async fn leaving_a_page_stops_watching_what_was_on_it() {
    let mut tab = Browser::new(app());

    tab.get("/boards/7").await;
    tab.get("/boards/8").await;

    assert_eq!(tab.watching(), [board(8).topic().clone()]);
}

/// A document replaces the document, whichever of the two ways it arrives, so
/// the fragments on it are the whole of what is watched afterwards.
#[tokio::test]
async fn a_page_handed_over_by_an_action_replaces_what_is_watched() {
    let mut tab = Browser::new(app());
    tab.get("/boards/9").await;

    tab.call("POST", "/boards/10/handover").await;

    assert_eq!(tab.watching(), [board(10).topic().clone()]);
}

/// A tab on its way somewhere else is watching nothing in the meantime, and
/// says so: the set is replaced wholesale, and an empty one is what stops the
/// page it left being patched into the page it is going to.
#[tokio::test]
async fn a_navigation_stops_watching_the_page_it_left() {
    let mut tab = Browser::new(app());
    tab.get("/boards/11").await;

    tab.call("POST", "/boards/11/elsewhere").await;

    assert!(tab.watching().is_empty());

    // The server was told, rather than the tab merely having forgotten. What
    // proves it is the ordering: the publish below reaches nobody, so the one
    // after it, for the page the tab went on to, is the first thing to arrive.
    // A tab still subscribed would be handed this one instead.
    tab.call("POST", "/boards/11/touch").await;

    tab.get("/boards/12").await;
    tab.call("POST", "/boards/12/touch").await;

    let Step::Patch(markup) = tab.next().await else {
        panic!("a publish arrives as a patch")
    };

    assert!(markup.as_str().contains("board 12"), "{markup}");
}

/// A tab whose stream the server ended opens another, rather than the
/// subscription that follows being an error.
///
/// The tokens it was holding were minted for the session it has left, so they
/// prove nothing and are dropped: the page arriving again is what brings
/// tokens the new session owns, which is why the application answers a
/// rotation with a reload.
#[tokio::test]
async fn a_rotation_ends_the_stream_and_the_tab_opens_another() {
    let mut tab = Browser::new(app());
    tab.get("/boards/14").await;

    let before = tab.cookie("exos").expect("a session").to_owned();

    assert_eq!(tab.call("POST", "/rotate").await.steps(), [Step::Reload]);
    assert_ne!(tab.cookie("exos"), Some(before.as_str()));

    // What the reload does, and the tab is watching again on the far side of
    // it: a publish reaches it, which it could not while its grant was stale.
    tab.get("/boards/14").await;
    tab.call("POST", "/boards/14/touch").await;

    let Step::Patch(markup) = tab.next().await else {
        panic!("a publish arrives as a patch")
    };

    assert!(markup.as_str().contains("board 14"), "{markup}");
}
