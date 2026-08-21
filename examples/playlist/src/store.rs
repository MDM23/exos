//! The room's records: the queue, and what can be done to it.
//!
//! The operations are free functions over a [`Queue`] rather than methods that
//! reach for global state, which is what lets them be tested against a local
//! one with no shared state between tests and no reliance on the order they
//! run in.
//!
//! **The list does not move on its own.** What is playing is a mark on a row,
//! so a track ending travels the mark down the queue and leaves every row
//! where it was. Rotating the list instead would mean the thing a listener is
//! reaching for moves while they reach, and only a drag should ever reorder
//! what somebody is looking at.

use std::sync::Mutex;

use crate::sleeve::Sleeve;

/// One track.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Track {
    /// The identifier the DOM, the routes and the drag all use.
    pub(crate) id: u32,
    pub(crate) title: String,
    pub(crate) artist: String,
    /// Whether somebody in the room hearted it. Anybody can, and it is the
    /// room's rather than any one listener's.
    pub(crate) hearted: bool,
    /// The art, and the blur it decodes to before it arrives.
    pub(crate) cover: Sleeve,
}

/// The queue, and which of it is playing.
#[derive(Clone, Debug, Default)]
pub(crate) struct Queue {
    pub(crate) tracks: Vec<Track>,
    /// The track that is on, by id.
    ///
    /// An id rather than a position, which is the whole reason a drag needs no
    /// special case: move a row and the mark goes with it, and reordering the
    /// queue cannot change what is playing.
    pub(crate) playing: u32,
}

impl Queue {
    /// The track that is on.
    #[must_use]
    pub(crate) fn playing(&self) -> Option<&Track> {
        self.tracks.iter().find(|track| track.id == self.playing)
    }

    /// Whether `id` is the one on.
    #[must_use]
    pub(crate) fn is_playing(&self, id: u32) -> bool {
        self.playing == id
    }
}

/// The room, as application data.
#[derive(Debug, Default)]
pub(crate) struct Room(Mutex<Queue>);

impl Room {
    /// A queue with something in it, so the example has something to play.
    #[must_use]
    pub(crate) fn seed() -> Self {
        // The art is embedded here and hashed here. `asset!` puts the file in
        // the binary and hands back the name it is served under, and
        // [`Sleeve`] reads the same bytes to work out what to draw while it
        // loads, so a swapped sleeve needs nothing else touched.
        let seeds = [
            (
                "Coffee and a Compiler",
                "The Borrow Checkers",
                exos::asset!("img/coffee.png"),
            ),
            (
                "Slow Morning",
                "Hana Reyes",
                exos::asset!("img/morning.png"),
            ),
            (
                "Nothing To Declare",
                "Public Interface",
                exos::asset!("img/declare.png"),
            ),
            ("Tail Call", "Recursion", exos::asset!("img/tail-call.png")),
            (
                "Held Across an Await",
                "The Deadlocks",
                exos::asset!("img/await.png"),
            ),
            (
                "Last Orders",
                "Closing Time",
                exos::asset!("img/last-orders.png"),
            ),
        ];

        let tracks: Vec<Track> = seeds
            .iter()
            .enumerate()
            .map(|(index, (title, artist, art))| Track {
                id: u32::try_from(index).unwrap_or(0) + 1,
                title: (*title).to_owned(),
                artist: (*artist).to_owned(),
                hearted: false,
                cover: Sleeve::new(*art),
            })
            .collect();

        Self(Mutex::new(Queue {
            playing: tracks.first().map_or(0, |track| track.id),
            tracks,
        }))
    }

    /// A copy of the room, for rendering.
    ///
    /// # Panics
    ///
    /// If the lock was poisoned by a panic in another thread while held.
    #[must_use]
    pub(crate) fn snapshot(&self) -> Queue {
        self.0
            .lock()
            .expect("the store lock is never held across a panic")
            .clone()
    }

    /// Applies `change` to the room and hands back whatever it decided.
    ///
    /// # Panics
    ///
    /// If the lock was poisoned; see [`snapshot`](Self::snapshot).
    pub(crate) fn update<T>(&self, change: impl FnOnce(&mut Queue) -> T) -> T {
        change(
            &mut self
                .0
                .lock()
                .expect("the store lock is never held across a panic"),
        )
    }
}

/// Moves the mark to the next track, wrapping at the end.
///
/// Nothing else moves. The room loops rather than running out, which is what a
/// party playlist does and what keeps the example worth leaving open.
pub(crate) fn advance(queue: &mut Queue) {
    let at = queue
        .tracks
        .iter()
        .position(|track| track.id == queue.playing);

    let next = match at {
        Some(at) => (at + 1) % queue.tracks.len().max(1),
        // The marked track is not here, which cannot happen while the room
        // refuses to remove it. Starting again beats stopping.
        None => 0,
    };

    if let Some(track) = queue.tracks.get(next) {
        queue.playing = track.id;
    }
}

/// Takes `ids` out of the room, and names what it would not take.
///
/// **The room will not remove what it is playing.** That is the whole of the
/// rule, and it is why this example needs no simulated failure: a refusal is
/// something a listener can ask for on purpose, watch the optimistic paint
/// undo itself, and understand without being told.
///
/// It also means the queue can never be emptied, so there is always something
/// on and the mark always has a row to sit on.
pub(crate) fn remove(queue: &mut Queue, ids: &[u32]) -> Option<String> {
    let playing = queue.playing;

    let refused = queue
        .playing()
        .filter(|track| ids.contains(&track.id))
        .map(|track| track.title.clone());

    queue
        .tracks
        .retain(|track| !ids.contains(&track.id) || track.id == playing);

    refused
}

