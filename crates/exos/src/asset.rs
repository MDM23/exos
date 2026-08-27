//! Assets, served out of the binary.
//!
//! Every asset is content-hashed at build time, so its URL changes exactly
//! when its bytes do. That makes the caching policy unconditional: immutable,
//! for a year. There is no revalidation to negotiate and no way to serve a
//! stale file, because a changed file is a different URL.

use axum::{
    Router,
    body::Body,
    extract::Path,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};

use crate::{AttributeValue, Render};

/// Where assets sit under whatever [`base`](crate::App::base) the application
/// has.
///
/// Hashed names make the segment itself arbitrary. What is not arbitrary is
/// that the runtime finds the base by looking for this in its own script URL,
/// so the endpoints in [`live`](crate::live) share it deliberately rather than
/// by coincidence.
pub(crate) const PREFIX: &str = "/_exos";

/// The URL of the client runtime, which every page has to load.
///
/// ```ignore
/// view! { <script defer src={ exos::runtime() }></script> }
/// ```
///
/// It ships inside this crate, so there is nothing to copy into a project and
/// no version to keep in step.
///
/// # The query a dev build adds
///
/// `?dev`, which is how the runtime finds out it is one. It cannot read a
/// `cfg!`, and it is the same file in both builds, so the answer has to arrive
/// from the server. The URL of its own script is the channel already there: the
/// runtime reads it to work out the [base](crate::App::base), and it cannot be
/// looking at anybody else's. Nothing routes on a query, so where the file is
/// served from does not change.
///
/// What the runtime does with it is keep its stream open on a page with nothing
/// live on it, and answer a reconnect with a reload rather than a repair, so
/// that a rebuilt server reaches the tab looking at it.
///
/// A [`String`] rather than an [`Asset`], because a query is not part of any
/// file name.
pub fn runtime() -> String {
    let url = crate::asset!("js/exos.js").url();

    if cfg!(debug_assertions) {
        format!("{url}?dev")
    } else {
        url
    }
}

/// What [`asset!`](crate::asset) evaluates to: a file the binary carries.
///
/// It holds the hashed file name and nothing else, so it is a `&'static str` in
/// a newtype and every call site is a constant. Interpolating it into a
/// [`view!`](crate::view) writes the URL it is served from:
///
/// ```ignore
/// view! { <link rel="stylesheet" href={ exos::asset!("css/app.css") }> }
/// ```
///
/// [`bytes`](Self::bytes) is the other half. The file is in the binary
/// already, so whatever can be derived from its content can be derived at
/// startup rather than kept beside it by hand:
///
/// ```ignore
/// static BLURRED: LazyLock<String> = LazyLock::new(|| {
///     placeholder(exos::asset!("img/cover.png").bytes())
/// });
/// ```
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Asset(&'static str);

impl Asset {
    /// Names one built asset. Called by [`asset!`](crate::asset).
    pub const fn new(file: &'static str) -> Self {
        Self(file)
    }

    /// The hashed file name, such as `app-9f2c1b4e.css`.
    pub const fn file(&self) -> &'static str {
        self.0
    }

    /// The URL this asset is served from, under the application's base.
    ///
    /// Only worth calling where a URL has to be a [`String`]: in a view the
    /// asset renders as one.
    pub fn url(&self) -> String {
        self.render().into_string()
    }

    /// The bytes that went into the binary under this name.
    ///
    /// The lookup walks what the binary embedded, so hold the result rather
    /// than calling this per request. A [`LazyLock`](std::sync::LazyLock) is
    /// the shape for it: the bytes never change, and neither does anything
    /// computed from them.
    ///
    /// # Panics
    ///
    /// If nothing embedded this file. Every asset is claimed by exactly one
    /// call site per crate, and that site is compiled into the same binary as
    /// this one, so a failure here means the claiming site was never generated
    /// code: an `asset!` inside a generic function nothing instantiates. That
    /// asset has no URL that serves either, which is a build-shaped fault
    /// rather than a request-shaped one, and it should say so at startup.
    pub fn bytes(&self) -> &'static [u8] {
        crate::discover::asset_sets()
            .iter()
            .flat_map(|set| set.0)
            .find(|embedded| embedded.file == self.0)
            .unwrap_or_else(|| panic!("exos: nothing in this binary embedded {}", self.0))
            .bytes
    }
}

impl Render for Asset {
    /// Straight into the buffer. A hashed file name and a base hold nothing
    /// that HTML escaping would touch, and the [`String`] the URL would be
    /// built in first is a temporary that only ever gets copied here.
    fn render_to(&self, out: &mut String) {
        out.push_str(crate::base::path());
        out.push_str(PREFIX);
        out.push('/');
        out.push_str(self.0);
    }
}

impl AttributeValue for Asset {
    type Output<'value>
        = &'value Self
    where
        Self: 'value;

