//! The todos, and the operations on them.
//!
//! The operations are free functions over a `Vec` rather than methods that
//! reach for global state. That is what lets them be tested against a local
//! list, with no shared state between tests and no reliance on the order they
//! run in.

use std::sync::Mutex;

/// One todo.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Todo {
    /// The identifier the DOM, the routes and the client all use.
    pub(crate) id: u32,
    /// What the viewer typed.
    pub(crate) title: String,
    /// Whether it is done.
    pub(crate) done: bool,
}

/// The list, as application data.
#[derive(Debug, Default)]
pub(crate) struct Todos(Mutex<Vec<Todo>>);

impl Todos {
    /// A list with something in it, so the example has something to show.
    pub(crate) fn seed() -> Self {
        let seeds = [
            ("Read the exos guide", true),
            ("Write the TodoMVC example", false),
            ("Open a second tab", false),
        ];

        let todos = seeds
            .iter()
            .enumerate()
            .map(|(index, (title, done))| Todo {
                id: u32::try_from(index).unwrap_or(0) + 1,
                title: (*title).to_owned(),
                done: *done,
            })
            .collect();

        Self(Mutex::new(todos))
    }

    /// A copy of the todos, for rendering.
    ///
    /// # Panics
    ///
    /// If the lock was poisoned by a panic in another thread while held.
    pub(crate) fn snapshot(&self) -> Vec<Todo> {
        self.0
            .lock()
            .expect("the store lock is never held across a panic")
            .clone()
    }

    /// Applies `change` to the todos and hands back whatever it decided.
    ///
    /// The result travels out because a handler usually has to answer
    /// differently depending on it: an accepted draft clears the field and a
    /// refused one leaves it alone.
    ///
    /// # Panics
    ///
    /// If the lock was poisoned; see [`snapshot`](Self::snapshot).
    pub(crate) fn update<T>(&self, change: impl FnOnce(&mut Vec<Todo>) -> T) -> T {
        let mut todos = self
            .0
            .lock()
            .expect("the store lock is never held across a panic");

        change(&mut todos)
    }
}

/// Appends a todo, and reports whether there was one to append.
///
/// Trimming and refusing an empty title is the list's rule rather than the
/// form's, so pressing enter on a field holding three spaces cannot add a todo
/// no matter which page did it.
///
/// The id is the highest one plus one, which keeps this a function of the list
/// alone. An example can afford that; an application that hands out ids a
/// viewer may still be holding wants a counter that never goes backwards.
pub(crate) fn add(todos: &mut Vec<Todo>, title: &str) -> bool {
    let title = title.trim();

    if title.is_empty() {
        return false;
    }

    let id = todos.iter().map(|todo| todo.id).max().unwrap_or(0) + 1;

    todos.push(Todo {
        id,
        title: title.to_owned(),
        done: false,
    });

    true
}

/// Flips one todo, and reports whether it existed.
pub(crate) fn toggle(todos: &mut [Todo], id: u32) -> bool {
    match todos.iter_mut().find(|todo| todo.id == id) {
        Some(todo) => {
            todo.done = !todo.done;
            true
        }
        None => false,
    }
}

/// Retitles one todo, or drops it when the title is emptied.
///
/// Emptying the field is how the classic application deletes from the editor,
/// and it is the same rule as refusing an empty draft: a todo with no title is
/// not a todo.
pub(crate) fn rename(todos: &mut Vec<Todo>, id: u32, title: &str) {
    let title = title.trim();

    if title.is_empty() {
        remove(todos, id);
        return;
    }

    if let Some(todo) = todos.iter_mut().find(|todo| todo.id == id) {
        todo.title = title.to_owned();
    }
}

/// Drops one todo.
pub(crate) fn remove(todos: &mut Vec<Todo>, id: u32) {
    todos.retain(|todo| todo.id != id);
}

/// Drops everything already done.
pub(crate) fn clear_completed(todos: &mut Vec<Todo>) {
    todos.retain(|todo| !todo.done);
}

/// Marks every todo one way.
pub(crate) fn set_all(todos: &mut [Todo], done: bool) {
    for todo in todos {
        todo.done = done;
    }
}

/// How many are still to do.
pub(crate) fn remaining(todos: &[Todo]) -> usize {
    todos.iter().filter(|todo| !todo.done).count()
}

/// How many are done.
pub(crate) fn completed(todos: &[Todo]) -> usize {
    todos.len() - remaining(todos)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh, local list. Nothing here touches global state, so these tests
    /// are independent of each other and of the order they run in.
    fn todos() -> Vec<Todo> {
        vec![
            Todo {
                id: 1,
                title: String::from("write it"),
                done: true,
            },
            Todo {
                id: 2,
                title: String::from("read it"),
                done: false,
            },
        ]
    }

    fn titles(todos: &[Todo]) -> Vec<&str> {
        todos.iter().map(|todo| todo.title.as_str()).collect()
    }

    #[test]
    fn adding_appends_the_trimmed_title() {
        let mut todos = todos();

        assert!(add(&mut todos, "  ship it  "));
        assert_eq!(titles(&todos), vec!["write it", "read it", "ship it"]);
    }

    #[test]
    fn adding_nothing_is_refused_rather_than_stored() {
        let mut todos = todos();

        assert!(!add(&mut todos, "   "));
        assert_eq!(todos.len(), 2);
    }

    #[test]
    fn a_new_todo_gets_an_id_of_its_own() {
        let mut todos = todos();
        add(&mut todos, "ship it");

        assert_eq!(todos[2].id, 3);
    }

    #[test]
    fn toggling_flips_one_todo_and_reports_that_it_existed() {
        let mut todos = todos();

        assert!(toggle(&mut todos, 2));
        assert!(todos[1].done);

        assert!(toggle(&mut todos, 2));
        assert!(!todos[1].done);
    }

    #[test]
    fn toggling_a_missing_todo_reports_failure() {
        assert!(!toggle(&mut todos(), 99));
    }

    #[test]
    fn renaming_replaces_the_title() {
        let mut todos = todos();
        rename(&mut todos, 1, "  rewrite it  ");

        assert_eq!(titles(&todos), vec!["rewrite it", "read it"]);
    }

    /// Emptying the editor is how the classic application deletes from it.
    #[test]
    fn renaming_to_nothing_drops_the_todo() {
        let mut todos = todos();
        rename(&mut todos, 1, "   ");

        assert_eq!(titles(&todos), vec!["read it"]);
    }

    #[test]
    fn clearing_keeps_only_what_is_left_to_do() {
        let mut todos = todos();
        clear_completed(&mut todos);

        assert_eq!(titles(&todos), vec!["read it"]);
    }

    #[test]
    fn marking_everything_is_all_or_nothing() {
        let mut todos = todos();

        set_all(&mut todos, true);
        assert_eq!(remaining(&todos), 0);

        set_all(&mut todos, false);
        assert_eq!(remaining(&todos), 2);
    }

    #[test]
    fn the_counts_split_the_list_between_them() {
        let todos = todos();

        assert_eq!(remaining(&todos), 1);
        assert_eq!(completed(&todos), 1);
    }
}
