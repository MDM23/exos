//! One row, and everything a click on it can do.
//!
//! Both actions here paint before the server has answered, which is the point
//! of them. A heart fills under the pointer and a removed row goes at once,
//! and each is confirmed or undone by the patch that follows. The handler does
//! not have to know which of the two it is doing.

use axum::extract::Path;
use exos::{Effect, Markup, attr_now, bind, data, on_click, show, view};

use crate::{
    room::{self, publish_room},
    selection::{Selection, SelectionSignals},
    store::{self, Room, Track},
};

/// One track in the queue, marked if it is the one on.
///
/// Every row is the same shape whether or not it is playing, down to the width
/// of the mark, so the queue does not shift when the mark moves along it. The
/// row that is playing is a row like any other: it can be dragged, ticked and
/// hearted, and asking to remove it is exactly how a listener finds out that
/// the room will not.
pub(crate) fn row(track: &Track, playing: bool, selection: &SelectionSignals) -> Markup {
    let id = track.id;
    let hearted = track.hearted;

    // `data-hearted` and `data-playing` are not mirrored into signals. The
    // server owns them, a click writes one speculatively, and the patch that
    // follows overwrites it either way. One source of truth, so nothing can
    // drift.
    //
    // Whether this row is on its way out is the other kind of state: the server
    // gets a say, because it can refuse. So it lives on the model rather than
    // on this element, and [`Selection::going`] is where the reason is written
    // down.
    view! {
        <li
            id={ format!("track-{id}") }
            class="track"
            data-sort-item={ track.id }
            data-playing={ playing }
            data-hearted={ track.hearted }
            aria-current={ playing.then_some("true") }
            {show(selection.going.get().contains(id).not())}
        >
            // Always here and always the same width, so the mark travelling
            // down the queue moves nothing but itself.
            <span class="mark" aria-hidden="true"></span>

            <input type="checkbox" aria-label="Select" value={ track.id } {bind(&selection.picked)}>

            <span class="handle" data-drag-handle aria-hidden="true">"::"</span>

            <span class="name">
                <span class="title">{ &track.title }</span>
                <span class="artist">{ &track.artist }</span>
            </span>

            <button
                class="heart"
                type="button"
                aria-label="Heart"
                {on_click(move |_| {
                    attr_now("data-hearted", !hearted);
                    heart::post(id);
                })}
            >"\u{2665}"</button>

            <button
                class="drop"
                type="button"
                aria-label="Remove"
                {on_click(move |_| {
                    selection.going.push(id);
                    drop_track::post(id);
                })}
            >"x"</button>
        </li>
    }
}

/// Turns the heart on one track.
#[exos::post("/tracks/{id}/heart")]
async fn heart(Path(id): Path<u32>) -> Effect {
    data::<Room>().update(|queue| store::heart(queue, id));

    publish_room();
    Effect::none()
}

/// Takes one track out, unless it is the one playing.
#[exos::post("/tracks/{id}/remove")]
async fn drop_track(Path(id): Path<u32>) -> Effect {
    let refused = data::<Room>().update(|queue| store::remove(queue, &[id]));

    // Published either way. On success this confirms the row that already
    // went, and on a refusal it puts it back exactly where it was, and the
    // handler does not have to know which.
    publish_room();

    // And the row is shown again either way, because the patch above decides
    // whether there is a row left to show. Emptying the whole list rather than
    // taking one id out of it costs a rapid second click a flicker and saves
    // the server from having to know what the page is holding.
    room::say(refused).and_set(&Selection::signals().going, Vec::new())
}
