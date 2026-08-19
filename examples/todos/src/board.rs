//! The list itself, and the actions that act on all of it.
//!
//! One live fragment per filter. A topic has to completely determine its
//! content, so "the list, filtered to active" is a different fragment from
//! "the list", rather than one fragment that would mean something different in
//! every tab.

use exos::{Effect, Flag, Markup, data, on_change, on_click, publish, view};

use crate::{
    filter::Filter,
    item::{Edit, item},
    store::{self, Todos},
};

/// The list and its summary, as one fragment.
///
/// Everything below is the server's state, which is why it is markup rather
/// than signals: an empty list is a section that was never rendered, and the
/// count is a number the server already knew.
#[exos::live]
pub(crate) fn board(filter: Filter) -> Markup {
    let todos = data::<Todos>().snapshot();

    if todos.is_empty() {
        // Native control flow, at render time, on the server. Nothing about
        // this has to reach the browser.
        return Markup::default();
    }

    let remaining = store::remaining(&todos);
    let completed = store::completed(&todos);

    // One editing buffer for the whole list, declared where the rows that
    // share it are. Being a model it lands on the document, so the row that
    // seeds it and the handler that reads it back agree without this having to
    // sit on some ancestor that has nothing to do with editing.
    let edit = Edit::signals();

    view! {
        <section class="main">
            <div class="mark-all">
                <input
                    id="toggle-all"
                    type="checkbox"
                    checked={ Flag(remaining == 0) }
                    {on_change(|_| toggle_all::post())}
                >
                <label for="toggle-all">"Mark all as complete"</label>
            </div>

            <ul class="todo-list" {&edit}>
                {
                    todos
                        .iter()
                        .filter(|todo| filter.keeps(todo))
                        .map(item)
                        .collect::<Vec<_>>()
                }
            </ul>
        </section>

        <footer class="footer">
            <span class="todo-count">
                <strong>{ remaining }</strong>
                { left(remaining) }
            </span>

            <ul class="filters">
                {
                    Filter::ALL
                        .iter()
                        .map(|link| view! {
                            <li>
                                <a
                                    href={ link.path() }
                                    aria-current={ link.current(filter) }
                                >{ link.label() }</a>
                            </li>
                        })
                        .collect::<Vec<_>>()
                }
            </ul>

            // Present or absent rather than "false": `[hidden]` is a selector
            // the browser itself acts on, and `hidden="false"` would hide the
            // button.
            <button
                class="clear-completed"
                type="button"
                hidden={ Flag(completed == 0) }
                {on_click(|_| clear_completed::delete())}
            >"Clear completed"</button>
        </footer>
    }
}

/// Pushes the list to every tab, whichever view of it that tab is showing.
///
/// Every action ends here, and that one line is what keeps two tabs in step.
/// A filtered list is one topic per filter, so publishing it is publishing all
/// three: a tab receives the one it is subscribed to and nothing else.
pub(crate) fn publish_board() {
    for filter in Filter::ALL {
        publish(|| board(filter));
    }
}

/// What the count says next to the number.
const fn left(remaining: usize) -> &'static str {
    if remaining == 1 {
        " item left"
    } else {
        " items left"
    }
}

/// Marks everything done, unless everything already is.
///
/// The server decides which way the switch goes, because it is the one holding
/// the list. The checkbox is server-rendered from the same fact, so the patch
/// that follows agrees with it.
#[exos::post("/todos/done")]
async fn toggle_all() -> Effect {
    data::<Todos>().update(|todos| {
        let done = store::remaining(todos) > 0;
        store::set_all(todos, done);
    });

    publish_board();
    Effect::none()
}

/// Drops everything already done.
#[exos::delete("/todos/completed")]
async fn clear_completed() -> Effect {
    data::<Todos>().update(store::clear_completed);

    publish_board();
    Effect::none()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::seed;

    fn markup(filter: Filter) -> String {
        seed();
        board(filter).to_markup().into_string()
    }

    /// Being able to subscribe is the authorization, so the wrapper carries a
    /// token the client could not have produced.
    #[test]
    fn the_list_is_a_live_fragment() {
        let html = markup(Filter::All);

        assert!(html.starts_with("<exos-live style=\"display:contents\" id=\"live-board-"));
        assert!(html.contains("data-token=\""));
    }

    /// Two tabs on different views must not share a fragment, or each would
    /// receive the other's list.
    #[test]
    fn every_view_is_a_topic_of_its_own() {
        seed();

        let mut topics: Vec<String> = Filter::ALL
            .iter()
            .map(|filter| board(*filter).topic().as_str().to_owned())
            .collect();

        topics.sort();
        topics.dedup();

        assert_eq!(topics.len(), Filter::ALL.len());
    }

    #[test]
    fn a_view_shows_only_what_it_keeps() {
        let active = markup(Filter::Active);
        let completed = markup(Filter::Completed);

        assert!(active.contains("Open a second tab"));
        assert!(!active.contains("Read the exos guide"));

        assert!(completed.contains("Read the exos guide"));
        assert!(!completed.contains("Open a second tab"));
    }

    #[test]
    fn the_footer_counts_what_is_left_rather_than_what_is_shown() {
        let seeded = markup(Filter::Completed);

        assert!(seeded.contains("<strong>2</strong> items left"), "{seeded}");
    }

    #[test]
    fn the_current_view_is_the_only_one_marked() {
        let html = markup(Filter::Active);

        assert_eq!(html.matches("aria-current=\"page\"").count(), 1);
        assert!(html.contains("<a href=\"/active\" aria-current=\"page\">Active</a>"));
    }

    #[test]
    fn one_item_left_is_not_one_items_left() {
        assert_eq!(left(1), " item left");
        assert_eq!(left(0), " items left");
        assert_eq!(left(2), " items left");
    }
}
