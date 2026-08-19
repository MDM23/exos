//! What a handler hands back: an ordered list of things for the client to do.
//!
//! One type, so adding a capability later changes no signature.
//!
//! ```
//! # use exos::{Effect, Markup};
//! # use serde::{Deserialize, Serialize};
//! #[exos::model]
//! #[derive(Debug, Default, Deserialize, Serialize)]
//! struct Selection {
//!     picked: Vec<u32>,
//! }
//!
//! # fn file_list() -> Markup { Markup::default() }
//! let effect = Effect::patch(file_list())
//!     .and_set(&Selection::signals().picked, Vec::new())
//!     .focus("#file-list");
//!
//! assert_eq!(effect.steps().len(), 3);
//! ```
//!
//! # Wire format
//!
//! The same server-sent-event framing the live stream uses, which is the
//! Streams in the name. That is not tidiness: it means one parser on the
//! client rather than two, the action path and the live path become the same
//! code, and a slow handler can stream steps as it computes them instead of
//! buffering the lot.

use axum::{
    body::Body,
    http::{HeaderValue, header},
    response::{IntoResponse, Response, sse},
};
use serde::Serialize;
use serde_json::{Map, Value};

use crate::{Markup, Placement, Signal};

mod streaming;

pub use crate::effect::streaming::EffectStream;

/// One instruction for the client.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Step {
    /// Move the keyboard focus to the first match.
    Focus(String),
    /// Client-side navigation to a URL.
    Navigate(String),
    /// Replace the active page without a second fetch.
    ///
    /// A `GET` still answers with a real [`Page`](crate::Page) document,
    /// because a cold browser, a bookmark or a crawler gets no JavaScript.
    /// This is only the round-trip-saving path for an action that lands the
    /// user somewhere new.
    Page(Markup),
    /// Morph this HTML into place, keyed by the ids it carries.
    Patch(Markup),
    /// Reload the document. The last resort.
    Reload,
    /// Delete the elements matching a selector.
    Remove(String),
    /// Scroll the first match into view.
    Scroll(String),
    /// Merge into the client's signal store.
    Signals(Value),
}

impl Step {
    /// The event name and payload this step is sent as.
    ///
    /// A step with nothing to say still has to say something. A server-sent
    /// event carrying no `data` field at all is not dispatched by a browser,
    /// so it would arrive down the wire and hit no listener, and the only step
    /// that is all name and no payload would silently do nothing on the one
    /// path that frames it strictly. It repeats its own name, which is the
    /// payload that cannot be mistaken for a value.
    fn frame(&self) -> (&'static str, String) {
        match self {
            Self::Focus(selector) => ("focus", selector.clone()),
            Self::Navigate(url) => ("navigate", url.clone()),
            Self::Page(markup) => ("page", markup.as_str().to_owned()),
            Self::Patch(markup) => ("patch", markup.as_str().to_owned()),
            Self::Reload => ("reload", String::from("reload")),
            Self::Remove(selector) => ("remove", selector.clone()),
            Self::Scroll(selector) => ("scroll", selector.clone()),
            Self::Signals(value) => ("signals", value.to_string()),
        }
    }
}

/// An ordered list of [`Step`]s.
///
/// Order is meaningful: the client applies them as given, so clearing a
/// selection before patching the list it referred to does what it reads like.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Effect {
    steps: Vec<Step>,
}

impl Effect {
    /// Nothing to do.
    ///
    /// A successful action whose result the client already painted
    /// optimistically usually wants this.
    #[must_use]
    pub fn none() -> Self {
        Self::default()
    }

    /// Starts with a patch.
    #[must_use]
    pub fn patch(markup: impl Into<Markup>) -> Self {
        Self::none().and_patch(markup)
    }

    /// Starts by writing a signal.
    #[must_use]
    pub fn set<T: Serialize>(signal: &Signal<T>, value: T) -> Self {
        Self::none().and_set(signal, value)
    }

    /// Starts with a removal.
    #[must_use]
    pub fn remove(selector: impl Into<String>) -> Self {
        Self::none().and_remove(selector)
    }

    /// Starts with a navigation.
    #[must_use]
    pub fn navigate(url: impl Into<String>) -> Self {
        Self::none().and_navigate(url)
    }

    /// Starts by replacing the active page.
    #[must_use]
    pub fn page(markup: impl Into<Markup>) -> Self {
        Self::none().and_page(markup)
    }

    /// Reloads the document.
    #[must_use]
    pub fn reload() -> Self {
        Self::none().push(Step::Reload)
    }

    /// Adds a patch.
    #[must_use]
    pub fn and_patch(self, markup: impl Into<Markup>) -> Self {
        self.push(Step::Patch(markup.into()))
    }

