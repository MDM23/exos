//! The example's markup.

use exos::{Markup, Page, attr_now, bind, on_click, show, signal, text, view, when};

use crate::{SelectionSignals, archive, delete_file, favorite, presence, store::Entry};

/// The document every page sits in.
pub(crate) fn layout(title: &str, path: &str, body: Markup) -> Page {
    Page(view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8">
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <title>{ title }</title>
                <link rel="stylesheet" href={ exos::asset("app.css") }>
                <script defer src={ exos::asset("exos.js") }></script>
            </head>
            <body>
                <nav class="site-nav">
                    <div class="shell">
                        <a href="/" aria-current={ current(path, "/") }>"Files"</a>
                        <a href="/about" aria-current={ current(path, "/about") }>"About"</a>
                    </div>
                </nav>
                <main class="shell">{ body }</main>
            </body>
        </html>
    })
}

/// `aria-current` is what a screen reader announces, so the styling keys off
/// the same attribute rather than a parallel class that could drift from it.
fn current(path: &str, href: &str) -> Option<&'static str> {
    (path == href).then_some("page")
}

/// One row.
pub(crate) fn row(entry: &Entry, selection: &SelectionSignals) -> Markup {
    let id = format!("file-{}", entry.id);
    let entry_id = entry.id;
    let favourited = entry.favorite;

    // Whether this row is pending deletion is not something the server has an
    // opinion about, so it is a real signal. It is never declared: the macro
    // finds the reference below and declares it as null.
    let gone = signal!(_gone = false);

    // `data-favorite` is not mirrored into a signal. The server owns it, the
    // click writes it speculatively, and the patch that follows overwrites it
    // either way. One source of truth, so nothing can drift.
    view! {
        <li
            id={ id }
            class="file-entry"
            data-sort-item={ entry.id }
            data-favorite={ entry.favorite }
            {&gone}
            {show(!gone.get())}
        >
            <input type="checkbox" value={ entry.id } {bind(&selection.picked)}>

            <span class="handle" data-drag-handle aria-hidden="true">"::"</span>

            <button
                class="star"
                type="button"
                aria-label="Favourite"
                {on_click(move |_| {
                    attr_now("data-favorite", !favourited);
                    favorite::post(entry_id, selection);
                })}
            >"*"</button>

            <span class="file-name">{ &entry.name }</span>

            { presence(entry.owner) }

            <button
                class="danger"
                type="button"
                {on_click(move |_| {
                    gone.set(true);
                    delete_file::post(entry_id, selection);
                })}
            >"Delete"</button>
        </li>
    }
}

/// The bar that appears once something is checked.
pub(crate) fn selection_bar(selection: &SelectionSignals) -> Markup {
    view! {
        <div class="bar" {show(selection.picked.get().any())}>
            <span {text(selection.picked.get().len())}></span>
            " selected"

            <button
                class="danger"
                type="button"
                {on_click(|_| {
                    // A no-op when nothing is picked, so the button cannot
                    // fire an empty batch even if it is somehow clicked.
                    when(selection.picked.get().any(), |()| archive::post(selection));
                })}
            >"Archive"</button>

            <button type="button" {on_click(|_| selection.picked.clear())}>
                "Clear selection"
            </button>
        </div>
    }
}
