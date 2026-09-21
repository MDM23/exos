//! A browser for testing exos applications, without a browser.
//!
//! An exos application talks to one client: the runtime exos ships, over one
//! wire format, in both directions. So the useful test double is not a request
//! helper but that client, and this is it. [`Browser`] keeps cookies, says the
//! header the runtime says, holds the live stream open, and reports the
//! fragments it has been sent, which is everything a tab does that a `oneshot`
//! does not.
//!
//! ```no_run
//! # use exos_test::Browser;
//! # use exos::Step;
//! # #[derive(Debug, Default, serde::Deserialize, serde::Serialize)]
//! # struct Selection { picked: Vec<u32> }
//! # fn app() -> axum::Router { exos::app().into() }
//! # async fn example() {
//! let mut tab = Browser::new(app());
//!
//! let page = tab.get("/tracks").await;
//! assert!(page.body().contains("Nights in White Satin"));
//!
//! let answer = tab.call("POST", "/tracks/shuffle").await;
//! assert_eq!(answer.steps(), [Step::Scroll(String::from("#queue"))]);
//!
//! // What a background job published, as the tab would receive it.
//! assert!(matches!(tab.next().await, Step::Patch(_)));
//! # }
//! ```
//!
//! # The ladder
//!
//! Reach for this on the third rung, not the first. A [`view!`](exos::view)
//! renders to a string with no server at all, and a handler is an `async fn`
//! that can be called with its arguments and its [`Effect`](exos::Effect)
//! read back through [`steps`](exos::Effect::steps). Both are faster to write
//! and faster to read than anything here. What a browser is for is what only a
//! request can answer: routing, extraction, sessions, refusals, and the live
//! stream.
//!
//! # What it is not
//!
//! It is not a DOM. Nothing here morphs markup, evaluates a binding or runs an
//! event handler, because a second implementation of the runtime would be a
//! second thing to keep true. What it models is the half of a tab the server
//! can see: what was asked for, what came back, and which fragments the tab
//! then said it was watching. Whether the runtime does the right thing with a
//! step is the runtime's own test suite, in `crates/exos/js/tests`.
//!
//! That shows up in one place. The runtime holds no set of subscriptions: it
//! reads the document after every change it applies, so a fragment goes when
//! the element does. Here a document replaces the set and a patch adds to it,
//! which is the same answer for everything except a fragment *taken away* by a
//! patch over the region holding it or by a `remove`. A tab would stop watching
//! it; this one keeps watching until the next document arrives, and receives a
//! publish the real tab would not. Nothing is lost in a test that asserts what
//! did arrive, and a test about a fragment leaving the page should load the
//! page that no longer holds it.
//!
//! # One process, one application
//!
//! [`exos::provide`] stores by type for the whole process and cargo runs a
//! crate's tests in parallel, so two browsers in one binary share application
//! data. Give each test its own types, or seed once and have tests touch
//! disjoint data; see [`exos::provide`] for the trade.

use core::time::Duration;
use std::collections::BTreeMap;

use axum::{
    Router,
    body::{Body, BodyDataStream, to_bytes},
    http::{HeaderMap, Method, Request, StatusCode, header},
};
use exos::{ModelFields, Step, Topic};
use serde::Serialize;
use tokio::time::timeout;
use tokio_stream::StreamExt as _;
use tower::ServiceExt as _;

/// How long [`Browser::next`] waits before deciding nothing is coming.
///
/// Never waited out by a passing test: a step already on the wire is returned
/// at once. This is only the difference between a test that fails and a suite
/// that hangs.
const PATIENCE: Duration = Duration::from_secs(5);

// -----------------------------------------------------------------------------
//                                 THE BROWSER
// -----------------------------------------------------------------------------

/// One open tab.
///
/// Holds what a browser holds: the application it is pointed at, the cookies
/// it has been given, the live stream it opened, and the fragments it has been
/// told about. Drop it and the stream closes, exactly as closing a tab does.
///
/// # Panics
///
/// Everywhere, and on purpose. A browser is used in tests, where an
/// application that does not answer, answers unreadably, or refuses to open a
/// stream is the failure being looked for rather than a condition to hand back
/// and have every call site unwrap.
#[derive(Debug)]
pub struct Browser {
    /// Cloned per request, because serving one consumes it.
    router: Router,
    cookies: BTreeMap<String, String>,
    /// Topic to token, for every fragment this tab has been served.
    fragments: BTreeMap<String, String>,
    /// What the last subscription said, so an unchanged set is not resent.
    /// The runtime keeps the same memo, for the same reason.
    subscribed: String,
    stream: Option<Live>,
}

