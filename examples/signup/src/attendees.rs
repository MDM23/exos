//! Rows the browser owns until the form is sent.
//!
//! A row is a model like any other and the form holds many of them, so one
//! `register` carries the whole guest list. Nothing here is a route: adding a
//! row clones a `<template>` the server rendered once, removing one takes the
//! element off the page, and neither costs a request. The server holds nothing
//! half-typed, because there is nothing of the rows on the server at all.

use exos::{Markup, Rows, RowsOf, bind, on_click, show, text, view};
use serde::{Deserialize, Serialize};

/// One person on the registration.
///
/// Its rules are declared where every other model's are and checked per row:
/// what is wrong with the third row's name lands on the third row.
#[exos::model]
#[derive(Debug, Default, Deserialize, Serialize)]
pub(crate) struct Attendee {
    /// Who they are.
    #[valid(required, length = 2..=40)]
    pub(crate) name: String,
}

/// The rows, and the button that adds one.
pub(crate) fn roster(rows: &RowsOf<Attendee>) -> Markup {
    view! {
        <div class="rows">
            {
                rows.each(|row| view! {
                    <div class="row" {row}>
                        <input
                            type="text"
                            aria-label="Attendee"
                            placeholder="Name"
                            {bind(&row.name)}
                        >

                        <button
                            type="button"
                            class="drop"
                            aria-label="Remove attendee"
                            {on_click(|_| rows.remove())}
                        >"x"</button>

                        <p class="error" {show(row.name.invalid())} {text(row.name.error())}></p>
                    </div>
                })
            }

            <button type="button" class="add" {on_click(|_| rows.add())}>
                "Add attendee"
            </button>

            // The rule about how many rows there are rather than about any one
            // of them. Only the server answers it: the rows are not a signal,
            // so there is nothing here to count.
            <p class="error" {show(rows.invalid())} {text(rows.error())}></p>
        </div>
    }
}

/// What the form opens with: one blank row, so there is somewhere to type.
pub(crate) fn opening() -> Rows<Attendee> {
    [Attendee::default()].into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::Attendee;
    use crate::{form::Signup, tests::get};

    /// The row markup is rendered twice: once into the template a new row is
    /// cloned from, and once per row the form opened with. One closure, so a
    /// row added later is the row that was already there.
    #[tokio::test]
    async fn the_row_markup_is_also_the_template_a_new_row_clones() {
        let html = get("/").await;
        let name = Signup::signals().attendees.key();

        assert!(
            html.contains(&format!("data-rows=\"{name}\"")),
            "{html:.3000}"
        );
        assert!(html.contains("<template>"), "{html:.3000}");

        // A bare attribute, so the group's own marker has to be discounted.
        let marks = html.matches("data-row").count() - html.matches("data-rows").count();
        assert_eq!(marks, 2, "the template and the one row it opens with");
    }

    /// A row added and never typed into still has to be a row. The template
    /// carries the row model's `Default`, not nothing: a field that started as
    /// `null` would be one the server cannot read back.
    #[tokio::test]
    async fn the_template_row_starts_where_a_default_row_starts() {
        let html = get("/").await;
        let field = <Attendee as exos::RowModel>::keys()[0];

        assert!(
            !html.contains(&format!("{field}&quot;:null")),
            "{html:.3000}"
        );
        assert_eq!(
            html.matches(&format!("{field}&quot;:&quot;&quot;")).count(),
            2,
            "the template and the row, both empty strings"
        );
    }

    /// Adding and removing are recorded expressions, not routes. Nothing about
    /// a row reaches the server until the form is submitted.
    #[tokio::test]
    async fn a_row_is_added_and_dropped_without_a_round_trip() {
        let html = get("/").await;

        assert!(html.contains("addRow(el, "), "{html:.3000}");
        assert!(html.contains("dropRow(el, "), "{html:.3000}");
        assert!(
            !html.contains("/attendees"),
            "no route for a row: {html:.3000}"
        );
    }

    /// Every row carries the same field name, because there is one call site.
    /// What tells them apart is the element each one is declared on, which is
    /// the same thing that has always given a row its own signal.
    #[tokio::test]
    async fn every_row_declares_one_name_and_holds_its_own_value() {
        let html = get("/").await;
        let form = Signup::signals();
        let field = <Attendee as exos::RowModel>::keys()[0];

        assert_eq!(html.matches(&format!("data-bind=\"{field}\"")).count(), 2);
        assert!(
            html.contains(&format!("data-bind-rows=\"{}\"", form.attendees.key())),
            "{html:.3000}"
        );
    }

    /// The submission reads the rows out of the group when it is sent, so what
    /// is on screen is what goes, however many were added since it rendered.
    #[tokio::test]
    async fn the_payload_collects_the_rows_at_the_moment_it_is_sent() {
        let html = get("/").await;
        let form = Signup::signals();

        assert!(
            html.contains(&format!("rows(el, &quot;{}&quot;", form.attendees.key())),
            "{html:.3000}"
        );
    }
}
