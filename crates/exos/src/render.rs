//! Turning Rust values into markup.
//!
//! The rule is one line long: everything is escaped except [`Markup`].
//! `Markup` is the only type that writes through unescaped, it is only
//! produced by the `view!` macro, and its field is public so that injecting
//! raw HTML is something you have to type out and can grep for.

use core::fmt::{self, Display, Write as _};

// -----------------------------------------------------------------------------
//                                    MARKUP
// -----------------------------------------------------------------------------

/// A rendered fragment of HTML.
///
/// This is what `view!` produces, and the only type that renders without
/// escaping.
///
/// ```
/// # use exos::{Markup, Render};
/// let raw = Markup(String::from("<b>bold</b>"));
/// assert_eq!(raw.render().as_str(), "<b>bold</b>");
/// ```
#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Markup(pub String);

impl Markup {
    /// The markup as a string slice.
    #[must_use = "reading the markup without using it does nothing"]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consumes the value and returns the underlying string.
    #[must_use = "reading the markup without using it does nothing"]
    pub fn into_string(self) -> String {
        self.0
    }

    /// Whether anything was rendered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Display for Markup {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl From<String> for Markup {
    fn from(markup: String) -> Self {
        Self(markup)
    }
}

// -----------------------------------------------------------------------------
//                                    TRAITS
// -----------------------------------------------------------------------------

/// A value that can be written into markup.
///
/// Implementations other than [`Markup`] escape, so interpolating a value into
/// a template can never produce tags the author did not write.
///
/// ```
/// # use exos::Render;
/// let mut out = String::new();
/// "<script>".render_to(&mut out);
/// assert_eq!(out, "&lt;script&gt;");
/// ```
pub trait Render {
    /// Appends this value's markup to `out`.
    fn render_to(&self, out: &mut String);

    /// Renders into a fresh [`Markup`].
    #[must_use]
    fn render(&self) -> Markup {
        let mut out = String::new();
        self.render_to(&mut out);
        Markup(out)
    }
}

/// How a value behaves in attribute position.
///
/// The distinction that matters is [`None`]. An absent attribute is not the
/// same as an empty one: `aria-current=""` is a value to a screen reader,
/// while no attribute at all is not.
pub trait AttributeValue {
    /// What gets rendered when the attribute is present.
    type Output<'value>: Render
    where
        Self: 'value;

    /// The value to render, or [`None`] to omit the attribute entirely.
    fn attribute_value(&self) -> Option<Self::Output<'_>>;
}

// -----------------------------------------------------------------------------
//                                IMPLEMENTATIONS
// -----------------------------------------------------------------------------

/// Escapes into an HTML text or double-quoted attribute context.
///
/// `&`, `<` and `>` cover text; `"` covers the attribute case. A single quote
/// is left alone because the `view!` macro only ever emits double-quoted
/// attributes.
pub fn escape_into(text: &str, out: &mut String) {
    out.reserve(text.len());

    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(character),
        }
    }
}

impl Render for Markup {
    /// The one unescaped implementation in the crate.
    fn render_to(&self, out: &mut String) {
        out.push_str(&self.0);
    }
}

impl Render for str {
    fn render_to(&self, out: &mut String) {
        escape_into(self, out);
    }
}

impl Render for String {
    fn render_to(&self, out: &mut String) {
        escape_into(self, out);
    }
}

impl<T: Render + ?Sized> Render for &T {
    fn render_to(&self, out: &mut String) {
        (**self).render_to(out);
    }
}

impl<T: Render> Render for Option<T> {
    fn render_to(&self, out: &mut String) {
        if let Some(value) = self {
            value.render_to(out);
        }
    }
}

impl<T: Render> Render for Vec<T> {
    fn render_to(&self, out: &mut String) {
        for value in self {
            value.render_to(out);
        }
    }
}

impl<T: Render> Render for [T] {
    fn render_to(&self, out: &mut String) {
        for value in self {
            value.render_to(out);
        }
    }
}

