//! Escaping performed at expansion time.
//!
//! Static text is escaped once, by the macro, so the binary holds the finished
//! bytes and the server does no work per request. Runtime values take the
//! other path, through [`exos::Render`].

/// Escapes into an HTML text context.
pub(super) fn escape_text(text: &str) -> String {
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
pub(super) fn escape_attribute(text: &str) -> String {
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

/// Collapses runs of whitespace to a single space, the way HTML itself does,
/// so template indentation does not reach the wire.
pub(super) fn collapse_whitespace(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut pending = false;

    for character in text.chars() {
        if character.is_whitespace() {
            pending = true;
            continue;
        }

        if pending && !result.is_empty() {
            result.push(' ');
        }

        pending = false;
        result.push(character);
    }

    if pending && !result.is_empty() {
        result.push(' ');
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

    #[test]
    fn indentation_collapses_but_single_spaces_survive() {
        assert_eq!(collapse_whitespace("\n    one   two\n"), "one two ");
        assert_eq!(collapse_whitespace("   "), "");
    }
}
