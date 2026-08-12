//! What a file extension says about a file.

/// How a file is processed on its way into the binary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Pipeline {
    /// Bundled through lightningcss.
    Css,
    /// Bundled by following its imports, then minified.
    Javascript,
    /// Embedded byte for byte.
    Verbatim,
}

pub(crate) fn pipeline(extension: &str) -> Pipeline {
    match extension {
        "css" => Pipeline::Css,
        "js" | "mjs" => Pipeline::Javascript,
        _ => Pipeline::Verbatim,
    }
}

/// The `Content-Type` for an extension, if it is one the web has agreed on.
///
/// Deliberately short. It covers what a server-rendered application actually
/// ships, and anything else is spelled out at the call site rather than
/// guessed at here.
///
/// ```
/// assert_eq!(exos_build::content_type("woff2"), Some("font/woff2"));
/// assert_eq!(exos_build::content_type("xyz"), None);
/// ```
#[must_use]
pub fn content_type(extension: &str) -> Option<&'static str> {
    Some(match extension {
        "avif" => "image/avif",
        "css" => "text/css; charset=utf-8",
        "gif" => "image/gif",
        "html" => "text/html; charset=utf-8",
        "ico" => "image/x-icon",
        "jpeg" | "jpg" => "image/jpeg",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" | "map" => "application/json",
        "mp4" => "video/mp4",
        "otf" => "font/otf",
        "pdf" => "application/pdf",
        "png" => "image/png",
        "svg" => "image/svg+xml",
        "ttf" => "font/ttf",
        "txt" => "text/plain; charset=utf-8",
        "wasm" => "application/wasm",
        "webm" => "video/webm",
        "webp" => "image/webp",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "xml" => "application/xml",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_types_carry_a_charset_so_browsers_do_not_guess() {
        assert_eq!(content_type("css"), Some("text/css; charset=utf-8"));
        assert_eq!(content_type("js"), Some("text/javascript; charset=utf-8"));
    }

    #[test]
    fn binary_types_carry_none() {
        assert_eq!(content_type("woff2"), Some("font/woff2"));
        assert_eq!(content_type("png"), Some("image/png"));
    }

    #[test]
    fn an_unknown_extension_is_the_callers_business() {
        assert_eq!(content_type("xyz"), None);
    }

    #[test]
    fn only_stylesheets_and_scripts_are_processed() {
        assert_eq!(pipeline("css"), Pipeline::Css);
        assert_eq!(pipeline("mjs"), Pipeline::Javascript);
        assert_eq!(pipeline("woff2"), Pipeline::Verbatim);
    }
}
