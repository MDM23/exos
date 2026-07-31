//! Build-time asset pipeline for [exos](https://docs.rs/exos).
//!
//! Call this from a `build.rs`. It bundles CSS, resolving `@import` into one
//! sheet, minifies JavaScript, content-hashes every output, and writes a Rust
//! file holding the bytes. The application then serves its assets out of its
//! own binary: nothing to deploy alongside it, no directory to fall out of
//! sync, and `cargo run` is the whole toolchain.
//!
//! ```no_run
//! # fn main() -> Result<(), exos_build::Error> {
//! exos_build::Assets::new().css("css/app.css")?.emit()?;
//! # Ok(())
//! # }
//! ```
//!
//! Debug builds skip minification. The only thing it buys during development
//! is a slower edit cycle and unreadable stack traces.

use std::{
    env, fs,
    path::{Path, PathBuf},
};

mod codegen;
mod css;
mod javascript;

pub use crate::codegen::GENERATED_FILE;

use crate::codegen::Generated;

/// Anything that can go wrong while building assets.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// A source file could not be read, or an output could not be written.
    #[error("cannot access {path}")]
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

    /// A script could not be minified, which means it does not parse.
    #[error("cannot minify script {path}: {message}")]
    Javascript {
        /// The file that failed.
        path: PathBuf,
        /// What the minifier reported.
        message: String,
    },

    /// `OUT_DIR` was missing, so this is not running as a build script.
    #[error("OUT_DIR is not set; Assets is meant to be used from a build script")]
    NotABuildScript,
}

/// The result type used throughout this crate.
pub type Result<T, E = Error> = core::result::Result<T, E>;

/// One processed asset.
#[derive(Clone, Debug)]
struct Asset {
    /// The name callers look it up by, such as `app.css`.
    name: String,
    /// The hashed file name, such as `app-9f2c1b4e.css`.
    file: String,
    content_type: &'static str,
    bytes: Vec<u8>,
}

/// Collects the assets a crate ships and writes them into the build output.
///
/// Every method takes `self` and returns it, so a pipeline reads as one
/// expression.
#[derive(Debug, Default)]
pub struct Assets {
    assets: Vec<Asset>,
    minify: Option<bool>,
}

impl Assets {
    /// An empty pipeline.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Overrides the default, which is to minify in release and not in debug.
    #[must_use = "the pipeline is consumed, so the returned value is the one to keep building on"]
    pub fn minify(mut self, minify: bool) -> Self {
        self.minify = Some(minify);
        self
    }

    fn should_minify(&self) -> bool {
        self.minify
            .unwrap_or_else(|| env::var("PROFILE").is_ok_and(|profile| profile == "release"))
    }

    /// Bundles a stylesheet, inlining everything it imports.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Css`] when the entry point or anything it imports
    /// fails to parse.
    pub fn css(mut self, path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();

        // The whole directory, not just the entry file. An imported file is
        // part of this asset, and watching only the entry means editing a
        // token file changes nothing until something else forces a rebuild.
        // Cargo watches a directory recursively.
        match path.parent() {
            Some(directory) if !directory.as_os_str().is_empty() => watch(directory),
            _ => watch(path),
        }

        let bundled = css::bundle(path, self.should_minify())?;
        self.assets.push(Asset::new(
            "text/css; charset=utf-8",
            file_name(path),
            bundled.into_bytes(),
        ));

        Ok(self)
    }

    /// Adds one script.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] when the file cannot be read, or
    /// [`Error::Javascript`] when minification rejects it.
    pub fn js(mut self, path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        watch(path);

        let source = read(path)?;
        let bytes = if self.should_minify() {
            javascript::minify(&source, path)?
        } else {
            source
        };

        self.assets.push(Asset::new(
            "text/javascript; charset=utf-8",
            file_name(path),
            bytes,
        ));

        Ok(self)
    }

    /// Concatenates several scripts into one asset, in the order given.
    ///
    /// A runtime and its plugins should be one request, and that order is the
    /// author's business rather than a resolver's, so it is not sorted.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] when a file cannot be read, or
    /// [`Error::Javascript`] when minification rejects one.
    pub fn js_bundle(mut self, name: &str, paths: &[&str]) -> Result<Self> {
        let mut combined = Vec::new();

        for path in paths {
            let path = Path::new(path);
            watch(path);

            let source = read(path)?;
            let piece = if self.should_minify() {
                javascript::minify(&source, path)?
            } else {
                source
            };

            combined.extend_from_slice(&piece);

            // Each file is its own scope, but a trailing statement without a
            // semicolon would fuse with the next file's opening paren and be
            // parsed as a call.
            combined.extend_from_slice(b"\n;\n");
        }

        self.assets.push(Asset::new(
            "text/javascript; charset=utf-8",
            name.to_owned(),
            combined,
        ));

        Ok(self)
    }

    /// Adds an already-built file unchanged, such as a font or an image.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] when the file cannot be read.
    pub fn raw(mut self, path: impl AsRef<Path>, content_type: &'static str) -> Result<Self> {
        let path = path.as_ref();
        watch(path);

        self.assets
            .push(Asset::new(content_type, file_name(path), read(path)?));

        Ok(self)
    }

    /// Writes the generated table into `OUT_DIR`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::NotABuildScript`] outside a build script, and
    /// [`Error::Io`] when the output cannot be written.
    pub fn emit(self) -> Result<()> {
        let out = env::var_os("OUT_DIR").ok_or(Error::NotABuildScript)?;
        Generated::new(&self.assets).write(Path::new(&out))
    }
}

impl Asset {
    fn new(content_type: &'static str, name: String, bytes: Vec<u8>) -> Self {
        let digest = <sha2::Sha256 as sha2::Digest>::digest(&bytes);
        let short: String = digest
            .iter()
            .take(6)
            .map(|byte| format!("{byte:02x}"))
            .collect();

        let file = match name.rsplit_once('.') {
            Some((stem, extension)) => format!("{stem}-{short}.{extension}"),
            None => format!("{name}-{short}"),
        };

        Self {
            name,
            file,
            content_type,
            bytes,
        }
    }
}

fn read(path: &Path) -> Result<Vec<u8>> {
    fs::read(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

/// Only the paths actually read trigger a rebuild. Declaring the whole crate
/// would rebuild on every editor swap file.
fn watch(path: &Path) {
    println!("cargo:rerun-if-changed={}", path.display());
}