    /// Writes a signal.
    ///
    /// The handle carries both the name and the type, so there is no string to
    /// keep in step with the template. In practice that means a `#[model]`
    /// field: those are named per field, so the handle a handler builds names
    /// the same signal the template declared, and they are declared on the
    /// document, which is where the client applies this.
    ///
    /// A [`signal`](crate::signal) handle belongs to the element that declared
    /// it and is not reachable from here. Nothing about the types says so, so
    /// a debug build asserts rather than writing a signal nothing reads; see
    /// [`Placement`].
    ///
    /// # Panics
    ///
    /// In debug builds, if `signal` is not declared on the document.
    #[must_use]
    pub fn and_set<T: Serialize>(mut self, signal: &Signal<T>, value: T) -> Self {
        debug_assert_eq!(
            signal.placement(),
            Placement::Document,
            "this signal belongs to the element that declared it, so a write from here \
             would reach a different signal of the same name; only a #[model] field is \
             reachable from a handler"
        );

        let value = serde_json::to_value(&value).unwrap_or(Value::Null);

        // Consecutive writes are one merge, which keeps `Object.assign` on the
        // client to a single pass and the wire to a single event. Anything
        // between them keeps its place, because order is what a caller sees.
        match self.steps.last_mut() {
            Some(Step::Signals(Value::Object(held))) => {
                held.insert(signal.name().to_owned(), value);
                self
            }
            _ => {
                let mut merge = Map::new();
                merge.insert(signal.name().to_owned(), value);
                self.push(Step::Signals(Value::Object(merge)))
            }
        }
    }

    /// Adds a removal.
    #[must_use]
    pub fn and_remove(self, selector: impl Into<String>) -> Self {
        self.push(Step::Remove(selector.into()))
    }

    /// Adds a navigation.
    #[must_use]
    pub fn and_navigate(self, url: impl Into<String>) -> Self {
        self.push(Step::Navigate(url.into()))
    }

    /// Adds a page replacement.
    #[must_use]
    pub fn and_page(self, markup: impl Into<Markup>) -> Self {
        self.push(Step::Page(markup.into()))
    }

    /// Moves the keyboard focus.
    #[must_use]
    pub fn focus(self, selector: impl Into<String>) -> Self {
        self.push(Step::Focus(selector.into()))
    }

    /// Scrolls an element into view.
    #[must_use]
    pub fn scroll(self, selector: impl Into<String>) -> Self {
        self.push(Step::Scroll(selector.into()))
    }

    /// The steps, in the order the client will apply them.
    #[must_use]
    pub fn steps(&self) -> &[Step] {
        &self.steps
    }

    /// The steps, owned.
    ///
    /// What [`EffectStream`](crate::EffectStream) frames, since a step on its
    /// way to the wire has no reason to be copied first.
    #[must_use]
    pub fn into_steps(self) -> Vec<Step> {
        self.steps
    }

    /// The server-sent-event framed body.
    #[must_use]
    pub fn to_stream(&self) -> String {
        let mut out = String::new();

        for step in &self.steps {
            let (event, data) = step.frame();

            out.push_str("event: ");
            out.push_str(event);
            out.push('\n');

            // A data line cannot contain a newline, so a multi-line payload
            // becomes several lines and the client joins them back.
            for line in data.split('\n') {
                out.push_str("data: ");
                out.push_str(line);
                out.push('\n');
            }

            out.push('\n');
        }

        out
    }

    fn push(mut self, step: Step) -> Self {
        self.steps.push(step);
        self
    }
}

/// One step is one server-sent event, which is what lets a live stream and an
/// action response share a wire format and a parser.
impl From<Step> for sse::Event {
    fn from(step: Step) -> Self {
        let (event, data) = step.frame();
        Self::default().event(event).data(data)
    }
}

impl From<Markup> for Effect {
    fn from(markup: Markup) -> Self {
        Self::patch(markup)
    }
}

impl IntoResponse for Effect {
    fn into_response(self) -> Response {
        let mut response = Response::new(Body::from(self.to_stream()));

        response.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/event-stream"),
        );

        response
    }
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "a failing assertion is the point of a test"
)]
mod tests {
    use super::*;
    use crate::signal;

    /// A handle placed the way `#[model]` places one, which is the only kind
    /// a handler can write.
    fn field<T>(name: &str, initial: serde_json::Value) -> Signal<T> {
        Signal::with_value(name, initial, Placement::Document)
    }

