//! Adding a todo: the field, what it holds, and the action that accepts it.

use exos::{Effect, Markup, Model, bind, data, on_submit, view};
use serde::{Deserialize, Serialize};

use crate::{
    board::publish_board,
    store::{self, Todos},
};

/// What the new-todo field holds, and what submitting it sends.
///
/// One declaration for both. The template binds `draft.title`, the handler
/// takes `Model<Draft>` and the effect writes the same handle back, so
/// renaming the field breaks all three at once and none of them silently.
#[exos::model]
#[derive(Debug, Default, Deserialize, Serialize)]
pub(crate) struct Draft {
    /// The title being typed.
    pub(crate) title: String,
}

/// The banner and the field a todo is typed into.
pub(crate) fn composer() -> Markup {
    // Declared on the form, which is the markup it is about. Being a model it
    // lands on the document, so the handler below reaches it from here.
    let draft = Draft::signals();

    view! {
        <header class="header">
            <h1>"todos"</h1>

            // A form, so enter submits it and the field is what a browser
            // thinks it is. The runtime suppresses the default submission,
            // which is why nothing here has to say so.
            <form {&draft} {on_submit(|_| add::post(&draft))}>
                <input
                    class="new-todo"
                    type="text"
                    placeholder="What needs to be done?"
                    aria-label="New todo"
                    autofocus
                    {bind(&draft.title)}
                >
            </form>
        </header>
    }
}

/// Accepts a draft, or does not.
#[exos::post("/todos")]
async fn add(Model(draft): Model<Draft>) -> Effect {
    if !data::<Todos>().update(|todos| store::add(todos, &draft.title)) {
        // Nothing was added, so the field keeps what the viewer typed. A
        // stray enter is not a way to lose it.
        return Effect::none();
    }

    publish_board();

    // Cleared by the server rather than by the click, so the field empties
    // exactly when the todo was really accepted. The handle names the signal,
    // so this cannot drift from what the template declared.
    Effect::set(&Draft::signals().title, String::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{get, post};

    #[test]
    fn the_field_is_bound_to_the_draft() {
        let title = Draft::signals().title;
        let html = composer().into_string();

        assert!(html.contains(&format!("data-bind=\"{}\"", title.name())));
        assert!(html.contains("data-bind-kind=\"string\""));
    }

    /// Declared on the form and not on some ancestor: being a model field, it
    /// lands on the document from there, which is what lets the handler clear
    /// it with `Effect::set`.
    #[test]
    fn the_form_declares_the_draft_on_the_document() {
        let title = Draft::signals().title;
        let html = composer().into_string();

        // The record a model's messages land in is declared beside its fields,
        // whether or not this model has any rules to produce one.
        assert!(html.contains(&format!(
            "<form data-signals-root=\"{{&quot;{}&quot;:&quot;&quot;,&quot;{}&quot;:{{}}}}\"",
            title.name(),
            <Draft as exos::Validate>::STATE,
        )));
    }

    /// The call carries the model and nothing else: one entry per field, under
    /// the generated key rather than the field's own name.
    #[tokio::test]
    async fn submitting_sends_exactly_the_draft() {
        let html = get("/").await;
        let title = Draft::signals().title;

        assert!(
            html.contains("data-on-submit=\"post(&quot;/todos&quot;"),
            "{html:.900}"
        );
        assert!(html.contains(&format!("&quot;{0}&quot;: $.{0}", title.name())));
    }

    /// A refused draft answers with nothing to do, which is what leaves the
    /// half-typed title where it is.
    #[tokio::test]
    async fn an_empty_draft_neither_adds_nor_clears() {
        let answer = post(
            "/todos",
            &Draft {
                title: String::from("   "),
            },
        )
        .await;

        assert!(answer.steps().is_empty(), "{:?}", answer.steps());
    }
}
