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

/// Where assets are mounted. Hashed names make the prefix arbitrary.
pub(crate) const PREFIX: &str = "/_exos";

/// One asset produced by [`exos_build`](https://docs.rs/exos-build).
///
/// Values are built by the generated table rather than by hand, which is why
/// the fields are public.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct Asset {
    /// The logical name, such as `app.css`.
    pub name: &'static str,
    /// The hashed file name, such as `app-9f2c1b4e.css`.
    pub file: &'static str,
    /// What to serve it as.
    pub content_type: &'static str,
    /// The bytes, embedded in the binary.
    pub bytes: &'static [u8],
}

impl Asset {
    /// The URL this asset is served from.
    #[must_use]
    pub fn url(&self) -> String {
        format!("{PREFIX}/{}", self.file)
    }
}

/// A crate's assets, as registered by [`assets!`](crate::assets).
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
}
