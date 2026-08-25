//! Where this application's URLs start.
//!
//! exos invents two kinds of URL: the endpoints the client runtime talks to,
//! and the URL of every [`asset!`](crate::asset). Routing them needs no help,
//! because `Router::nest` already puts them wherever it is told. What does need
//! help is the string written into a page, since a URL in HTML is either
//! absolute, and needs the prefix, or relative, and needs to know which page it
//! is being rendered for.
//!
//! Relative is the tempting one and does not survive contact with this design.
//! A fragment renders [detached](crate::detached) and
//! [`publish`](crate::publish) renders with no request at all, so "which page
//! is this for" is a question exos deliberately cannot answer, while "where is
//! the application mounted" is one fact for the life of the process.
//!
//! # Nothing to configure
//!
//! So exos works the prefix out. `Router::nest` rewrites the path it forwards
//! and axum records what arrived, and the difference between the two is the
//! mount point:
//!
//! ```text
//! original  /admin/_exos/live
//! current         /_exos/live
//! prefix    /admin
//! ```
//!
//! It is read on the first request and kept, because the answer belongs to the
//! process rather than to the request: `publish` renders fragments where no
//! request exists, and they carry asset URLs like any other markup.
//!
//! # When it has to be told
//!
//! It is the outermost axum `Router` that records the arriving URI, so anything
//! that rewrites the path in front of one is invisible from here. A reverse
//! proxy stripping `/admin` is the ordinary case: the server genuinely never
//! sees that prefix, and no amount of looking will find it.
//!
//! ```rust
//! exos::base("/admin");
//! ```
//!
//! Said explicitly it wins, and discovery never runs.
//!
//! # The client is not told either
//!
//! It works it out too, and from a different direction. The runtime is itself
//! an asset served under this prefix, so `document.currentScript.src` carries
//! the answer and everything before `/_exos/` in it is the base. That is
//! self-verifying: if the script is running at all, the URL it came from was
//! right.

use std::sync::OnceLock;

use axum::extract::{OriginalUri, Request};

static BASE: OnceLock<String> = OnceLock::new();

/// Says where this application is mounted, rather than letting exos find out.
///
/// Only needed where the path is rewritten by something that is not an axum
/// `Router`, since that is the one case exos cannot work out for itself: it is
/// the outermost `Router` that records the arriving URI. A reverse proxy
/// serving this application at `/admin` while forwarding `/` to it is the
/// ordinary example.
///
/// ```rust
/// exos::base("/admin");
/// ```
///
/// Nesting needs no call: `Router::nest("/admin", exos::app())` is found on its
/// own.
///
/// A trailing slash is dropped, so `/admin` and `/admin/` mean the same thing.
/// `/` means the root and is the same as saying nothing.
///
/// # Panics
///
/// If `path` is not empty and does not start with `/`, because a relative base
/// would resolve against whichever page happened to be open.
///
/// If a base is already in place. That means either two parts of the program
/// disagree about where the application lives, or this arrived after the first
/// request had already answered the question, and both are worth being loud
/// about rather than resolving by whichever ran first.
pub fn base(path: impl Into<String>) {
    assert!(
        BASE.set(normalize(&path.into())).is_ok(),
        "a base is already in place; exos::base goes once, before anything is \
         served, and is only needed where the path is rewritten by something \
         that is not an axum Router"
    );
}

/// Where this application's URLs start, as a browser sees them.
///
/// Empty at the root, and never with a trailing slash. exos puts this in front
/// of everything it writes: an [`asset!`](crate::asset) URL, its own endpoints,
/// and the URL a generated route caller posts to.
///
/// Mostly it is [`url`] you want, which is this with the joining already
/// decided. Reach for the prefix itself only where there is no path to join it
/// to: a canonical link, a cookie's `Path`, something handed to another system.
///
/// Empty until the first request has been seen, which is why it is read while
/// rendering rather than kept anywhere: a page renders inside a request, so the
/// answer is already in by the time one asks.
pub fn base_path() -> &'static str {
    BASE.get().map_or("", String::as_str)
}

/// The same answer, for the places inside this crate that build a URL.
pub(crate) fn path() -> &'static str {
    base_path()
}

/// One of this application's own paths, as a browser has to ask for it.
///
/// The short way to write [`base_path`] into a URL, and the only one worth
/// using, since it is also the one place the joining rule lives:
///
/// ```rust
/// # use exos::{Markup, view};
/// # fn link() -> Markup {
/// view! { <a href={ exos::url("/files") }>"Files"</a> }
/// # }
/// ```
///
/// Where the path is a route's, prefer the `url` the route attribute generates:
/// `files::url(3)` is checked against the handler's own signature, so renaming
/// the route or changing its parameter breaks the link at compile time. This is
/// for everything else, and for a path that is not a route at all.
///
/// # What it does not do
///
/// It cannot tell that a path already carries the base, because `/admin/files`
/// is a perfectly ordinary route to have. Pass what a route is declared with,
/// which is the path the server sees.
///
/// It does take any number of leading slashes down to one, so a path that
/// arrived from outside cannot turn into `//example.com` and send somebody to
/// another host.
pub fn url(path: impl AsRef<str>) -> String {
    format!("{}/{}", base_path(), path.as_ref().trim_start_matches('/'))
}

