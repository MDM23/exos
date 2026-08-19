//! The room: what is playing, what is next, and who is listening.
//!
//! One live fragment for all of it. A track changing, a heart, a removal and a
//! drag all end in the same publish, and every tab watching gets the same
//! patch, so two browsers stay in step without either of them asking.

use axum::Json;
use exos::{Effect, Markup, connection_count, data, publish, view};
use serde::Deserialize;

use crate::{
    selection::Selection,
    store::{self, Room},
    track,
};

/// What the drag sends when it ends.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub(crate) struct Dragged {
    /// The queued track ids, in the order they were dragged into.
    pub(crate) order: Vec<String>,
}

/// The whole room, as one fragment.
///
/// Two things in here are the server's rather than any viewer's, which is what
/// lets them share a topic: what is playing, and how many streams are open.
/// Neither depends on who is reading, so the same topic means the same HTML
/// for everybody, which is what a topic has to promise.
#[exos::live]
pub(crate) fn room() -> Markup {
    let queue = data::<Room>().snapshot();
    let selection = Selection::signals();

    if queue.tracks.is_empty() {
        // Native control flow, at render time, on the server. An empty room is
        // markup that was never rendered rather than a branch in the browser.
        // It cannot happen while the room refuses to remove what it is
        // playing, which is why this says so plainly rather than apologising.
        return view! { <p class="empty">"Nothing in the room."</p> };
    }

    view! {
        { listeners() }

        <ul
            id="queue"
            class="queue"
            role="list"
            data-sortable="post('/tracks/reorder', { order: $._order })"
        >
            {
                queue
                    .tracks
                    .iter()
                    .map(|track| track::row(track, queue.is_playing(track.id), &selection))
                    .collect::<Vec<_>>()
            }
        </ul>
    }
}

/// How many tabs have this room open.
///
/// A real count of open streams rather than anything invented: exos already
/// knows, because it is holding every one of them. It moves when a tab opens
/// or closes, and the clock in [`main`](crate::main) republishes the room when
/// it notices, so nobody has to poll for it.
fn listeners() -> Markup {
    let open = connection_count();

    view! {
        <p class="listeners">
            { open }
            { if open == 1 { " listener" } else { " listeners" } }
        </p>
    }
}

/// Pushes the room to every tab watching it.
///
/// Every action ends here, and that one line is what keeps two tabs in step.
pub(crate) fn publish_room() {
    publish(room);
}

/// What the room has to say about a batch, if anything.
///
/// A refusal is worth a sentence, because the visible half of it is a row
/// coming back and a listener deserves to know why. Nothing refused clears
/// whatever the last one said, so the note never outlives what it was about.
#[must_use]
pub(crate) fn say(refused: Option<String>) -> Effect {
    let note = refused.map_or_else(String::new, |title| {
        format!("\u{201c}{title}\u{201d} is playing, so it stayed.")
    });

    Effect::set(&Selection::signals().note, note)
}

/// Puts the queue in the order a drag ended in.
#[exos::post("/tracks/reorder")]
async fn reorder(Json(dragged): Json<Dragged>) -> Effect {
    data::<Room>().update(|tracks| store::reorder(tracks, &dragged.order));

    publish_room();
    Effect::none()
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "a failing assertion is the point of a test"
)]
mod tests {
    use super::*;
    use crate::tests::seeded;

    fn markup() -> String {
        seeded();
        room().to_markup().into_string()
    }

    /// Being able to subscribe is the authorization, so the wrapper carries a
    /// token the client could not have produced.
    #[test]
    fn the_room_is_a_live_fragment() {
        let html = markup();

        assert!(html.starts_with("<exos-live style=\"display:contents\" id=\"live-room-"));
        assert!(html.contains("data-token=\""));
    }

    /// A topic has to completely determine its content, so nothing in here may
    /// depend on who is reading it.
    #[test]
    fn the_room_says_the_same_thing_to_everybody() {
        assert_eq!(markup(), markup());
    }

    /// Exactly one row is marked, and it is a row like any other. The mark is
    /// what moves when a track ends; the list does not.
    #[test]
    fn one_row_is_marked_and_it_is_still_a_row() {
        seeded();

        let playing = data::<Room>().update(|queue| queue.playing);
        let html = markup();

        assert_eq!(html.matches("data-playing=\"true\"").count(), 1);
        assert_eq!(html.matches("aria-current=\"true\"").count(), 1);

        // Marked, and still carrying everything an unmarked row carries: it can
        // be dragged, and it can be ticked, which is how a listener finds out
        // that the room will not remove it.
        let row = html
            .find(&format!("id=\"track-{playing}\""))
            .expect("the marked row is in the list");
        let end = row + html[row..].find("</li>").expect("a row ends");

        assert!(html[row..end].contains("data-sort-item="));
        assert!(html[row..end].contains("type=\"checkbox\""));
        assert!(html[row..end].contains("data-playing=\"true\""));
    }

    /// Every row, without exception, which is what makes the mark moving a
    /// change of one attribute rather than a rearrangement.
    #[test]
    fn every_track_is_one_draggable_selectable_row() {
        seeded();

        let tracks = data::<Room>().update(|queue| queue.tracks.len());
        let html = markup();

        assert_eq!(html.matches("<li").count(), tracks);
        assert_eq!(html.matches("data-sort-item=").count(), tracks);
        assert_eq!(html.matches("type=\"checkbox\"").count(), tracks);
    }

    /// The drag posts what the plugin collected, which is a name the plugin
    /// owns rather than one this example invented.
    #[test]
    fn the_queue_hands_its_new_order_to_the_server() {
        assert!(
            markup().contains("data-sortable=\"post('/tracks/reorder', { order: $._order })\"")
        );
    }

    #[test]
    fn a_refusal_is_said_in_words_and_anything_else_clears_it() {
        let note = Selection::signals().note;

        let refused = say(Some(String::from("Tail Call"))).to_stream();
        assert!(refused.contains("event: signals"), "{refused}");
        assert!(refused.contains("Tail Call"), "{refused}");

        // Cleared rather than left alone, so what the room last refused cannot
        // outlive the batch it was about.
        let cleared = say(None).to_stream();
        assert!(
            cleared.contains(&format!("\"{}\":\"\"", note.name())),
            "{cleared}"
        );
    }
}
