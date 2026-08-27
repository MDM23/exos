//! Who a stream belongs to, through the whole stack.
//!
//! exos names the browser and the application says what the name stands for.
//! That division is the same one [`session`](../src/session.rs) draws, seen
//! from the other side: a resolver runs once when a stream opens, turns the
//! name into audiences, and the connection carries them for as long as it
//! lives.
//!
//! Everything called `Sessions` below is the application's. This file is the
//! worked example of the arrangement as much as it is the test of it.

use std::{
    collections::HashMap,
    sync::{Mutex, MutexGuard},
};

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
    response::Response,
};
use exos::{Audience, Audiences, Id, connected, data};
use tower::ServiceExt as _;

/// Who a name stands for, which exos never learns.
#[derive(Clone, Copy)]
struct Who {
    id: u32,
    team: u32,
}

/// What an application keeps under a session name. A `HashMap` here, a table
/// with an index and an expiry job in anything real.
#[derive(Default)]
struct Sessions(Mutex<HashMap<Id, Who>>);

impl Sessions {
    fn bind(&self, id: &Id, who: Who) {
        self.lock().insert(id.clone(), who);
    }

    /// Async because resolving a name is a database call in anything real, and
    /// that is the reason the resolver awaits.
    async fn viewer(&self, id: &Id) -> Option<Who> {
        self.lock().get(id).copied()
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<Id, Who>> {
        self.0.lock().expect("the lock is not poisoned")
    }
}

#[derive(Hash)]
struct Viewer(u32);

impl Audience for Viewer {
    const NAME: &'static str = "viewer";
}

#[derive(Hash)]
struct Team(u32);

impl Audience for Team {
    const NAME: &'static str = "team";
}

/// A visitor with a name and nobody behind it, which is what a queue position
/// or a checkout timer is addressed to.
#[derive(Hash)]
struct Visitor(Id);

impl Audience for Visitor {
    const NAME: &'static str = "visitor";
}

/// The name that stands in for a database that is not answering.
const UNREACHABLE: &str = "ffffffffffffffffffffffffffffffff";

/// exos's half of signing in, which is the rotation. What a name means is the
/// application's; what a rotation does to the streams it renamed is exos's.
#[exos::post("/sign-in")]
async fn sign_in() -> StatusCode {
    let id = exos::session().rotate();

    data::<Sessions>().bind(&id, Who { id: 8, team: 80 });

    StatusCode::NO_CONTENT
}

/// The application: the sessions it holds, and who a name stands for.
///
/// Built per call, which the builder allows: the same line saying the same
/// things is one application, so the store keeps what a test bound into it.
fn app() -> Router {
    exos::app()
        .provide(Sessions::default())
        .identify(async |name: Option<Id>| {
            // What anonymous means is the application's to say. A visit with no
            // name has nothing to be addressed by; a name with nobody behind it
            // has itself.
            let Some(name) = name else {
                return Ok(Audiences::none());
            };

            if name.as_str() == UNREACHABLE {
                return Err("the sessions table is unreachable".into());
            }

            Ok(match data::<Sessions>().viewer(&name).await {
                Some(who) => Audiences::of(&Viewer(who.id)).and(&Team(who.team)),
                None => Audiences::of(&Visitor(name)),
            })
        })
        .into()
}

/// A name the application has already bound to `who`.
fn known(who: Who) -> Id {
    drop(app());

    let id = Id::random();
    data::<Sessions>().bind(&id, who);

    id
}

/// Opens one tab's stream, with the cookie a browser holding `session` sends.
///
/// The response is handed back rather than dropped, because dropping it closes
/// the connection and there would be nothing left to ask about.
async fn opened(session: Option<&str>) -> Response {
    let mut builder = Request::builder().method("GET").uri("/_exos/live");

    if let Some(session) = session {
        builder = builder.header(header::COOKIE, format!("theme=dark; exos={session}"));
    }

    app()
        .oneshot(builder.body(Body::empty()).expect("a valid request"))
        .await
        .expect("the router answers")
}

/// The point of the whole arrangement: the cookie names a session, the
/// application says who that is, and the connection carries it.
#[tokio::test]
async fn a_stream_is_addressable_as_whoever_opened_it() {
    let id = known(Who { id: 1, team: 10 });

    let stream = opened(Some(id.as_str())).await;
    assert_eq!(stream.status(), StatusCode::OK);

    assert!(connected(&Viewer(1)));
    assert!(connected(&Team(10)), "and as every audience they are in");
    assert!(!connected(&Viewer(999)), "and as nobody else");
}

/// A resolver answering with a set rather than one value is what makes this a
/// property of the design rather than something to build later.
#[tokio::test]
async fn one_audience_can_hold_more_than_one_viewer() {
    let first = known(Who { id: 2, team: 20 });
    let second = known(Who { id: 3, team: 20 });

    let first = opened(Some(first.as_str())).await;
    let second = opened(Some(second.as_str())).await;

    assert!(connected(&Team(20)));

    drop(first);
    assert!(connected(&Team(20)), "the other member is still there");

    drop(second);
    assert!(!connected(&Team(20)));
}

/// Every tab is its own connection, so identity is per stream and the last one
/// to close is what ends it.
#[tokio::test]
async fn a_viewer_stays_reachable_while_any_tab_is_open() {
    let id = known(Who { id: 4, team: 40 });

    let tab = opened(Some(id.as_str())).await;
    let other = opened(Some(id.as_str())).await;

    drop(tab);
    assert!(connected(&Viewer(4)), "one tab left");

    drop(other);
    assert!(!connected(&Viewer(4)));
}

/// A visit that never asked for a name has nothing to key an audience on, and
/// the stream opens anyway: fragments on the page still update, because a
/// subscription is proved rather than identified.
#[tokio::test]
async fn a_stream_with_no_name_behind_it_still_opens() {
    let stream = opened(None).await;

    assert_eq!(stream.status(), StatusCode::OK);
}

/// Anonymous is not a case exos decides. A name with nobody behind it is still
/// a name, and whether it is worth addressing is the resolver's answer.
#[tokio::test]
async fn a_name_with_nobody_behind_it_is_addressable_as_itself() {
    let id = Id::random();
    let stream = opened(Some(id.as_str())).await;

    assert_eq!(stream.status(), StatusCode::OK);
    assert!(connected(&Visitor(id.clone())));
    assert!(!connected(&Viewer(0)), "and is nobody in particular");

    drop(stream);
    assert!(!connected(&Visitor(id)));
}

/// A rotation reaches the streams the old name opened, and it has to reach
/// them by ending them.
///
/// Those tabs made no request, so there is nothing to answer them with, and
/// correcting a connection in place would carry it across the boundary
/// rotation exists to draw: a stolen cookie with a stream open would be
/// upgraded to the new identity instead of cut off by it.
#[tokio::test]
async fn rotating_a_name_ends_the_streams_that_carried_it() {
    let id = known(Who { id: 7, team: 70 });

    let stream = opened(Some(id.as_str())).await;
    assert!(connected(&Viewer(7)));

    let response = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/sign-in")
                .header(header::COOKIE, format!("exos={id}"))
                .body(Body::empty())
                .expect("a valid request"),
        )
        .await
        .expect("the router answers");

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert!(
        !connected(&Viewer(7)),
        "the old identity holds no stream any more"
    );

    // The response held it open until now, which is what makes the assertion
    // above about the rotation rather than about the tab having gone away.
    drop(stream);
}

/// Refusing is the loud version of what opening anyway would do silently: a tab
/// with no audiences receives nothing and never says so. `EventSource` retries
/// on its own, so the cost of refusing a stream is a delay.
#[tokio::test]
async fn a_resolver_that_fails_refuses_the_stream() {
    let stream = opened(Some(UNREACHABLE)).await;

    assert_eq!(stream.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

/// A stream must never start a session, because its response headers go out
/// when it opens and there is no second chance to set the cookie. Reading a
/// name it was not given would be the only way to owe one.
#[tokio::test]
async fn opening_a_stream_never_names_the_browser() {
    let stream = opened(None).await;

    assert!(stream.headers().get(header::SET_COOKIE).is_none());
}