#[expect(
    clippy::missing_panics_doc,
    reason = "the type says it, once: a browser is used in tests, and every \
              method panics where an application does not answer the way the \
              runtime requires"
)]
impl Browser {
    /// A tab pointed at an application, with no cookies and nothing open.
    #[must_use]
    pub fn new(router: Router) -> Self {
        Self {
            router,
            cookies: BTreeMap::new(),
            fragments: BTreeMap::new(),
            subscribed: String::new(),
            stream: None,
        }
    }

    /// Loads a page, as typing a URL or clicking a link does.
    ///
    /// A `GET` answering with a document is a navigation, and a navigation
    /// replaces the document, so this tab stops watching whatever was on the
    /// page before and starts watching what is on this one. The runtime does
    /// nothing cleverer: it unsubscribes by not mentioning a topic again.
    ///
    /// A `GET` answering with an effect is a call like any other, and lands on
    /// the page this tab is already on.
    pub async fn get(&mut self, url: &str) -> Answer {
        self.call("GET", url).await
    }

    /// An action carrying a model, which is what `#[exos::post]` and its
    /// siblings are called with.
    ///
    /// The bound is the point: only a `#[exos::model]` can be posted, so a
    /// renamed field stops the test compiling the way it stops the handler
    /// compiling.
    pub async fn post<T: ModelFields + Serialize>(&mut self, url: &str, model: &T) -> Answer {
        self.send(
            Request::builder()
                .method("POST")
                .uri(url)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(exos::to_wire(model)))
                .expect("a valid request"),
        )
        .await
    }

    /// An action carrying nothing.
    pub async fn call(&mut self, method: &str, url: &str) -> Answer {
        self.send(
            Request::builder()
                .method(method)
                .uri(url)
                .body(Body::empty())
                .expect("a valid request"),
        )
        .await
    }

    /// Any other request, for whatever the four above do not cover.
    ///
    /// The cookies and the runtime's header are added here, so a request built
    /// by hand is still made by this browser rather than by a stranger.
    pub async fn send(&mut self, request: Request<Body>) -> Answer {
        let navigable = request.method() == Method::GET;
        let answer = self.roundtrip(request).await;

        if answer.is_effect() {
            for step in answer.steps() {
                self.applied(&step);
            }
        } else if navigable && answer.is_document() {
            self.arrived(answer.body());
        }

        self.sync().await;

        answer
    }

    /// The next step this tab is sent over the live stream.
    ///
    /// Only steps: the greeting naming the connection and the keep-alives
    /// between events are the transport talking to itself, and a test that had
    /// to skip them would be a test of the transport.
    ///
    /// # Panics
    ///
    /// If nothing arrives within [`PATIENCE`], or if no stream is open, which
    /// means nothing this tab has been served held a live fragment.
    pub async fn next(&mut self) -> Step {
        let step = loop {
            let stream = self
                .stream
                .as_mut()
                .expect("this tab has opened no stream; no page it loaded held a live fragment");

            let event = stream.event().await;

            if let Some(step) = Step::from_frame(&event.name, &event.data) {
                break step;
            }
        };

        self.applied(&step);
        self.sync().await;

        step
    }

    /// The fragments this tab has told the server it is watching.
    ///
    /// Compares against what a fragment calls itself:
    /// `assert!(tab.watching().contains(presence(7).topic()))`.
    #[must_use]
    pub fn watching(&self) -> Vec<Topic> {
        self.fragments
            .keys()
            .map(|id| Topic::from_raw(id))
            .collect()
    }

    /// The value of a cookie this browser is holding, which is how a session
    /// that started, rotated or ended is seen from the outside.
    #[must_use]
    pub fn cookie(&self, name: &str) -> Option<&str> {
        self.cookies.get(name).map(String::as_str)
    }

    /// A request as this browser makes it: the header the runtime says on
    /// every call, and whatever cookies it has been handed since.
    fn dressed(&self, mut request: Request<Body>) -> Request<Body> {
        // What an unsafe method is refused without; see the `csrf` module in
        // exos.
        request
            .headers_mut()
            .insert("x-exos", "true".parse().expect("a header value"));

        if !self.cookies.is_empty() {
            let jar = self
                .cookies
                .iter()
                .map(|(name, value)| format!("{name}={value}"))
                .collect::<Vec<_>>()
                .join("; ");

            request.headers_mut().insert(
                header::COOKIE,
                jar.parse().expect("a cookie jar is a header value"),
            );
        }

        request
    }

    /// One request, with nothing read out of the answer.
    ///
    /// Everything a browser attaches goes on here, and nothing it learns comes
    /// off here, which is what keeps [`sync`](Self::sync) from recursing
    /// through its own subscription.
    async fn roundtrip(&mut self, request: Request<Body>) -> Answer {
        let request = self.dressed(request);

        let response = self
            .router
            .clone()
            .oneshot(request)
            .await
            .expect("the router answers");

        let (parts, body) = response.into_parts();

        for cookie in parts.headers.get_all(header::SET_COOKIE) {
            let Ok(cookie) = cookie.to_str() else {
                continue;
            };
            let pair = cookie.split(';').next().unwrap_or(cookie);

            if let Some((name, value)) = pair.split_once('=') {
                self.cookies
                    .insert(name.trim().to_owned(), value.trim().to_owned());
            }
        }

        let bytes = to_bytes(body, usize::MAX).await.expect("the body is whole");

        Answer {
            status: parts.status,
            headers: parts.headers,
            body: String::from_utf8(bytes.to_vec()).expect("an answer is text"),
        }
    }

    /// What one step does to the set this tab is watching.
    ///
    /// The runtime holds no set of its own: it reads the document after every
    /// change it applies, so what a step does here is whatever it does to that
    /// document. A patch morphs into the page and can bring a fragment the
    /// page had not seen; the two whole-document steps replace the page, and
    /// with it everything that was being watched.
    ///
    /// The one thing a step can do that this cannot follow is take a fragment
    /// away: a patch over a region holding one, or a `remove` of it, needs the
    /// document to say what is left. See the crate docs.
    fn applied(&mut self, step: &Step) {
        match step {
            Step::Patch(markup) => self.learn(markup.as_str()),
            Step::Page(markup) => self.arrived(markup.as_str()),

            // The tab is leaving, and the runtime fetches what it named. This
            // does not, because a request nobody in the test asked for is
            // worse than a test that says where it went next: open the URL the
            // step carries.
            Step::Navigate(_) | Step::Reload => self.fragments.clear(),

            _ => {}
        }
    }

    /// A whole document, which is the only thing that decides the set outright.
    fn arrived(&mut self, document: &str) {
        self.fragments.clear();
        self.learn(document);
    }

    /// Notes every fragment in a piece of markup.
    ///
    /// A fragment arriving without a token is a publish rather than a serving,
    /// and the client keeps the grant it was served with, so there is nothing
    /// to learn from one.
    fn learn(&mut self, text: &str) {
        for tag in text.split("<exos-live ").skip(1) {
            let Some((tag, _)) = tag.split_once('>') else {
                continue;
            };

            if let (Some(topic), Some(token)) = (attribute(tag, "topic"), attribute(tag, "token")) {
                self.fragments.insert(topic, token);
            }
        }
    }

    /// Says what this tab is displaying, opening the stream if it has to.
    ///
    /// The same arithmetic the runtime does, and both halves matter. A tab with
    /// nothing live on it opens no stream, because there is nothing to be sent.
    /// A tab that *had* something live on it says so when it no longer does:
    /// the set is replaced wholesale, and an empty one is what stops a page
    /// being sent patches for the page before it.
    async fn sync(&mut self) {
        if self.fragments.is_empty() && self.stream.is_none() {
            return;
        }

        let topics: Vec<(&String, &String)> = self.fragments.iter().collect();
        let encoded = serde_json::to_string(&topics).expect("topics are strings");

        if self.stream.is_some() && encoded == self.subscribed {
            return;
        }

        self.subscribed = encoded.clone();

        if self.stream.is_none() {
            self.stream = Some(self.dial().await);
        }

        let connection = &self
            .stream
            .as_ref()
            .expect("a stream was just opened")
            .connection;

        let body = format!(r#"{{"connection":"{connection}","topics":{encoded}}}"#);

        let answer = self
            .roundtrip(
                Request::builder()
                    .method("POST")
                    .uri("/_exos/subscribe")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body))
                    .expect("a valid request"),
            )
            .await;

        assert_eq!(
            answer.status(),
            StatusCode::NO_CONTENT,
            "the subscription was refused: {}",
            answer.body()
        );
    }

    /// Opens the stream and reads the greeting off it.
    async fn dial(&mut self) -> Live {
        // Not through `roundtrip`, which reads a body to the end, and a stream
        // has no end.
        let request = self.dressed(
            Request::builder()
                .uri("/_exos/live")
                .body(Body::empty())
                .expect("a valid request"),
        );

        let response = self
            .router
            .clone()
            .oneshot(request)
            .await
            .expect("the router answers");

        assert_eq!(
            response.status(),
            StatusCode::OK,
            "the application refused to open a stream"
        );

        let mut live = Live {
            connection: String::new(),
            events: response.into_body().into_data_stream(),
            buffer: String::new(),
        };

        let greeting = live.event().await;

        assert_eq!(
            greeting.name, "connection",
            "a stream names its connection first"
        );

        live.connection = greeting.data;
        live
    }
}

