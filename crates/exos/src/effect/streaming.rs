//! Answering with effects as they are computed, rather than all at once.
//!
//! An [`Effect`] returned from a handler is framed as server-sent events and
//! then handed over whole, which is right for the handlers that know everything
//! before they answer. Some do not: an import that walks ten thousand rows, a
//! report that takes a minute, a job whose progress is the point. Those have
//! something to say long before they are finished.
//!
//! The wire format already allows it, which is why this is small. A handler's
//! reply and the live stream are the same server-sent events through the same
//! parser, so a body that arrives in pieces needs nothing new on the client:
//! [`consume`](../../js/runtime.js) reads frames off the response as they land
//! and applies each one.
//!
//! ```ignore
//! #[exos::post("/reports/build")]
//! async fn build() -> EffectStream<ReceiverStream<Effect>> {
//!     let (sender, receiver) = tokio::sync::mpsc::channel(8);
//!
//!     tokio::spawn(async move {
//!         for step in plan {
//!             let done = run(step).await;
//!             drop(sender.send(Effect::patch(progress(done))).await);
//!         }
//!     });
//!
//!     EffectStream::new(ReceiverStream::new(receiver))
//! }
//! ```
//!
//! # What it is not for
//!
//! Reaching a tab that did not ask. This is one request's answer, so it stops
//! when the request does and it goes nowhere else; [`publish`](crate::publish)
//! and [`send`](crate::send) are the two that address somebody. The reason to
//! choose this over publishing progress is that progress belongs to the caller
//! and to nobody else, so it needs no topic and no audience.

use core::{
    convert::Infallible,
    pin::Pin,
    task::{Context, Poll},
};

use axum::response::{IntoResponse, Response, sse};
use futures_core::Stream;

use crate::{Effect, Step};

/// Effects sent as they are computed.
///
/// Answer with one where an [`Effect`] would do but the handler cannot finish
/// before it has something worth saying. Every effect that arrives is framed
/// exactly as a whole one would be, so what reaches the browser is
/// indistinguishable from a handler that answered several times.
///
/// The stream has to be [`Unpin`], which every ordinary source already is:
/// a channel receiver, [`tokio_stream::iter`], or a boxed stream.
///
/// # No keep-alive
///
/// A live stream sends comments to hold an idle connection open. This is a
/// reply, so it ends when the handler does, and a comment arriving in the
/// middle of one would be a frame the client has no step for.
#[derive(Debug)]
#[must_use = "an effect stream does nothing until it is returned from a handler"]
pub struct EffectStream<S>(S);

impl<S> EffectStream<S>
where
    S: Stream<Item = Effect> + Send + Unpin + 'static,
{
    /// Answers with `effects` as they arrive.
    pub fn new(effects: S) -> Self {
        Self(effects)
    }
}

impl<S> IntoResponse for EffectStream<S>
where
    S: Stream<Item = Effect> + Send + Unpin + 'static,
{
    fn into_response(self) -> Response {
        // Through `Sse`, rather than framing the bytes here, because that is
        // what keeps this identical to the live stream by construction rather
        // than by two pieces of code agreeing. The two drifted once already,
        // over a step whose payload was empty.
        sse::Sse::new(Frames {
            effects: self.0,
            pending: Vec::new().into_iter(),
        })
        .into_response()
    }
}

/// The steps of each effect, flattened into one stream of events.
///
/// An effect is several steps and a stream yields one item at a time, so the
/// steps of the effect in hand are drained before the next one is asked for.
/// That is what keeps the order a caller wrote: an effect's own steps stay
/// together and in sequence, and effects stay in the order they were sent.
struct Frames<S> {
    effects: S,
    pending: std::vec::IntoIter<Step>,
}

impl<S> Stream for Frames<S>
where
    S: Stream<Item = Effect> + Unpin,
{
    type Item = Result<sse::Event, Infallible>;

    fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            if let Some(step) = self.pending.next() {
                return Poll::Ready(Some(Ok(sse::Event::from(step))));
            }

            match Pin::new(&mut self.effects).poll_next(context) {
                // An effect with no steps says nothing and is not a reason to
                // end the response, so the next one is asked for instead.
                Poll::Ready(Some(effect)) => self.pending = effect.into_steps().into_iter(),
                Poll::Ready(None) => return Poll::Ready(None),
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "a failing assertion is the point of a test"
)]
mod tests {
    use super::*;
    use crate::Markup;

    /// The bytes this puts on the wire.
    async fn streamed(effects: Vec<Effect>) -> String {
        let response = EffectStream::new(tokio_stream::iter(effects)).into_response();

        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("the body is readable");

        String::from_utf8(bytes.to_vec()).expect("an event is text")
    }

    fn patch(id: &str) -> Effect {
        Effect::patch(Markup(format!("<p id=\"{id}\"></p>")))
    }

    /// The whole promise: a browser cannot tell this from a handler that knew
    /// everything up front, so nothing on the client has to learn about it.
    #[tokio::test]
    async fn a_streamed_effect_is_framed_exactly_as_a_whole_one_is() {
        let effect = patch("x").focus("#x").scroll("#x");

        assert_eq!(streamed(vec![effect.clone()]).await, effect.to_stream());
    }

    /// Order is what a caller wrote: an effect's steps stay together and in
    /// sequence, and the effects stay in the order they were sent.
    #[tokio::test]
    async fn the_steps_arrive_in_the_order_they_were_produced() {
        let stream = streamed(vec![patch("first").focus("#first"), patch("second")]).await;

        let positions: Vec<usize> = ["id=\"first\"", "#first", "id=\"second\""]
            .iter()
            .map(|needle| stream.find(needle).expect("every step is on the wire"))
            .collect();

        assert!(
            positions.windows(2).all(|pair| pair[0] < pair[1]),
            "{stream}"
        );
    }

    /// A handler that finds nothing to say for a while is still running, so an
    /// empty effect is skipped rather than ending the response.
    #[tokio::test]
    async fn an_effect_with_nothing_in_it_does_not_end_the_stream() {
        let stream = streamed(vec![Effect::none(), patch("after"), Effect::none()]).await;

        assert_eq!(stream, patch("after").to_stream());
    }

    #[tokio::test]
    async fn a_stream_that_says_nothing_at_all_is_an_empty_body() {
        assert_eq!(streamed(Vec::new()).await, "");
    }

    /// The header the client dispatches on. Without it the reply is read as
    /// markup and patched, which for a burst of events is nonsense.
    #[tokio::test]
    async fn it_answers_as_an_event_stream() {
        let response = EffectStream::new(tokio_stream::iter(Vec::new())).into_response();

        assert_eq!(
            response
                .headers()
                .get(axum::http::header::CONTENT_TYPE)
                .expect("a content type"),
            "text/event-stream"
        );
    }
}
