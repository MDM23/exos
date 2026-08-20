//! Escaping performed at expansion time.
//!
//! Static text is escaped once, by the macro, so the binary holds the finished
//! bytes and the server does no work per request. Runtime values take the
//! other path, through [`exos::Render`].
//!
//! `view!` is not the only macro with text to escape: a message with a slot in
//! it renders as markup, so its own words are escaped here too.

/// Escapes into an HTML text context.
pub(crate) fn escape_text(text: &str) -> String {
    let mut result = String::with_capacity(text.len());

    for character in text.chars() {
        match character {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            _ => result.push(character),
        }
    }

    result
}

/// Escapes into a double-quoted attribute context.
///
/// The four characters `exos::Render` escapes, which is also what a message's
/// own text takes, so that a sentence renders the same bytes whether or not it
/// has a slot in it.
pub(crate) fn escape_attribute(text: &str) -> String {
    let mut result = String::with_capacity(text.len());

    for character in text.chars() {
        match character {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '"' => result.push_str("&quot;"),
            _ => result.push(character),
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_escaping_leaves_quotes_alone() {
        assert_eq!(escape_text(r#"a < b & "c""#), r#"a &lt; b &amp; "c""#);
    }

    #[test]
    fn attribute_escaping_closes_the_quote_hole() {
        assert_eq!(escape_attribute(r#"" onclick=""#), "&quot; onclick=&quot;");
    }
}
