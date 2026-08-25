//! Event handlers, the events they listen for, and the event they receive.

use crate::{
    Js,
    attributes::{Attr, recorded},
    emit,
};

/// An event type the runtime delegates.
///
/// The runtime listens for one of each on `document` and dispatches on
/// attributes at event time, which is what makes markup that arrives ten
/// minutes after page load already wired. The flip side is that the set is
/// closed: a handler for anything else would sit in the DOM and never fire, so
/// the set is a type rather than a string.
///
/// Anything outside it is registered from JavaScript with
/// `window.exos.listen`, and named here with [`Custom`](Self::Custom).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum EventType {
    /// A control's value was committed: a checkbox, a radio, a `<select>`.
    Change,
    /// A pointer click.
    Click,
    /// A double click.
    DblClick,
    /// Focus arrived. Unlike `focus`, this one bubbles, which is what
    /// delegation needs.
    FocusIn,
    /// Focus left. The bubbling counterpart of `blur`.
    FocusOut,
    /// A control's value changed as it is being edited.
    Input,
    /// A key went down.
    KeyDown,
    /// A key came back up.
    KeyUp,
    /// A pointer went down.
    PointerDown,
    /// A pointer came back up.
    PointerUp,
    /// A form was submitted. The runtime suppresses the browser's own
    /// submission.
    Submit,
    /// An event type the application registered itself.
    ///
    /// The runtime only delegates what it was told to, so this is a promise
    /// that `window.exos.listen("...")` has been called for the same name.
    Custom(&'static str),
}

impl EventType {
    /// The DOM name, which is also what the attribute is spelled with.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Change => "change",
            Self::Click => "click",
            Self::Custom(name) => name,
            Self::DblClick => "dblclick",
            Self::FocusIn => "focusin",
            Self::FocusOut => "focusout",
            Self::Input => "input",
            Self::KeyDown => "keydown",
            Self::KeyUp => "keyup",
            Self::PointerDown => "pointerdown",
            Self::PointerUp => "pointerup",
            Self::Submit => "submit",
        }
    }
}

/// The DOM event, inside a handler.
///
/// Every accessor builds an expression; nothing is read at render time.
#[derive(Clone, Copy, Debug, Default)]
pub struct Event;

impl Event {
    /// The element the event came from.
    pub fn target(self) -> Target {
        Target
    }

    /// The key that was pressed.
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
    pub fn value(self) -> Js<String> {
        Js::raw("ev.target.value")
    }

    /// Whether a checkbox or radio is checked.
    pub fn checked(self) -> Js<bool> {
        Js::raw("ev.target.checked")
    }
}

/// A delegated handler for any event the runtime carries.
///
/// The closure records; it does not run in the browser itself.
pub fn on(event: EventType, handler: impl FnOnce(Event)) -> Attr {
    Attr::new(
        format!("data-on-{}", event.name()),
        recorded(|| handler(Event)),
    )
}

/// A delegated `change` handler.
pub fn on_change(handler: impl FnOnce(Event)) -> Attr {
    on(EventType::Change, handler)
}

/// A delegated `click` handler.
pub fn on_click(handler: impl FnOnce(Event)) -> Attr {
    on(EventType::Click, handler)
}

/// A delegated `dblclick` handler.
pub fn on_dblclick(handler: impl FnOnce(Event)) -> Attr {
    on(EventType::DblClick, handler)
}

/// A delegated `focusout` handler.
///
/// The bubbling counterpart of `blur`, which does not bubble and so cannot be
/// delegated at all. This is what "the field lost focus" is spelled as.
pub fn on_focusout(handler: impl FnOnce(Event)) -> Attr {
    on(EventType::FocusOut, handler)
}

/// A delegated `input` handler.
pub fn on_input(handler: impl FnOnce(Event)) -> Attr {
    on(EventType::Input, handler)
}

/// A delegated `keydown` handler.
pub fn on_keydown(handler: impl FnOnce(Event)) -> Attr {
    on(EventType::KeyDown, handler)
}

/// A delegated `submit` handler. The default submission is suppressed.
pub fn on_submit(handler: impl FnOnce(Event)) -> Attr {
    on(EventType::Submit, handler)
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

    /// The runtime delegates a closed set, so the name is a type and a typo
    /// is a compile error rather than a handler that sits there never firing.
    #[test]
    fn an_event_is_named_by_its_type() {
        let mut attributes = Attributes::new();
        on_dblclick(|event| event.stop_propagation()).write(&mut attributes);

        assert!(attributes.render().starts_with(" data-on-dblclick="));
        assert_eq!(EventType::FocusOut.name(), "focusout");
    }

    /// What `window.exos.listen` registers, which is the only way a name
    /// outside the delegated set ever reaches an element.
    #[test]
    fn a_registered_event_can_still_be_named() {
        let mut attributes = Attributes::new();
        on(EventType::Custom("swipe"), |_| emit("go()")).write(&mut attributes);

        assert_eq!(attributes.render(), " data-on-swipe=\"go()\"");
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