// -----------------------------------------------------------------------------
//                                  THE ANSWER
// -----------------------------------------------------------------------------

/// What came back.
///
/// One type for both of the things an application answers with, because there
/// is one wire format: a document is [`body`](Self::body) and an effect is
/// [`steps`](Self::steps).
#[derive(Clone, Debug)]
pub struct Answer {
    status: StatusCode,
    headers: HeaderMap,
    body: String,
}

impl Answer {
    /// The status it carried.
    ///
    /// Worth asserting even where an effect is: a refusal answers with the
    /// status it deserves *and* with what the page should do about it.
    #[must_use]
    pub fn status(&self) -> StatusCode {
        self.status
    }

    /// A header, where it was set and is text.
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).and_then(|value| value.to_str().ok())
    }

    /// The body as it arrived: the document for a page, the framed events for
    /// an effect.
    #[must_use]
    pub fn body(&self) -> &str {
        &self.body
    }

    /// Whether this is an effect: the framed steps an action answers with.
    ///
    /// The content type decides, as it does for the runtime, which reads an
    /// action's answer as frames and a navigation's as a document.
    #[must_use]
    pub fn is_effect(&self) -> bool {
        self.is("text/event-stream")
    }

    /// Whether this is a document, which is what a page a tab navigates to
    /// answers with.
    #[must_use]
    pub fn is_document(&self) -> bool {
        self.is("text/html")
    }

    /// The steps an effect asked for, in the order the client would apply
    /// them.
    ///
    /// Empty for anything that is not an effect, and for an effect with
    /// nothing to do, which are the same body.
    #[must_use]
    pub fn steps(&self) -> Vec<Step> {
        events(&self.body)
            .iter()
            .filter_map(|event| Step::from_frame(&event.name, &event.data))
            .collect()
    }

    /// Whether the content type is this one, whatever it says after it.
    fn is(&self, kind: &str) -> bool {
        self.header(header::CONTENT_TYPE.as_str())
            .is_some_and(|value| value.starts_with(kind))
    }
}

