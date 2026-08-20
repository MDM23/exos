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

/// Where assets sit under whatever [`base`](crate::base) the application has.
///
/// Hashed names make the segment itself arbitrary. What is not arbitrary is
/// that the runtime finds the base by looking for this in its own script URL,
/// so the endpoints in [`live`](crate::live) share it deliberately rather than
/// by coincidence.
pub(crate) const PREFIX: &str = "/_exos";

/// The URL an asset is served from, under the application's base.
///
/// Called by [`asset!`](crate::asset), which knows the hashed file name at
/// compile time but cannot know the base, since that is chosen when the program
/// runs. There is no reason to call it by hand.
#[doc(hidden)]
#[must_use]
pub fn asset_url(file: &str) -> String {
    format!("{}{PREFIX}/{file}", crate::base::path())
}

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
/// runtime reads it to work out the [base](crate::base), and it cannot be
/// looking at anybody else's. Nothing routes on a query, so where the file is
/// served from does not change.
///
/// What the runtime does with it is keep its stream open on a page with nothing
/// live on it, and answer a reconnect with a reload rather than a repair, so
/// that a rebuilt server reaches the tab looking at it.
#[must_use]
pub fn runtime() -> String {
    let url = crate::asset!("js/exos.js");

    if cfg!(debug_assertions) {
        format!("{url}?dev")
    } else {
        url
    }
}

/// One asset built by [`asset!`](crate::asset).
///
/// Values come from that macro rather than being written by hand, which is
/// why the constructor takes everything at once.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Asset {
    name: &'static str,
    file: &'static str,
    content_type: &'static str,
    bytes: &'static [u8],
}

impl Asset {
    /// Describes one built asset. Called by [`asset!`](crate::asset).
    #[must_use]
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
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// The hashed file name, such as `app-9f2c1b4e.css`.
    #[must_use]
    pub const fn file(&self) -> &'static str {
        self.file
    }

    /// The URL this asset is served from.
    #[must_use]
    pub fn url(&self) -> String {
        asset_url(self.file)
    }
}

/// The assets one [`asset!`](crate::asset) call site registered.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct AssetSet(pub &'static [Asset]);

impl AssetSet {
    /// The asset with this logical name, if the set has one.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&'static Asset> {
        self.0.iter().find(|asset| asset.name == name)
    }

    /// The asset served under this hashed file name.
    #[must_use]
    pub fn by_file(&self, file: &str) -> Option<&'static Asset> {
        self.0.iter().find(|asset| asset.file == file)
    }
}

/// Serves the given sets under `/_exos`.
///
/// Mounted at the root of whatever router this ends up in, because nesting is
/// what puts an application under a prefix and doing it here too would put it
/// under one twice. What the [base](crate::base) changes is the URL
/// [`asset_url`] writes into a page, not where this answers.
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

fn serve(asset: &'static Asset) -> Response {
    let mut response = Response::new(Body::from(asset.bytes));
    let headers = response.headers_mut();

    // `from_static` cannot fail here: content types come from the build
    // pipeline, which only produces valid header values.
    if let Ok(value) = HeaderValue::from_str(asset.content_type) {
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

    const STYLESHEET: Asset = Asset {
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
        assert_eq!(STYLESHEET.url(), "/_exos/app-0123456789ab.css");
    }

    /// The macro and the router build their URLs from one function and one
    /// constant, so neither can drift into a 404 the other serves. What that
    /// looks like under a base is checked in
    /// [`tests/base.rs`](../../tests/base.rs), which needs a process of its own
    /// to set one.
    #[test]
    fn the_router_serves_what_the_macro_points_at() {
        assert!(
            runtime().starts_with(&format!("{PREFIX}/")),
            "the macro returned {}, which this crate does not route",
            runtime()
        );
    }
}