    #[test]
    fn steps_keep_the_order_they_were_added_in() {
        let stream = Effect::set(&field("count", Value::from(0)), 1)
            .and_patch(Markup(String::from("<li id=\"x\"></li>")))
            .focus("#x")
            .to_stream();

        let signals = stream.find("event: signals").expect("a signals step");
        let patch = stream.find("event: patch").expect("a patch step");
        let focus = stream.find("event: focus").expect("a focus step");

        assert!(signals < patch && patch < focus);
    }

    #[test]
    fn a_write_is_keyed_by_the_handles_name() {
        let picked: Signal<Vec<u32>> = field("picked", Value::from(Vec::<u32>::new()));
        let effect = Effect::set(&picked, vec![1, 2]);

        assert_eq!(
            effect.steps(),
            [Step::Signals(serde_json::json!({ picked.name(): [1, 2] }))]
        );
    }

    /// A signal belonging to the element that declared it is not reachable
    /// from a handler at all, and saying so beats writing one nothing reads.
    #[test]
    #[should_panic(expected = "belongs to the element that declared it")]
    #[cfg(debug_assertions)]
    fn writing_an_element_signal_says_it_cannot_work() {
        drop(Effect::set(&signal(false), true));
    }

    /// One event rather than three, and the client assigns once.
    #[test]
    fn consecutive_writes_merge_into_one_step() {
        let picked: Signal<Vec<u32>> = field("picked", Value::from(Vec::<u32>::new()));
        let fail = field("fail", Value::from(false));

        let effect = Effect::set(&picked, Vec::new()).and_set(&fail, true);

        assert_eq!(effect.steps().len(), 1);
        assert_eq!(effect.to_stream().matches("event: signals").count(), 1);
    }

    /// Merging must not reorder anything: a write after a patch stays after it.
    #[test]
    fn a_step_between_two_writes_keeps_them_apart() {
        let picked: Signal<Vec<u32>> = field("picked", Value::from(Vec::<u32>::new()));
        let fail = field("fail", Value::from(false));

        let effect = Effect::set(&picked, Vec::new())
            .and_patch(Markup(String::from("<li id=\"x\"></li>")))
            .and_set(&fail, true);

        assert_eq!(effect.steps().len(), 3);
        assert!(matches!(effect.steps()[2], Step::Signals(_)));
    }

    #[test]
    fn a_newline_in_the_payload_stays_one_event() {
        let stream = Effect::patch(Markup(String::from("<ul>\n<li>a</li>\n</ul>"))).to_stream();

        assert_eq!(stream.matches("event: patch").count(), 1);
        assert_eq!(stream.matches("data: ").count(), 3);
        assert!(stream.ends_with("\n\n"), "events are blank-line terminated");
    }

    #[test]
    fn nothing_to_do_is_an_empty_body() {
        assert_eq!(Effect::none().to_stream(), "");
        assert!(Effect::none().steps().is_empty());
    }

    /// A browser does not dispatch a server-sent event that carries no `data`
    /// field, so a step with nothing of its own to say says its own name.
    ///
    /// `reload` is the only one, and it was therefore the only step this could
    /// go wrong for. Sent down the live stream it arrived and hit no listener
    /// at all, while the same step in a handler's reply worked, because the
    /// two paths spelled an empty payload differently.
    #[test]
    fn a_step_with_no_payload_of_its_own_still_carries_one() {
        let (event, data) = Step::Reload.frame();

        assert_eq!(event, "reload");
        assert!(!data.is_empty(), "an empty one would not be dispatched");

        assert_eq!(
            Effect::reload().to_stream(),
            "event: reload\ndata: reload\n\n"
        );
    }

    /// The bytes an [`Sse`](sse::Sse) response puts on the wire for one step,
    /// which is the only way to read an [`Event`](sse::Event) back.
    async fn streamed(step: Step) -> String {
        let events =
            tokio_stream::iter([Ok::<_, core::convert::Infallible>(sse::Event::from(step))]);

        let bytes = axum::body::to_bytes(
            sse::Sse::new(events).into_response().into_body(),
            usize::MAX,
        )
        .await
        .expect("the body is readable");

        String::from_utf8(bytes.to_vec()).expect("an event is text")
    }

    /// One wire format, which is the whole reason the client has one parser
    /// and the action path and the live path are the same code. The two
    /// drifted once, over exactly the step above.
    #[tokio::test]
    async fn a_reply_and_the_stream_frame_a_step_identically() {
        for step in [
            Step::Reload,
            Step::Scroll(String::from("#x")),
            Step::Patch(Markup(String::from("<p id=\"x\">hi</p>"))),
            Step::Signals(serde_json::json!({ "a": 1 })),
        ] {
            assert_eq!(
                Effect::none().push(step.clone()).to_stream(),
                streamed(step.clone()).await,
                "{step:?} is framed two ways"
            );
        }
    }
}