/// Learns the mount point from a request, once.
///
/// The first request to answer the question wins, and a later one cannot change
/// it: the prefix belongs to the process, and a `publish` rendering a fragment
/// from a background job has to read the same answer a handler does.
pub(crate) fn observe(request: &Request) {
    if BASE.get().is_some() {
        return;
    }

    let Some(OriginalUri(original)) = request.extensions().get::<OriginalUri>() else {
        // Only when axum's `original-uri` feature is off, which this crate
        // turns on. Nothing to do but leave the answer for a later request.
        return;
    };

    if let Some(prefix) = prefix_of(original.path(), request.uri().path()) {
        // Ignored on a race, because both racers computed the same answer from
        // the same rewriting.
        drop(BASE.set(prefix));
    }
}

/// What was taken off the front of `current` to leave it, if that is legible.
///
/// `None` where the two do not line up, which means something rewrote the path
/// in a way this cannot read. Leaving the question open for the next request
/// beats guessing, since the answer is kept forever.
fn prefix_of(original: &str, current: &str) -> Option<String> {
    // A router nested at `/admin` forwards a request for `/admin` as `/`, so
    // the whole of the original is the prefix. Suffix stripping alone would
    // miss it, because `/admin` does not end with `/`.
    if current == "/" {
        return Some(normalize(original));
    }

    original.strip_suffix(current).map(ToOwned::to_owned)
}

/// The base as it is stored: no trailing slash, and empty for the root.
fn normalize(path: &str) -> String {
    let path = path.strip_suffix('/').unwrap_or(path);

    assert!(
        path.is_empty() || path.starts_with('/'),
        "a base starts with `/`, since it is where this application's URLs \
         start and a relative one would resolve against whatever page is open"
    );

    path.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    // The store is process-global and can be written once, so what discovery
    // and an explicit base do to a router and to an asset URL are checked in
    // `tests/mounted.rs` and `tests/base.rs`, where a test binary is a process
    // of its own. What is left here is the arithmetic, which is pure.

    #[test]
    fn an_application_at_the_root_has_no_prefix() {
        assert_eq!(prefix_of("/_exos/live", "/_exos/live").as_deref(), Some(""));
        assert_eq!(prefix_of("/", "/").as_deref(), Some(""));
    }

    #[test]
    fn a_nested_application_is_what_was_taken_off_the_front() {
        assert_eq!(
            prefix_of("/admin/_exos/live", "/_exos/live").as_deref(),
            Some("/admin")
        );
        assert_eq!(
            prefix_of("/a/b/_exos/live", "/_exos/live").as_deref(),
            Some("/a/b")
        );
    }

    /// The edge suffix stripping alone gets wrong. A router nested at `/admin`
    /// forwards a request for `/admin` as `/`, and `/admin` does not end with
    /// `/`, so this has to be its own case rather than a near miss.
    #[test]
    fn a_request_for_the_mount_point_itself_is_read_correctly() {
        assert_eq!(prefix_of("/admin", "/").as_deref(), Some("/admin"));
        assert_eq!(prefix_of("/admin/", "/").as_deref(), Some("/admin"));
    }

    /// Something rewrote the path in a way this cannot read, so it says so
    /// rather than guessing at an answer that is then kept for good.
    #[test]
    fn a_path_that_does_not_line_up_answers_nothing() {
        assert!(prefix_of("/admin/files", "/other").is_none());
        assert!(prefix_of("/short", "/a/much/longer/path").is_none());
    }

    // `url` reads the store, which these tests leave at the root, so what it
    // does under a base is checked in `tests/mounted.rs` and `tests/base.rs`.
    // What is worth pinning here is the joining, which is the same either way.

    #[test]
    fn a_path_is_joined_with_exactly_one_slash() {
        assert_eq!(url("/files"), "/files");
        assert_eq!(
            url("files"),
            "/files",
            "a missing slash is not a missing one"
        );
        assert_eq!(url("/files/3?page=2"), "/files/3?page=2");
    }

    /// A path that arrived from outside must not be able to become a URL
    /// pointing at another host, which is what a second leading slash would
    /// make it.
    #[test]
    fn a_protocol_relative_path_cannot_escape_the_application() {
        assert_eq!(url("//example.com/files"), "/example.com/files");
        assert_eq!(url("///example.com"), "/example.com");
    }

    #[test]
    fn the_root_path_is_a_single_slash() {
        assert_eq!(url(""), "/");
        assert_eq!(url("/"), "/");
    }

    #[test]
    fn a_base_keeps_its_leading_slash_and_loses_its_trailing_one() {
        assert_eq!(normalize("/admin"), "/admin");
        assert_eq!(normalize("/admin/"), "/admin");
        assert_eq!(normalize("/a/b"), "/a/b");
    }

    #[test]
    fn the_root_is_the_empty_string_however_it_is_spelled() {
        assert_eq!(normalize(""), "");
        assert_eq!(normalize("/"), "");
    }

    #[test]
    #[should_panic(expected = "a base starts with `/`")]
    fn a_relative_base_is_refused() {
        drop(normalize("admin"));
    }
}
