//! The room: what is playing, what is next, and who is listening.
//!
//! One live fragment for all of it. A track changing, a heart, a removal and a
//! drag all end in the same publish, and every tab watching gets the same
//! patch, so two browsers stay in step without either of them asking.

use axum::Json;
use exos::{Effect, Markup, connection_count, data, preserve, publish, view};
use serde::Deserialize;

use crate::{
    selection::Selection,
    store::{self, Queue, Room},
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
        { sleeve(&queue) }

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

/// The sleeve of whatever is on.
///
/// Two boxes rather than one, because a cross-fade needs something to fade
/// against. The blur is the wrapper's background and the sleeve is painted over
/// it, so the blur is what shows for as long as the sleeve is transparent, and
/// the fade is between two things that are both already there.
///
/// The `id` carries the track, which is what makes the mark moving a
/// *replacement* rather than the same `<img>` with a new `src`. The morph
/// matches children by id, so a different track is a different element, and it
/// arrives with no bitmap of its own to go on painting while the next one
/// loads.
///
/// The blur is markup rather than a signal, and rather than anything the
/// browser computes: the server owns it, it is derived from a file that cannot
/// change while the program runs, and a `style` the server rendered is
/// something the morph maintains rather than something it strips.
///
/// **Whether the sleeve has loaded is the other way round, and needs
/// [`preserve`].** It is client state on an element the server re-renders on
/// every heart, drag and removal, and an ordinary patch would write the
/// server's empty `data-fade` back over it. Nothing would put it back either: a
/// drag reorders the DOM before it posts, so the markup that lands moves no
/// rows, and a patch that changes only attributes fires no mutation for a
/// plugin to answer. So the sleeve stayed blurred until the track changed.
/// Opting the element out says what is true, which is that the server has
/// nothing left to tell this `<img>` for as long as it is the same one.
fn sleeve(queue: &Queue) -> Markup {
    let Some(track) = queue.playing() else {
        return Markup::default();
    };

    view! {
        <section class="playing">
            <div class="sleeve" style={ format!("--blur: url('{}')", track.cover.blur) }>
                <img
                    id={ format!("sleeve-{}", track.id) }
                    alt=""
                    data-fade
                    decoding="async"
                    height="240"
                    src={ track.cover.art }
                    width="240"
                    {preserve()}
                >
            </div>

            <div class="name">
                <p class="eyebrow">"Now playing"</p>
                <p class="title">{ &track.title }</p>
                <p class="artist">{ &track.artist }</p>
            </div>
        </section>
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
    publish(room());
}

/// What the room has to say about a batch, if anything.
///
/// A refusal is worth a sentence, because the visible half of it is a row
/// coming back and a listener deserves to know why. Nothing refused clears
/// whatever the last one said, so the note never outlives what it was about.
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

    /// The room as a browser is served it. In a request, because the token in
    /// the wrapper is bound to whoever the request is for.
    fn markup() -> String {
        seeded();
        exos::with_scope(|| room().to_markup().into_string())
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
    /// depend on who is reading it. The wrapper is the exception and the only
    /// one: its token is this browser's grant to watch the topic.
    #[test]
    fn the_room_says_the_same_thing_to_everybody() {
        seeded();

        assert_eq!(room().markup(), room().markup());
        assert_ne!(markup(), markup(), "and the grant is one browser's");
    }

    /// The blur travels in the page, and the sleeve over it opts out of being
    /// patched.
    ///
    /// Without the second half, a heart or a drag writes the server's empty
    /// `data-fade` back over a sleeve that had already faded in, and nothing
    /// puts it back: a patch that changes only attributes moves no nodes, and
    /// the client hears about nodes. The sleeve blurred again and stayed that
    /// way until the track changed.
    #[test]
    fn the_sleeve_carries_its_blur_and_keeps_what_it_has_loaded() {
        let html = markup();

        assert!(
            html.contains("--blur: url('data:image/png;base64,"),
            "{html:.400}"
        );
        assert!(
            html.contains("data-fade") && html.contains("data-preserve"),
            "{html:.400}"
        );
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