/// Turns the heart on a track, and says which way it went.
pub(crate) fn heart(queue: &mut Queue, id: u32) -> bool {
    queue
        .tracks
        .iter_mut()
        .find(|track| track.id == id)
        .map(|track| {
            track.hearted = !track.hearted;
            track.hearted
        })
        .unwrap_or_default()
}

/// Puts the queue in the order a drag ended in.
///
/// Every row can be dragged, the one playing included, because the mark is an
/// id and follows it. Moving what is on changes where it sits and not what is
/// on, which is the behaviour a listener would expect and the reason this
/// needs no rule of its own.
pub(crate) fn reorder(queue: &mut Queue, order: &[String]) {
    // Stable, so anything the drag did not mention keeps its place behind
    // everything it did.
    queue.tracks.sort_by_key(|track| {
        order
            .iter()
            .position(|id| id.parse::<u32>() == Ok(track.id))
            .unwrap_or(usize::MAX)
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh, local room. Nothing here touches global state, so these tests
    /// are independent of each other and of their running order.
    fn room() -> Queue {
        let tracks = ["one", "two", "three"]
            .iter()
            .enumerate()
            .map(|(index, title)| Track {
                id: u32::try_from(index).unwrap_or(0) + 1,
                title: (*title).to_owned(),
                artist: String::from("somebody"),
                hearted: false,
                cover: Sleeve::new(exos::asset!("img/coffee.png")),
            })
            .collect();

        Queue { tracks, playing: 1 }
    }

    /// Owned, so a test can hold the order from before a change and compare it
    /// with the order after one.
    fn titles(queue: &Queue) -> Vec<String> {
        queue
            .tracks
            .iter()
            .map(|track| track.title.clone())
            .collect()
    }

    #[test]
    fn the_mark_names_one_track() {
        let queue = room();

        assert_eq!(queue.playing().expect("something is on").title, "one");
        assert!(queue.is_playing(1));
        assert!(!queue.is_playing(2));
    }

    /// The point of the whole shape: the mark moves and the rows do not.
    #[test]
    fn advancing_moves_the_mark_and_leaves_the_list_alone() {
        let mut queue = room();
        let before = titles(&queue);

        advance(&mut queue);

        assert_eq!(queue.playing().expect("something is on").title, "two");
        assert_eq!(titles(&queue), before, "nothing moved");
    }

    #[test]
    fn the_mark_wraps_at_the_end() {
        let mut queue = room();

        advance(&mut queue);
        advance(&mut queue);
        assert!(queue.is_playing(3));

        advance(&mut queue);
        assert!(queue.is_playing(1), "and round again");
        assert_eq!(titles(&queue), ["one", "two", "three"]);
    }

    #[test]
    fn a_room_with_one_track_keeps_playing_it() {
        let mut queue = room();
        queue.tracks.truncate(1);

        advance(&mut queue);

        assert!(queue.is_playing(1));
    }

    #[test]
    fn advancing_an_empty_room_is_not_a_panic() {
        let mut queue = Queue::default();

        advance(&mut queue);

        assert!(queue.playing().is_none());
    }

    #[test]
    fn removing_takes_the_tracks_it_was_given() {
        let mut queue = room();

        assert!(remove(&mut queue, &[2, 3]).is_none(), "nothing refused");
        assert_eq!(titles(&queue), ["one"]);
    }

    /// The rule the whole example is built on, and the reason it needs no
    /// switch labelled "simulate a server error".
    #[test]
    fn the_room_will_not_remove_what_it_is_playing() {
        let mut queue = room();

        let refused = remove(&mut queue, &[1, 2]).expect("the playing track is named");

        assert_eq!(refused, "one");
        assert_eq!(titles(&queue), ["one", "three"], "and the rest still went");
    }

    /// Which is also what keeps the mark on something.
    #[test]
    fn the_queue_cannot_be_emptied() {
        let mut queue = room();

        drop(remove(&mut queue, &[1, 2, 3]));

        assert_eq!(titles(&queue), ["one"]);
        assert!(queue.playing().is_some());
    }

    #[test]
    fn a_heart_goes_both_ways() {
        let mut queue = room();

        assert!(heart(&mut queue, 2));
        assert!(queue.tracks[1].hearted);

        assert!(!heart(&mut queue, 2));
        assert!(!queue.tracks[1].hearted);

        assert!(!heart(&mut queue, 99), "and a track that is not here");
    }

    #[test]
    fn a_drag_reorders_the_queue() {
        let mut queue = room();

        reorder(
            &mut queue,
            &[String::from("3"), String::from("1"), String::from("2")],
        );

        assert_eq!(titles(&queue), ["three", "one", "two"]);
    }

    /// The mark follows the track rather than the position, so dragging what
    /// is playing moves where it sits and not what is on.
    #[test]
    fn a_drag_cannot_change_what_is_playing() {
        let mut queue = room();

        reorder(&mut queue, &[String::from("2"), String::from("1")]);

        assert_eq!(titles(&queue), ["two", "one", "three"]);
        assert!(queue.is_playing(1));
        assert_eq!(queue.playing().expect("something is on").title, "one");
    }

    #[test]
    fn reordering_an_empty_room_is_not_a_panic() {
        let mut queue = Queue::default();

        reorder(&mut queue, &[String::from("1")]);

        assert!(queue.tracks.is_empty());
    }
}
