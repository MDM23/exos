//! Choosing several tracks, and the one action that acts on all of them.

use exos::{Effect, Markup, Model, data, on_click, publish, show, text, view, when};
use serde::{Deserialize, Serialize};

use crate::{
    room::{self, room},
    store::{self, Room},
};

/// What the bar holds, and what removing sends.
///
/// One declaration for both. The template binds `picked`, the handler takes
/// `Model<Selection>` and the effect writes both handles back, so renaming a
/// field breaks all of them at once and none of them silently.
#[exos::model]
#[derive(Debug, Default, Deserialize, Serialize)]
pub(crate) struct Selection {
    /// The tracks the listener ticked.
    pub(crate) picked: Vec<u32>,
    /// The tracks a click has already taken off the page, before the server
    /// has agreed to it.
    ///
    /// **This is a model field rather than a [`signal`](exos::signal) on the
    /// row, and the reason is the whole of why an optimistic update needs
    /// somewhere the server can reach.** A row's own signal is declared when
    /// the element is inserted and never again, so a patch that re-renders the
    /// row leaves it holding whatever it held: the row would stay hidden after
    /// a refusal and only come back on a reload. A handler cannot clear it
    /// either, because [`Effect::set`](exos::Effect::set) can only write a
    /// signal that lives on the document.
    ///
    /// Declared on the document, both halves work. The click hides the row, and
    /// the reply puts it back by emptying this.
    pub(crate) going: Vec<u32>,
    /// What the room said about the last removal, or nothing.
    ///
    /// The server's word rather than a switch the page sets on itself: it is
    /// written only by a handler, and it is empty unless the room refused
    /// something.
    pub(crate) note: String,
}

/// The bar that appears once something is ticked, and whatever the room said.
pub(crate) fn bar(selection: &SelectionSignals) -> Markup {
    view! {
        <div class="bar" {show(selection.picked.get().any())}>
            <span {text(selection.picked.get().len())}></span>
            " selected"

            <button
                class="drop"
                type="button"
                {on_click(|_| {
                    // A no-op when nothing is ticked, so the button cannot
                    // send an empty batch even if it is somehow clicked.
                    when(selection.picked.get().any(), |()| remove::post(selection));
                })}
            >"Remove"</button>

            <button type="button" {on_click(|_| selection.picked.clear())}>
                "Clear"
            </button>
        </div>

        <p class="answer" role="status" {show(selection.note.get().is_empty().not())}>
            <span {text(selection.note.get())}></span>
        </p>
    }
}

/// Takes every ticked track out, except the one that is playing.
#[exos::post("/tracks/remove")]
async fn remove(Model(selection): Model<Selection>) -> Effect {
    let refused = data::<Room>().update(|tracks| store::remove(tracks, &selection.picked));

    publish(&room());

    // The selection no longer refers to anything the listener can see, whether
    // or not the room kept one of them. The handle names the signal, so this
    // cannot drift from what the template declared or from what the extractor
    // reads back.
    room::say(refused).and_set(&Selection::signals().picked, Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bar_derives_from_the_signal() {
        let selection = Selection::signals();
        let html = bar(&selection).into_string();

        // Escaped, because it is an attribute value and `>` is `&gt;` there.
        assert!(html.contains(&format!(
            "data-show=\"$.{}.length &gt; 0\"",
            selection.picked.name()
        )));
        assert!(html.contains(&format!(
            "data-text=\"$.{}.length\"",
            selection.picked.name()
        )));
    }

    /// The note is the server's to write and the page's to show, so nothing in
    /// the markup sets it.
    #[test]
    fn the_note_is_only_ever_read_here() {
        let note = Selection::signals().note;
        let html = bar(&Selection::signals()).into_string();

        assert!(html.contains(&format!("data-text=\"$.{}\"", note.name())));
        assert!(!html.contains(&format!("$.{} =", note.name())));
    }

    /// The whole page is server-generated and so is everything that reads it
    /// back, so a field name has no reason to appear in either. If one did, it
    /// could be written into a template, and renaming the field would then
    /// break the browser rather than the build.
    #[test]
    fn a_field_name_never_leaves_the_server() {
        let html = bar(&Selection::signals()).into_string();

        assert!(!html.contains("picked"));
        assert!(!html.contains("note"));
    }
}
