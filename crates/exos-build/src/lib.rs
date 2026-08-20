//! The asset pipeline behind [`exos::asset!`](https://docs.rs/exos).
//!
//! This is not a build script helper. The macro calls it while the crate is
//! being compiled, once per asset, and embeds what comes back. Bundling a
//! stylesheet resolves its `@import`s and builds whatever its `url()`s name,
//! bundling a script resolves its `import`s, everything else is embedded
//! verbatim, and all of it is content-hashed so a URL changes exactly when its
//! bytes do.
//!
//! ```no_run
//! # fn main() -> Result<(), exos_build::Error> {
//! use std::path::Path;
//!
//! let built = exos_build::build(Path::new("css/app.css"), None, exos_build::Mode::Release)?;
//! assert_eq!(built.name, "app.css");
//! # Ok(())
//! # }
//! ```
//!
//! [`Built::sources`] lists every file that was read, which is what lets the
//! macro declare its rebuild dependencies precisely instead of watching a
//! directory and hoping.

use std::{
    fs,
    path::{Path, PathBuf},
};

mod css;
mod javascript;
mod media;

pub use crate::media::content_type;

// Where an asset is served from is deliberately not here. This crate turns a
// file into bytes and a hashed name, and a URL is neither: it starts with the
// base the application picks when it runs, which nothing at build time can
// know. `exos::asset_url` is the one place that builds one.

/// Anything that can go wrong while building an asset.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// A source file could not be read.
    #[error("cannot read {path}")]
    Io {
        /// The file involved.
        path: PathBuf,
        /// The underlying failure.
        #[source]
        source: std::io::Error,
    },

    /// A stylesheet could not be parsed, bundled or printed.
    #[error("cannot bundle stylesheet {path}: {message}")]
    Css {
        /// The entry point that failed.
        path: PathBuf,
        /// What lightningcss reported.
        message: String,
    },

    /// A `url()` in a stylesheet names a file that could not be built.
    #[error("cannot embed {url} referenced by {path}")]
    Reference {
        /// The stylesheet the URL was written in.
        path: PathBuf,
        /// The URL as it was written.
        url: String,
        /// What was wrong with the file it names.
        #[source]
        source: Box<Self>,
    },

    /// A relative `url()` inside a custom property, which no rewriting can fix.
    #[error(
        "cannot embed {url} in custom property {property} of {path}: a browser \
         resolves a url() inside a custom property against the page the var() \
         is used on rather than against the stylesheet, so no URL written here \
         is right on every route. Put the url() in the rule that uses the \
         variable, or write an absolute URL and serve the file yourself"
    )]
    CustomProperty {
        /// The stylesheet the declaration is in.
        path: PathBuf,
        /// The property, such as `--splash`.
        property: String,
        /// The URL as it was written.
        url: String,
    },

    /// A script could not be bundled or minified.
    #[error("cannot bundle script {path}: {message}")]
    Javascript {
        /// The file that failed.
        path: PathBuf,
        /// What went wrong, and where.
        message: String,
    },

    /// The extension does not say what the file is.
    #[error("cannot infer a content type for {path}; pass one as the second argument")]
    UnknownType {
        /// The file whose extension went unrecognised.
        path: PathBuf,
    },
}

/// The result type used throughout this crate.
pub type Result<T, E = Error> = core::result::Result<T, E>;

/// Whether to optimise the output or keep it readable.
///
/// The macro derives this from the profile the crate is being compiled under,
/// so it matches `cfg!(debug_assertions)`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Mode {
    /// Leave the output readable. What development wants.
    Debug,
    /// Minify. What deployment wants.
    Release,
}

impl Mode {
    const fn minify(self) -> bool {
        matches!(self, Self::Release)
    }
}

/// One processed asset, ready to be embedded.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Built {
    /// The logical name, taken from the entry point, such as `app.css`.
    pub name: String,
    /// The hashed file name it is served under, such as `app-9f2c1b4e.css`.
    pub file: String,
    /// What to send as `Content-Type`.
    pub content_type: String,
    /// The processed bytes.
    pub bytes: Vec<u8>,
    /// Every file that was read, entry point included.
    ///
    /// Changing any of them changes this asset, so the caller has to treat all
    /// of them as inputs.
    pub sources: Vec<PathBuf>,
    /// The assets this one refers to, built the same way and already flat.
    ///
    /// A stylesheet's `url()`s, and nothing else so far. Their URLs are written
    /// into this asset, so they have to be served alongside it or it points at
    /// nothing.
    pub referenced: Vec<Self>,
}

/// Processes one asset, choosing the pipeline from the file extension.
///
/// `.css` is bundled, `.js` and `.mjs` are bundled, and anything else is
/// embedded byte for byte. Pass `content_type` to override what the extension
/// implies, which is also how an unrecognised extension is handled.
///
/// A stylesheet's `url()`s are built too and come back in
/// [`referenced`](Built::referenced), with the URLs in the CSS rewritten to
/// point at them. They are part of the asset and have to be served with it.
///
/// # Errors
///
/// Returns [`Error::Io`] when a file cannot be read, [`Error::Css`] or
/// [`Error::Javascript`] when one fails to parse, [`Error::Reference`] when a
/// `url()` names a file that cannot be built, and [`Error::UnknownType`] when
/// the extension is unknown and no content type was given.
pub fn build(path: &Path, content_type: Option<&str>, mode: Mode) -> Result<Built> {
    let extension = path
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();

    let mut referenced = Vec::new();

    let (bytes, sources) = match media::pipeline(&extension) {
        media::Pipeline::Css => {
            let bundled = css::bundle(path, mode)?;
            referenced = bundled.assets;
            (bundled.code.into_bytes(), bundled.sources)
        }
        media::Pipeline::Javascript => {
            let bundled = javascript::bundle(path, mode.minify())?;
            (bundled.code.into_bytes(), bundled.sources)
        }
        media::Pipeline::Verbatim => (read(path)?, vec![path.to_path_buf()]),
    };

    let content_type = match content_type {
        Some(given) => given.to_owned(),
        None => media::content_type(&extension)
            .ok_or_else(|| Error::UnknownType {
                path: path.to_path_buf(),
            })?
            .to_owned(),
    };

    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();

    Ok(Built {
        file: hashed(&name, &bytes),
        name,
        content_type,
        bytes,
        sources,
        referenced,
    })
}

/// `app.css` and its bytes become `app-9f2c1b4e12ab.css`.
fn hashed(name: &str, bytes: &[u8]) -> String {
    let digest = <sha2::Sha256 as sha2::Digest>::digest(bytes);
    let short: String = digest
        .iter()
        .take(6)
        .map(|byte| format!("{byte:02x}"))
        .collect();

    match name.rsplit_once('.') {
        Some((stem, extension)) => format!("{stem}-{short}.{extension}"),
        None => format!("{name}-{short}"),
    }
}

fn read(path: &Path) -> Result<Vec<u8>> {
    fs::read(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hash_goes_before_the_extension_so_the_type_survives() {
        assert!(hashed("app.css", b"body{}").starts_with("app-"));
        assert!(hashed("app.css", b"body{}").ends_with(".css"));
    }

    #[test]
    fn different_bytes_are_a_different_file() {
        assert_ne!(hashed("app.css", b"body{}"), hashed("app.css", b"body{ }"));
    }

    #[test]
    fn an_extensionless_name_still_gets_its_hash() {
        assert!(hashed("LICENSE", b"...").starts_with("LICENSE-"));
    }
}
