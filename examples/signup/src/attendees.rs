//! Rows added by a button, and what each one costs.
//!
//! This is the part of the form that is not part of the form. A repeating
//! group cannot ride along in one submission today, for two reasons that
//! compound: [`bind`] names one signal, and nothing can name row three's field;
//! and the per-element [`signal`] a row can hold is reachable neither by
//! `Effect::set` nor by the generated caller, which sends a model and only a
//! model.
//!
//! So the rows live on the server and each edit is a round trip of its own,
//! through a one-field model that exists to be a transport and for no other
//! reason. What keeps that from being a request per keystroke is [`debounce`],
//! and every row gets its own timer because every row is its own scope.

use axum::extract::Path;
use exos::{Effect, Markup, Model, bind, data, debounce, on_click, on_input, signal, view};
use serde::{Deserialize, Serialize};

use crate::store::{self, Attendee, Roster};

/// The buffer one row's name travels in.
///
/// One field, on the document, holding whichever row was last left. It is not
/// state anybody wants: it is the only shape an action's body can take, so a
/// row copies its own signal into this and posts immediately.
#[exos::model]
#[derive(Debug, Default, Deserialize, Serialize)]
pub(crate) struct Row {
    /// The name being saved.
    pub(crate) name: String,
}

/// The rows, and the button that adds one.
pub(crate) fn roster() -> Markup {
    let rows = data::<Roster>().snapshot();
    let row = Row::signals();

    view! {
        <div id="attendees" class="rows" {&row}>
            { rows.iter().map(line).collect::<Vec<_>>() }

            <button type="button" class="add" {on_click(|_| add::post())}>
                "Add attendee"
            </button>
        </div>
    }
}

/// One row.
fn line(attendee: &Attendee) -> Markup {
    let id = attendee.id;
    let row = Row::signals();

    // Per-row client state, so typing in the third row does not appear in the
    // first. The name is the same in every row, because it is generated from
    // this call site, and that is right: the row's element is its own scope,
    // and a declaration is never overwritten, so a patch that re-renders the
    // list leaves whatever is half-typed alone.
    let draft = signal(attendee.name.clone());

    view! {
        <div class="row" id={ format!("attendee-{id}") } {&draft}>
            <input
                type="text"
                aria-label="Attendee"
                placeholder="Name"
                {bind(&draft)}
                {on_input(|_| debounce(400, || {
                    // Two statements because there is no third: the row's own
                    // signal cannot be sent, so it is copied into the model the
                    // caller does know how to send.
                    row.name.set(draft.get());
                    rename::put(id, &row);
                }))}
            >

            <button
                type="button"
                class="drop"
                aria-label="Remove attendee"
                {on_click(|_| remove::delete(id))}
            >"x"</button>
        </div>
    }
}

/// Adds an empty row.
#[exos::post("/attendees")]
async fn add() -> Effect {
    data::<Roster>().update(store::add_attendee);
    Effect::patch(roster())
}

/// Writes one row's name, once the typing in it has stopped.
#[exos::put("/attendees/{id}")]
async fn rename(Path(id): Path<u32>, Model(row): Model<Row>) -> Effect {
    data::<Roster>().update(|rows| store::rename_attendee(rows, id, &row.name));

    // Nothing to say. The row already shows what was typed into it, and a patch
    // here would replace the field somebody may have tabbed into.
    Effect::none()
}

/// Drops one row.
#[exos::delete("/attendees/{id}")]
async fn remove(Path(id): Path<u32>) -> Effect {
    data::<Roster>().update(|rows| store::remove_attendee(rows, id));
    Effect::patch(roster())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::get;

    /// A row's field holds the row's own signal, and the buffer is written
    /// from it a statement before the call that sends it. Two statements
    /// because there is no way to send the first one directly.
    #[tokio::test]
    async fn a_row_sends_its_own_text_through_the_buffer() {
        let html = get("/").await;
        let name = Row::signals().name;

        assert!(html.contains("id=\"attendee-1\""));
        assert!(!html.contains(&format!("data-bind=\"{}\"", name.name())));
        assert!(html.contains(&format!("$.{} = $.", name.name())));
        assert!(html.contains("put(&quot;/attendees/1&quot;"));
    }

    /// Saving happens as it is typed rather than on the way out, which is only
    /// bearable because the call is held back. Every row carries the same key,
    /// since one call site renders them all, and the runtime resolves it
    /// against each row's own scope so that one row cannot cancel another.
    #[tokio::test]
    async fn a_row_saves_while_it_is_typed_without_a_call_per_keystroke() {
        let html = get("/").await;

        assert!(html.contains("data-on-input=\"debounce("), "{html:.4000}");
        assert!(!html.contains("data-on-focusout"), "{html:.4000}");

        let keys: Vec<&str> = html.matches("debounce(&quot;").collect();
        assert_eq!(keys.len(), 2, "one per row");

        let first = html.split("data-on-input=\"debounce(&quot;").nth(1);
        let second = html.split("data-on-input=\"debounce(&quot;").nth(2);

        assert_eq!(
            first.map(|rest| &rest[..9]),
            second.map(|rest| &rest[..9]),
            "one call site is one key"
        );
    }

    /// Every row declares the same generated name, because it comes from one
    /// call site, and every row is its own scope, so the second row's text
    /// does not appear in the first.
    #[tokio::test]
    async fn rows_share_a_name_and_not_a_value() {
        let html = get("/").await;
        let rows: Vec<&str> = html.matches("class=\"row\"").collect();

        assert_eq!(rows.len(), 2);
        assert_eq!(html.matches("id=\"attendee-").count(), 2);
    }

    /// Adding a row is a round trip that answers with the whole list, because
    /// there is no other way to get an input onto the page.
    #[tokio::test]
    async fn adding_a_row_is_a_round_trip() {
        let html = get("/").await;

        assert!(html.contains("data-on-click=\"post(&quot;/attendees&quot;)\""));
    }
}
