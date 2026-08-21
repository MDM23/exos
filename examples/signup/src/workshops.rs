//! The searchable multi-select, built out of what there is.
//!
//! There is no client-side loop, and this needs none: the server renders every
//! option and every chip once, each carrying its own condition, so filtering
//! and showing what is picked are both per-element visibility. What that costs
//! is that the whole programme is in the document whether or not it is on
//! screen, which is right for nine workshops and wrong for nine thousand.

use exos::{IntoJs as _, Js, Markup, Signal, bind, data, on_click, show, signal, text, view};

use crate::store::Programme;

/// The dropdown: what is picked, a search box, and one checkbox per workshop.
pub(crate) fn picker(picked: &Signal<Vec<u32>>, error: &Signal<String>) -> Markup {
    let open = signal(false);
    let query = signal(String::new());
    let programme = data::<Programme>();

    view! {
        <div class="picker" {(&open, &query)}>
            // aria-expanded is missing on purpose. `attr` writes an empty
            // attribute for a true boolean, and `aria-expanded=""` is read as
            // false, so saying it needs a conditional expression the
            // combinators do not have.
            <button
                type="button"
                id="workshops"
                class="toggle"
                aria-haspopup="listbox"
                {on_click(|_| open.toggle())}
            >
                // Three elements rather than one expression, because nothing
                // concatenates a count onto a word.
                <span {show(picked.get().is_empty())}>"Choose workshops"</span>
                <span {show(picked.get().any())}>
                    <span {text(picked.get().len())}></span>" selected"
                </span>
            </button>

            <div class="chips">
                {
                    programme
                        .all()
                        .iter()
                        .map(|workshop| view! {
                            <span class="chip" {show(picked.get().contains(workshop.id))}>
                                { &workshop.title }
                            </span>
                        })
                        .collect::<Vec<_>>()
                }
            </div>

            // Closing is a button, because nothing can say that focus left the
            // widget: a click anywhere else reaches no handler at all.
            <div class="list" {show(open.get())}>
                <input
                    type="search"
                    class="search"
                    placeholder="Search the programme"
                    aria-label="Search the programme"
                    {bind(&query)}
                >

                {
                    programme
                        .all()
                        .iter()
                        .map(|workshop| {
                            let haystack =
                                format!("{} {}", workshop.title, workshop.track).to_lowercase();

                            view! {
                                <label
                                    class="option"
                                    {show(
                                        query
                                            .get()
                                            .is_empty()
                                            .or(haystack.into_js().contains(folded(&query)))
                                    )}
                                >
                                    <input
                                        type="checkbox"
                                        value={ workshop.id }
                                        {bind(picked)}
                                    >
                                    <span class="title">{ &workshop.title }</span>
                                    <span class="track">{ &workshop.track }</span>
                                </label>
                            }
                        })
                        .collect::<Vec<_>>()
                }

                <button type="button" class="close" {on_click(|_| open.set(false))}>
                    "Done"
                </button>
            </div>

            <p class="error" {show(!error.get().is_empty())} {text(error.get())}></p>
        </div>
    }
}

/// The query, lowercased.
///
/// Through the escape hatch, because `Js<String>` has `contains` and `trim`
/// and no case folding, and a search box that only matches the capitalisation
/// somebody happened to type is not a search box.
fn folded(query: &Signal<String>) -> Js<String> {
    Js::raw(format!("{}.toLowerCase()", query.get().source()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{get, seed};

    /// Every option is in the document, and each decides for itself whether it
    /// is on screen. That is the whole of what stands in for a client-side
    /// loop.
    #[tokio::test]
    async fn every_workshop_is_rendered_once_and_filtered_by_a_condition() {
        seed();

        let html = get("/").await;
        let offered = data::<Programme>().all().len();

        assert_eq!(html.matches("class=\"option\"").count(), offered);
        assert_eq!(html.matches("class=\"chip\"").count(), offered);
    }

    /// The filter reads the query signal rather than round-tripping, and folds
    /// case on both sides or it matches nothing anybody types.
    #[tokio::test]
    async fn the_filter_is_a_condition_over_the_query() {
        seed();

        let html = get("/").await;

        assert!(html.contains("async rust from the bottom up monday"));
        assert!(html.contains(".toLowerCase())"));
    }

    /// A checkbox collects into the model's `Vec`, which is the whole of what
    /// multiple selection needs.
    #[tokio::test]
    async fn picking_collects_into_the_model() {
        seed();

        let html = get("/").await;
        let workshops = crate::form::Signup::signals().workshops;

        assert!(html.contains(&format!("data-bind=\"{}\"", workshops.name())));
        assert!(html.contains("data-bind-kind=\"number\""));
    }
}
