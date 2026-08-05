//! What a handler hands back: an ordered list of things for the client to do.
//!
//! One type, so adding a capability later changes no signature.
//!
//! ```
//! # use exos::{Effect, Markup};
//! # fn file_list() -> Markup { Markup::default() }
//! let effect = Effect::patch(file_list())
//!     .and_signals(serde_json::json!({ "picked": [] }))
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
    response::{IntoResponse, Response},
};
use serde_json::Value;

use crate::Markup;

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
    fn frame(&self) -> (&'static str, String) {
        match self {
            Self::Focus(selector) => ("focus", selector.clone()),
            Self::Navigate(url) => ("navigate", url.clone()),
            Self::Page(markup) => ("page", markup.as_str().to_owned()),
            Self::Patch(markup) => ("patch", markup.as_str().to_owned()),
            Self::Reload => ("reload", String::new()),
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

    /// Starts with a signal merge.
    #[must_use]
    pub fn signals(value: Value) -> Self {
        Self::none().and_signals(value)
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

    /// Adds a signal merge.
    #[must_use]
    pub fn and_signals(self, value: Value) -> Self {
        self.push(Step::Signals(value))
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

    #[test]
    fn steps_keep_the_order_they_were_added_in() {
        let stream = Effect::signals(serde_json::json!({ "a": 1 }))
            .and_patch(Markup(String::from("<li id=\"x\"></li>")))
            .focus("#x")
            .to_stream();

        let signals = stream.find("event: signals").expect("a signals step");
        let patch = stream.find("event: patch").expect("a patch step");
        let focus = stream.find("event: focus").expect("a focus step");

        assert!(signals < patch && patch < focus);
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
}
