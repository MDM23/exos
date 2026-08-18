//! What the room says to one person.
//!
//! A price is state: the server holds it, a fragment renders it, and a publish
//! replaces it. Being outbid is not state. It happened, it happened once, and
//! there is no fragment whose re-render produces it, which is exactly the gap
//! a directed effect fills.
//!
//! The visible consequence is worth trying: reload the page and the price is
//! still there, because it was published state. The message below does not
//! come back, because it never was.
//!
//! # Three ways to reach somebody
//!
//! One `Effect` and three deliveries, which is the distinction this example is
//! built to make legible.
//!
//! * **Returned from a handler.** [`note`] handed back reaches exactly the tab
//!   that asked, with no identity involved at all. "Your bid is in" is this.
//! * **Sent to an audience.** [`tell`] reaches one person on every tab they
//!   have open, wherever they are. "You were outbid" is this, and it is the
//!   only one of the three that needs to know who anybody is.
//! * **Published as a fragment.** Not here: that is a lot's price, in
//!   [`room`](crate::room), and it reaches whoever is watching rather than
//!   whoever anybody is.

use exos::{Effect, Markup, attr, on_click, send, show, text, view};
use serde::{Deserialize, Serialize};

use crate::{
    bidder::{Guest, Viewer},
    store::Bidder,
};

/// The one line the room can say to somebody.
///
/// A model rather than a plain signal, because a handler can only write a
/// signal that lives on the document and today `#[model]` is the way to get
/// one. That it is also a request body shape is not used here, which is the
/// open question the roadmap's stage 4 is about.
#[exos::model]
#[derive(Debug, Default, Deserialize, Serialize)]
pub(crate) struct Toast {
    /// What was said, or nothing, which is also what decides whether the slot
    /// is on screen at all.
    pub(crate) message: String,
    /// How it reads. Its own field rather than something parsed back out of
    /// the text, so the styling is a signal like everything else.
    pub(crate) note: String,
}

/// How a message reads.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Note {
    /// Something went your way.
    Good,
    /// Something went against you.
    Warn,
}

impl Note {
    /// What the stylesheet keys on.
    const fn as_str(self) -> &'static str {
        match self {
            Self::Good => "good",
            Self::Warn => "warn",
        }
    }
}

/// The slot a message lands in, which every page carries.
///
/// Declared here because this is the markup it belongs to. Being a model it
/// lands on the document, so an effect arriving on the stream reaches it from
/// wherever the reader happens to be.
pub(crate) fn banner() -> Markup {
    let toast = Toast::signals();

    view! {
        // Rendered hidden, because it starts with nothing to say and the
        // script is deferred. Leaving it to the binding would paint an empty
        // toast for as long as the runtime takes to boot, and a navigation
        // re-seeds the message, so the server's word and the binding always
        // agree on the first frame.
        <div
            class="toast"
            role="status"
            aria-live="polite"
            hidden
            {&toast}
            {attr("data-note", toast.note.get())}
            {show(toast.message.get().is_empty().not())}
        >
            <span {text(toast.message.get())}></span>

            <button
                class="dismiss"
                type="button"
                aria-label="Dismiss"
                {on_click(|_| toast.message.set(""))}
            >"x"</button>
        </div>
    }
}

/// The effect that says `message`, whichever way it is delivered.
///
/// Two writes and one event on the wire, because consecutive `set`s merge into
/// a single signals step and the client assigns once.
#[must_use]
pub(crate) fn note(note: Note, message: impl Into<String>) -> Effect {
    let toast = Toast::signals();

    Effect::set(&toast.message, message.into()).and_set(&toast.note, String::from(note.as_str()))
}

/// Says `message` to one bidder, on every tab they have open.
///
/// This is the whole of what a directed effect costs at a call site, and the
/// match is the only place the two kinds of audience have to be told apart.
/// Neither branch knows or cares where the reader is: an effect is addressed
/// to a person, so it lands on the catalogue page as readily as on the room.
pub(crate) fn tell(bidder: &Bidder, kind: Note, message: impl Into<String>) {
    let effect = note(kind, message);

    match bidder {
        Bidder::Viewer(id) => send(&Viewer(*id), &effect),
        Bidder::Guest(name) => send(&Guest(name.clone()), &effect),
    }
}

/// Whether anybody is there to hear it.
///
/// A hint and never a guarantee: the last tab can close between this answer
/// and whatever is done about it. The honest use is the one in
/// [`room::hammer`](crate::room::hammer), which records the result either way
/// and treats reaching somebody as the accelerator it is.
#[must_use]
pub(crate) fn reaches(bidder: &Bidder) -> bool {
    match bidder {
        Bidder::Viewer(id) => exos::connected(&Viewer(*id)),
        Bidder::Guest(name) => exos::connected(&Guest(name.clone())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_slot_is_hidden_until_there_is_something_to_say() {
        let toast = Toast::signals();
        let html = banner().into_string();

        assert!(html.contains(&format!(
            "data-show=\"!($.{}.length === 0)\"",
            toast.message.name()
        )));
        assert!(html.contains(&format!("data-text=\"$.{}\"", toast.message.name())));
    }

    /// Dismissing is a signal write and nothing else: no round trip, because
    /// the server has no opinion about whether you have read something.
    #[test]
    fn dismissing_empties_the_message_in_the_browser() {
        let html = banner().into_string();

        assert!(html.contains("data-on-click"));
        assert!(!html.contains("post("), "{html}");
    }

    /// Being a model it lands on the document, which is what lets an effect
    /// arriving on the stream reach it from any page.
    #[test]
    fn the_slot_declares_both_signals_on_the_document() {
        let toast = Toast::signals();
        let html = banner().into_string();

        assert!(html.contains("data-signals-root=\""));

        for signal in [toast.message.name(), toast.note.name()] {
            assert!(html.contains(&format!("&quot;{signal}&quot;")), "{html}");
        }
    }

    /// How a message reads travels as a signal, so the stylesheet keys on an
    /// attribute the server never had to render.
    #[test]
    fn how_it_reads_is_an_attribute_a_signal_writes() {
        let toast = Toast::signals();
        let html = banner().into_string();

        assert!(html.contains(&format!(
            "data-attr=\"{{&quot;data-note&quot;: $.{}}}\"",
            toast.note.name()
        )));
    }

    #[test]
    fn a_note_writes_both_signals_as_one_event() {
        let effect = note(Note::Good, "Your bid is in.");

        assert_eq!(effect.steps().len(), 1, "one merge, one event");
        assert!(effect.to_stream().contains("good"));
        assert!(note(Note::Warn, "Outbid.").to_stream().contains("warn"));
    }

    /// The whole page is server-generated, so a field name has no reason to
    /// appear in it. If one did, renaming the field would break the browser
    /// rather than the build.
    #[test]
    fn a_field_name_never_leaves_the_server() {
        let html = banner().into_string();

        assert!(!html.contains("message"));
        assert!(!html.contains("\"note\""));
    }
}
