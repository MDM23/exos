//! One todo: what a row shows, and what a click on it does.

use axum::extract::Path;
use exos::{
    Effect, Flag, Markup, Model, bind, class, data, focus_now, on_change, on_click, on_dblclick,
    on_focusout, on_keydown, on_submit, show, signal, view, when,
};
use serde::{Deserialize, Serialize};

use crate::{
    board::publish_board,
    store::{self, Todo, Todos},
};

/// What the editor holds, and what a rename sends.
///
/// One buffer for the page rather than one per row, because only one row is
/// ever being edited. A model's signals are named after the model and the
/// field and live on the document, so the row that seeds the buffer, the form
/// that binds it, the list that declares it and the handler that reads it back
/// all mean the same signal.
#[exos::model]
#[derive(Debug, Default, Deserialize, Serialize)]
pub(crate) struct Edit {
    /// The title being edited.
    pub(crate) title: String,
}

/// One row, in both of its states.
///
/// The row is rendered once and holds its viewer state in signals rather than
/// being re-fetched in an editing flavour: whether this row is being edited,
/// and whether it is on its way out, are things the server has no opinion
/// about and therefore nothing to disagree with.
pub(crate) fn item(todo: &Todo) -> Markup {
    let edit = Edit::signals();
    let id = todo.id;
    let editor = format!("edit-{id}");

    // Named after where they are declared, which makes them the same two names
    // in every row. The row's `id` is the scope they resolve in, so a hundred
    // rows hold a hundred pairs and nothing has to invent `editing_3`.
    let editing = signal(false);
    let gone = signal(false);

    view! {
        <li
            id={ format!("todo-{id}") }
            class="todo"
            data-done={ todo.done }
            {(&editing, &gone)}
            {show(!gone.get())}
            {class("editing", editing.get())}
        >
            <div class="view">
                <input
                    class="toggle"
                    type="checkbox"
                    aria-label="Done"
                    checked={ Flag(todo.done) }
                    {on_change(|_| toggle::post(id))}
                >

                <label {on_dblclick(|_| {
                    // Seeded from the server's title, so the editor opens on
                    // what is on screen rather than on whatever was edited
                    // last. The focus waits for the class that reveals the
                    // field, which the runtime handles.
                    edit.title.set(todo.title.clone());
                    editing.set(true);
                    focus_now(&format!("#{editor}"));
                })}>{ &todo.title }</label>

                <button
                    class="destroy"
                    type="button"
                    aria-label="Delete"
                    {on_click(|_| {
                        // Painted before the server has answered. There is no
                        // second copy of this to drift: the row is either gone
                        // from the next list or back in it.
                        gone.set(true);
                        destroy::delete(id);
                    })}
                ></button>
            </div>

            <form class="editor" {on_submit(|_| {
                rename::put(id, &edit);
                editing.set(false);
            })}>
                <input
                    class="edit"
                    type="text"
                    id={ &editor }
                    aria-label="Edit todo"
                    {bind(&edit.title)}
                    {on_keydown(|event| {
                        when(event.key().eq("Escape"), |()| editing.set(false));
                    })}
                    {on_focusout(|_| {
                        // Leaving the editor saves, as the classic application
                        // does. Escape has already turned editing off, so the
                        // blur that follows it saves nothing, and the guard is
                        // the whole difference between the two.
                        when(editing.get(), |()| {
                            rename::put(id, &edit);
                            editing.set(false);
                        });
                    })}
                >
            </form>
        </li>
    }
}

/// Flips one todo.
#[exos::post("/todos/{id}/done")]
async fn toggle(Path(id): Path<u32>) -> Effect {
    data::<Todos>().update(|todos| store::toggle(todos, id));

    // One line, and every tab showing this list updates, whichever view of it
    // it is showing.
    publish_board();
    Effect::none()
}

/// Retitles one todo, or drops it when the editor was emptied.
#[exos::put("/todos/{id}/title")]
async fn rename(Path(id): Path<u32>, Model(edit): Model<Edit>) -> Effect {
    data::<Todos>().update(|todos| store::rename(todos, id, &edit.title));

    publish_board();
    Effect::none()
}

/// Drops one todo.
#[exos::delete("/todos/{id}")]
async fn destroy(Path(id): Path<u32>) -> Effect {
    data::<Todos>().update(|todos| store::remove(todos, id));

    publish_board();
    Effect::none()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn todo(title: &str) -> Todo {
        Todo {
            id: 9,
            title: String::from(title),
            done: false,
        }
    }

    fn row(title: &str) -> String {
        item(&todo(title)).into_string()
    }

    #[test]
    fn a_title_is_escaped_rather_than_rendered() {
        let html = row(r#"<img src=x onerror="alert(1)">"#);

        assert!(html.contains("&lt;img src=x onerror=&quot;alert(1)&quot;&gt;"));
        assert!(!html.contains("<img"));
    }

    /// The row's id is what its two signals resolve against, so every row can
    /// declare the same two names without colliding.
    #[test]
    fn a_row_is_its_own_signal_scope() {
        let html = row("read it");

        assert!(html.contains("id=\"todo-9\""));
        assert!(html.contains("data-signals=\"{&quot;s"));
    }

    #[test]
    fn the_editor_binds_the_shared_buffer() {
        let html = row("read it");
        let title = Edit::signals().title;

        assert!(html.contains(&format!("data-bind=\"{}\"", title.name())));
        assert!(html.contains("id=\"edit-9\""));
    }

    /// Escape turns editing off and saves nothing; the blur that follows it
    /// finds the guard already false.
    #[test]
    fn escape_cancels_the_edit_rather_than_saving_it() {
        let html = row("read it");

        assert!(html.contains("data-on-keydown=\"if (ev.key === &quot;Escape&quot;)"));
        assert!(html.contains("data-on-focusout=\"if ($."));
    }

    /// The route, the path parameter and the payload all come from the
    /// handler's own signature, so changing any of them breaks this call site.
    #[test]
    fn the_editor_saves_with_the_whole_model() {
        let html = row("read it");
        let title = Edit::signals().title;

        assert!(html.contains("put(&quot;/todos/9/title&quot;"));
        assert!(html.contains(&format!("&quot;{0}&quot;: $.{0}", title.name())));
    }
}