// -----------------------------------------------------------------------------
//                                 THE TRANSPORT
// -----------------------------------------------------------------------------

/// The open stream, and what has arrived on it but not yet been read.
#[derive(Debug)]
struct Live {
    /// What the server named this connection, from the greeting.
    connection: String,
    events: BodyDataStream,
    buffer: String,
}

impl Live {
    /// The next whole event, waiting for as much of the stream as it takes.
    ///
    /// # Panics
    ///
    /// If [`PATIENCE`] runs out, or the stream ends, since a tab that is
    /// waiting for an event and will now never get one has failed the test it
    /// was waiting in.
    async fn event(&mut self) -> Event {
        loop {
            if let Some(event) = self.take() {
                return event;
            }

            let chunk = timeout(PATIENCE, self.events.next())
                .await
                .expect("the stream said nothing in time")
                .expect("the stream ended")
                .expect("the stream is readable");

            self.buffer
                .push_str(&String::from_utf8_lossy(chunk.as_ref()));
        }
    }

    /// One event off the front of the buffer, if a whole one is there.
    fn take(&mut self) -> Option<Event> {
        loop {
            let (frame, rest) = self.buffer.split_once("\n\n")?;
            let event = parse(frame);
            self.buffer = rest.to_owned();

            // Keep-alives and anything else with no name of its own. The
            // browser would not dispatch them either.
            if !event.name.is_empty() {
                return Some(event);
            }
        }
    }
}

/// One server-sent event, reduced to the two fields exos uses.
#[derive(Debug, Default)]
struct Event {
    name: String,
    data: String,
}

/// Every whole event in a body, which is how an effect's reply is read.
fn events(body: &str) -> Vec<Event> {
    body.split("\n\n")
        .filter(|frame| !frame.trim().is_empty())
        .map(parse)
        .filter(|event| !event.name.is_empty())
        .collect()
}

/// A framed event, back into its name and payload.
///
/// A payload that had newlines in it arrived as several `data:` lines and is
/// joined back, which is what the client does with it.
fn parse(frame: &str) -> Event {
    let mut event = Event::default();
    let mut data: Vec<&str> = Vec::new();

    for line in frame.lines() {
        if let Some(name) = line.strip_prefix("event: ") {
            event.name = name.to_owned();
        } else if let Some(line) = line.strip_prefix("data: ") {
            data.push(line);
        }
    }

    event.data = data.join("\n");
    event
}

/// The value of one of a fragment wrapper's `data-` attributes.
fn attribute(tag: &str, name: &str) -> Option<String> {
    let (_, rest) = tag.split_once(&format!("data-{name}=\""))?;
    let (value, _) = rest.split_once('"')?;

    Some(value.to_owned())
}
