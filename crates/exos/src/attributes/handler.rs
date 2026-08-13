//! Event handlers, and the event they receive.

use crate::{
    Js,
    attributes::{Attr, recorded},
    emit,
};

/// The DOM event, inside a handler.
///
/// Every accessor builds an expression; nothing is read at render time.
#[derive(Clone, Copy, Debug, Default)]
pub struct Event;

impl Event {
    /// The element the event came from.
    #[must_use]
    pub fn target(self) -> Target {
        Target
    }

    /// The key that was pressed.
    #[must_use]
    pub fn key(self) -> Js<String> {
        Js::raw("ev.key")
    }

    /// Suppresses the browser's default behaviour.
    ///
    /// # Panics
    ///
    /// If called outside a handler; see [`emit`].
    pub fn prevent_default(self) {
        emit("ev.preventDefault()");
    }

    /// Stops the event travelling further up the tree.
    ///
    /// # Panics
    ///
    /// If called outside a handler; see [`emit`].
    pub fn stop_propagation(self) {
        emit("ev.stopPropagation()");
    }
}

/// The element an event came from.
#[derive(Clone, Copy, Debug, Default)]
pub struct Target;

impl Target {
    /// The control's value.
    #[must_use]
    pub fn value(self) -> Js<String> {
        Js::raw("ev.target.value")
    }

    /// Whether a checkbox or radio is checked.
    #[must_use]
    pub fn checked(self) -> Js<bool> {
        Js::raw("ev.target.checked")
    }
}

/// A delegated handler for any event.
///
/// The closure records; it does not run in the browser itself.
#[must_use]
pub fn on(event: &str, handler: impl FnOnce(Event)) -> Attr {
    Attr::new(format!("data-on-{event}"), recorded(|| handler(Event)))
}

/// A delegated `click` handler.
#[must_use]
pub fn on_click(handler: impl FnOnce(Event)) -> Attr {
    on("click", handler)
}

/// A delegated `input` handler.
#[must_use]
pub fn on_input(handler: impl FnOnce(Event)) -> Attr {
    on("input", handler)
}

/// A delegated `change` handler.
#[must_use]
pub fn on_change(handler: impl FnOnce(Event)) -> Attr {
    on("change", handler)
}

/// A delegated `submit` handler. The default submission is suppressed.
#[must_use]
pub fn on_submit(handler: impl FnOnce(Event)) -> Attr {
    on("submit", handler)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        attributes::{Attributes, IntoAttributes},
        signal,
    };

    #[test]
    fn a_handler_records_its_body() {
        let gone = signal(false);
        let mut attributes = Attributes::new();

        on_click(|_| gone.set(true)).write(&mut attributes);

        assert_eq!(
            attributes.render(),
            format!(" data-on-click=\"$.{} = true\"", gone.name())
        );
    }

    #[test]
    fn the_event_builds_expressions_rather_than_reading_anything() {
        let mut attributes = Attributes::new();

        on_change(|event| {
            event.prevent_default();
        })
        .write(&mut attributes);

        assert!(attributes.render().contains("ev.preventDefault()"));
    }
}