    fn attribute_value(&self) -> Option<&Self> {
        Some(self)
    }
}

/// One asset built by [`asset!`](crate::asset), with the bytes it embedded.
///
/// Values come from that macro rather than being written by hand, which is
/// why the constructor takes everything at once.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Embedded {
    name: &'static str,
    file: &'static str,
    content_type: &'static str,
    bytes: &'static [u8],
}

impl Embedded {
    /// Describes one built asset. Called by [`asset!`](crate::asset).
    pub const fn new(
        name: &'static str,
        file: &'static str,
        content_type: &'static str,
        bytes: &'static [u8],
    ) -> Self {
        Self {
            name,
            file,
            content_type,
            bytes,
        }
    }

    /// The logical name, such as `app.css`.
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// A handle to this asset, which is what a page needs.
    pub const fn asset(&self) -> Asset {
        Asset(self.file)
    }

    /// The bytes, as they are served.
    pub const fn bytes(&self) -> &'static [u8] {
        self.bytes
    }
}

/// The assets one [`asset!`](crate::asset) call site registered.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct AssetSet(pub &'static [Embedded]);

impl AssetSet {
    /// The asset with this logical name, if the set has one.
    pub fn get(&self, name: &str) -> Option<&'static Embedded> {
        self.0.iter().find(|embedded| embedded.name == name)
    }

    /// The asset served under this hashed file name.
    pub fn by_file(&self, file: &str) -> Option<&'static Embedded> {
        self.0.iter().find(|embedded| embedded.file == file)
    }
}

/// Serves the given sets under `/_exos`.
///
/// Mounted at the root of whatever router this ends up in, because nesting is
/// what puts an application under a prefix and doing it here too would put it
/// under one twice. What the [base](crate::App::base) changes is the URL an
/// [`Asset`] renders as, not where this answers.
///
/// Takes the sets by value: discovery assembles them at startup and each one
/// only points at `'static` data, so this is a handful of fat pointers.
///
/// Reach for this only when composing the router by hand.
pub fn routes(sets: Vec<AssetSet>) -> Router {
    Router::new().route(
        &format!("{PREFIX}/{{file}}"),
        get(move |Path(file): Path<String>| async move {
            sets.iter()
                .find_map(|set| set.by_file(&file))
                .map_or_else(|| StatusCode::NOT_FOUND.into_response(), serve)
        }),
    )
}

fn serve(embedded: &'static Embedded) -> Response {
    let mut response = Response::new(Body::from(embedded.bytes));
    let headers = response.headers_mut();

    // `from_static` cannot fail here: content types come from the build
    // pipeline, which only produces valid header values.
    if let Ok(value) = HeaderValue::from_str(embedded.content_type) {
        headers.insert(header::CONTENT_TYPE, value);
    }

    // Safe unconditionally, because the hash is in the file name.
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=31536000, immutable"),
    );

    response
}

#[cfg(test)]
mod tests {
    use super::*;

    const STYLESHEET: Embedded = Embedded {
        name: "app.css",
        file: "app-0123456789ab.css",
        content_type: "text/css; charset=utf-8",
        bytes: b"body{}",
    };

    const SET: AssetSet = AssetSet(&[STYLESHEET]);

    #[test]
    fn looks_up_by_logical_name_and_by_hashed_file() {
        assert_eq!(SET.get("app.css"), Some(&STYLESHEET));
        assert_eq!(SET.by_file("app-0123456789ab.css"), Some(&STYLESHEET));
        assert_eq!(SET.get("missing.css"), None);
    }

    #[test]
    fn the_url_carries_the_hash_so_it_can_be_cached_forever() {
        assert_eq!(STYLESHEET.asset().url(), "/_exos/app-0123456789ab.css");
    }

    /// In a view it is the URL and nothing else, so a page written against a
    /// `String` reads the same after the handle replaced one.
    #[test]
    fn an_asset_renders_as_its_url() {
        assert_eq!(
            STYLESHEET.asset().render().as_str(),
            "/_exos/app-0123456789ab.css"
        );
    }

    /// The macro and the router build their URLs from one constant, so neither
    /// can drift into a 404 the other serves. What that looks like under a base
    /// is checked in [`tests/base.rs`](../../tests/base.rs), which needs a
    /// process of its own to set one.
    #[test]
    fn the_router_serves_what_the_macro_points_at() {
        assert!(
            runtime().starts_with(&format!("{PREFIX}/")),
            "the macro returned {}, which this crate does not route",
            runtime()
        );
    }

    /// The one call that can panic, on the file this crate embeds itself.
    #[test]
    fn the_bytes_of_an_embedded_file_are_reachable() {
        let runtime = crate::asset!("js/exos.js");

        assert!(runtime.bytes().starts_with(b"//"));
    }
}
