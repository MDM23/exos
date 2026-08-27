//! The attribute helpers a template reaches for.

use crate::{
    Js, Signal,
    attributes::{Attributes, IntoAttributes, source},
    quote_js,
};

/// One `name="value"` pair produced by a helper.
#[derive(Clone, Debug, Eq, PartialEq)]
#[must_use = "an attribute does nothing until it is written onto an element"]
pub struct Attr(String, String);

impl Attr {
    /// A raw attribute, for the cases the helpers do not cover.
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self(name.into(), value.into())
    }
}

impl IntoAttributes for Attr {
    fn write(self, attributes: &mut Attributes) {
        attributes.set(self.0, self.1);
    }
}

/// One class, toggled by a client-side condition.
#[derive(Clone, Debug, Eq, PartialEq)]
#[must_use = "an attribute does nothing until it is written onto an element"]
pub struct Class(&'static str, Js<bool>);

impl IntoAttributes for Class {
    fn write(self, attributes: &mut Attributes) {
        attributes.merge_object("data-class", self.0, self.1.source());
    }
}

/// A link to one of this application's own URLs.
///
/// The `href`, and `aria-current="page"` where that URL is the page being
/// rendered. Marking it is what a nav bar has to do anyway, and doing it here
/// means the page being read is decided once rather than threaded through every
/// template that draws a link to it.
///
/// A route's own `link()` is the one to reach for, since it is built from the
/// handler's signature. This is for a path that is not a route's:
///
/// ```rust
/// # use exos::{Link, Markup, view};
/// # fn files() -> Markup {
/// view! { <a {Link::to(exos::url("/files"))}>"Files"</a> }
/// # }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
#[must_use = "an attribute does nothing until it is written onto an element"]
pub struct Link(String);

impl Link {
    /// A link to `url`, which is one of this application's URLs and therefore
    /// already carries the base: what [`url`](crate::url) hands back, or a
    /// route's own `url()`.
    pub fn to(url: impl Into<String>) -> Self {
        Self(url.into())
    }
}

impl IntoAttributes for Link {
    fn write(self, attributes: &mut Attributes) {
        let here = crate::base::is_here(&self.0);
        attributes.set("href", self.0);

        if here {
            attributes.set("aria-current", "page");
        }
    }
}

/// A binding: the signal's name, its type, and what may be wrong with it.
#[derive(Clone, Debug, Eq, PartialEq)]
#[must_use = "an attribute does nothing until it is written onto an element"]
pub struct Bind {
    name: String,
    kind: &'static str,
    /// The record a message about this field lives in.
    state: Option<&'static str>,
    /// The rows field this control is one row's copy of.
    group: Option<&'static str>,
    /// The fields whose rules this one arms, where it gates any.
    arms: &'static str,
    /// The field's own rules, for the control to answer as it is typed into.
    rules: Option<String>,
    /// The model that answers the rule this field cannot, where it has one.
    check: &'static str,
}

impl IntoAttributes for Bind {
    fn write(self, attributes: &mut Attributes) {
        attributes.set("data-bind", self.name);
        attributes.set("data-bind-kind", self.kind);

        // Only a model field has either. The control answers its own rules and
        // writes the verdict into the record, which is the same slot a refusal
        // writes, so a template reads one place whoever decided it.
        if let Some(state) = self.state {
            attributes.set("data-bind-state", state);
        }

        // A field of a row is keyed by where the row sits, which only the
        // browser can say, so the group travels and the key is built there.
        if let Some(group) = self.group {
            attributes.set("data-bind-rows", group);
        }

        // Editing this field changes which rules apply to those, so what was
        // said about them is retired along with what was said about this.
        if !self.arms.is_empty() {
            attributes.set("data-bind-arms", self.arms);
        }

        if let Some(rules) = self.rules {
            attributes.set("data-bind-rules", rules);
        }

        // The model the round trip is addressed to, and the whole of what says
        // there is one to make. The field's own name is `data-bind` already, so
        // the pair the route resolves through is on the control either way.
        if !self.check.is_empty() {
            attributes.set("data-bind-check", self.check);
        }
    }
}

/// Something a control can be bound to.
///
/// Two implementors and one difference between them: a `#[model]` field knows
/// its rules and the record they are written into, and a plain
/// [`signal`](crate::signal) has neither, because nothing off the page can say
/// anything about one.
pub trait Bindable {
    /// The attributes this binding needs.
    fn binding(&self) -> Bind;
}

impl<T: BindKind> Bindable for Signal<T> {
    fn binding(&self) -> Bind {
        Bind {
            name: self.name().to_owned(),
            kind: T::KIND,
            state: None,
            rules: None,
            group: None,
            arms: "",
            check: "",
        }
    }
}

impl<T: BindKind> Bindable for crate::Bound<T> {
    fn binding(&self) -> Bind {
        Bind {
            name: self.name().to_owned(),
            kind: T::KIND,
            state: Some(self.state()),
            rules: self.rules().map(|rules| rules.source().to_owned()),
            group: self.group(),
            arms: self.arms(),
            check: self.check(),
        }
    }
}

/// What a bound control's value should become before it is sent.
///
/// A DOM input always hands back a string, but the signal's Rust type decides
/// what the server accepts: pushing `"1"` into a `Vec<u32>` fails to
/// deserialize. That type is known here, so it travels with the binding rather
/// than being guessed at runtime.
pub trait BindKind {
    /// The tag the runtime coerces by.
    const KIND: &'static str;
}

macro_rules! bind_kind {
    ($kind:literal: $($type:ty),* $(,)?) => {
        $(
            impl BindKind for $type {
                const KIND: &'static str = $kind;
            }
        )*
    };
}

bind_kind!("bool": bool);
bind_kind!("number": f32, f64, i8, i16, i32, i64, isize, u8, u16, u32, u64, usize);
bind_kind!("string": String);

/// A collection binds like its element: a `Vec<u32>` of checkbox values still
/// needs each value parsed as a number.
impl<T: BindKind> BindKind for Vec<T> {
    const KIND: &'static str = T::KIND;
}

/// Text content, kept in sync with the expression.
pub fn text<T>(expression: Js<T>) -> Attr {
    Attr::new("data-text", source(expression))
}

/// Toggles the `hidden` attribute.
pub fn show(condition: impl crate::IntoJs<bool>) -> Attr {
    Attr::new("data-show", condition.into_js().into_source())
}

/// One class toggle. Repeat the block for more.
pub fn class(name: &'static str, condition: impl crate::IntoJs<bool>) -> Class {
    Class(name, condition.into_js())
}

/// One reactive attribute.
pub fn attr<T>(name: &'static str, value: Js<T>) -> Attr {
    Attr::new(
        "data-attr",
        format!("{{{}: {}}}", quote_js(name), value.into_source()),
    )
}

/// One reactive property: `value`, `checked`, `indeterminate` and the like.
pub fn prop<T>(name: &'static str, value: Js<T>) -> Attr {
    Attr::new(
        "data-prop",
        format!("{{{}: {}}}", quote_js(name), value.into_source()),
    )
}

/// Two-way binding for a form control.
///
/// On a checkbox whose signal is a `Vec`, checking collects the element's
/// `value` into the array, which is what lets selecting many rows work with no
/// per-row bookkeeping.
pub fn bind(value: &impl Bindable) -> Bind {
    value.binding()
}

/// Never morph this element: a media player, a third-party widget.
pub fn preserve() -> Attr {
    Attr::new("data-preserve", String::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signal;

    #[test]
    fn a_binding_carries_the_signals_rust_type() {
        // Without this the runtime pushes the string "1" into what the server
        // declared as Vec<u32>, and the request fails to deserialize.
        let picked = signal(Vec::<u32>::new());
        let query = signal(String::new());

        let mut attributes = Attributes::new();
        bind(&picked).write(&mut attributes);
        assert!(attributes.render().contains("data-bind-kind=\"number\""));

        let mut attributes = Attributes::new();
        bind(&query).write(&mut attributes);
        assert!(attributes.render().contains("data-bind-kind=\"string\""));
    }

    /// Outside a request there is no page to be on, so a link is an `href` and
    /// nothing else. Which one is marked is `base`'s to say and tested there.
    #[test]
    fn a_link_is_the_url_it_was_given() {
        let mut attributes = Attributes::new();
        Link::to("/files").write(&mut attributes);

        assert_eq!(attributes.render(), " href=\"/files\"");
    }

    #[test]
    fn show_renders_the_condition_verbatim() {
        let mut attributes = Attributes::new();
        show(Js::<bool>::raw("!$.gone")).write(&mut attributes);

        assert_eq!(attributes.render(), " data-show=\"!$.gone\"");
    }
}