/// Numbers, booleans and characters carry no HTML-significant text, so they go
/// in through `Display` without a scan.
macro_rules! render_via_display {
    ($($type:ty),* $(,)?) => {
        $(
            impl Render for $type {
                fn render_to(&self, out: &mut String) {
                    // Writing into a String is infallible, so the result
                    // carries no information worth propagating.
                    let _ = write!(out, "{self}");
                }
            }
        )*
    };
}

render_via_display!(
    bool, char, f32, f64, i8, i16, i32, i64, isize, u8, u16, u32, u64, usize
);

impl<T: Render> AttributeValue for Option<T> {
    type Output<'value>
        = &'value T
    where
        Self: 'value;

    fn attribute_value(&self) -> Option<&T> {
        self.as_ref()
    }
}

impl<T: AttributeValue + ?Sized> AttributeValue for &T {
    type Output<'value>
        = T::Output<'value>
    where
        Self: 'value;

    fn attribute_value(&self) -> Option<T::Output<'_>> {
        (**self).attribute_value()
    }
}

/// Everything that is not an [`Option`] is simply always present.
macro_rules! always_present {
    ($($type:ty),* $(,)?) => {
        $(
            impl AttributeValue for $type {
                type Output<'value> = &'value $type where Self: 'value;

                fn attribute_value(&self) -> Option<&$type> {
                    Some(self)
                }
            }
        )*
    };
}

always_present!(
    Markup, String, bool, char, f32, f64, i8, i16, i32, i64, isize, str, u8, u16, u32, u64, usize
);

// -----------------------------------------------------------------------------
//                                     FLAG
// -----------------------------------------------------------------------------

/// An HTML boolean attribute: present when true, absent when false.
///
/// A bare [`bool`] renders the text `"true"` or `"false"`, which is what a
/// `data-*` attribute wants so that CSS can match both states. `disabled`
/// wants the other thing, and confusing the two is a real bug: the selector
/// `[data-online]` matches `data-online="false"` too.
///
/// ```
/// # use exos::{AttributeValue, Flag};
/// assert!(Flag(true).attribute_value().is_some());
/// assert!(Flag(false).attribute_value().is_none());
/// ```
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Flag(pub bool);

impl Render for Flag {
    /// A present flag renders as a bare attribute name, so it has no value.
    fn render_to(&self, _out: &mut String) {}
}

impl AttributeValue for Flag {
    type Output<'value>
        = &'value Self
    where
        Self: 'value;

    fn attribute_value(&self) -> Option<&Self> {
        self.0.then_some(self)
    }
}

// -----------------------------------------------------------------------------
//                                     TESTS
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_escaped_but_markup_is_not() {
        let mut out = String::new();
        "<script>".render_to(&mut out);
        assert_eq!(out, "&lt;script&gt;");

        let mut out = String::new();
        Markup(String::from("<b>bold</b>")).render_to(&mut out);
        assert_eq!(out, "<b>bold</b>");
    }

    #[test]
    fn quotes_are_escaped_so_attributes_cannot_be_broken_out_of() {
        let mut out = String::new();
        r#"" onclick="evil()"#.render_to(&mut out);
        assert_eq!(out, "&quot; onclick=&quot;evil()");
    }

    #[test]
    fn absent_attributes_differ_from_empty_ones() {
        let present: Option<&str> = Some("page");
        let absent: Option<&str> = None;

        assert!(present.attribute_value().is_some());
        assert!(absent.attribute_value().is_none());
    }

    #[test]
    fn a_flag_is_present_or_absent_never_false() {
        assert!(Flag(true).attribute_value().is_some());
        assert!(Flag(false).attribute_value().is_none());
    }

    #[test]
    fn collections_render_each_item_in_order() {
        let items = vec!["a", "b", "c"];
        assert_eq!(items.render().as_str(), "abc");
    }
}
