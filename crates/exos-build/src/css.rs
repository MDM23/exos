//! Stylesheet bundling.

use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

use lightningcss::{
    bundler::{Bundler, FileProvider, ResolveResult, SourceProvider},
    stylesheet::{MinifyOptions, ParserOptions, PrinterOptions},
    targets::Targets,
};

use crate::{Error, Result};

/// A bundled stylesheet and the files that went into it.
pub(crate) struct Bundled {
    pub(crate) code: String,
    pub(crate) sources: Vec<PathBuf>,
}

/// Bundles an entry point and everything it imports into one stylesheet.
///
/// `StyleSheet::parse` only parses: it leaves `@import` rules standing, and
/// the browser then goes looking for a file the application never serves.
/// Resolving them is the bundler's job, and that is a separate API.
pub(crate) fn bundle(path: &Path, minify: bool) -> Result<Bundled> {
    let provider = Recording::new();
    let mut bundler = Bundler::new(&provider, None, ParserOptions::default());

    let mut sheet = bundler.bundle(path).map_err(|error| Error::Css {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;

    // No browser targets: minify and bundle, but do not downlevel. Modern CSS
    // is the input and the output, and a transform that silently rewrites
    // light-dark() or nesting is a surprise rather than a service.
    let targets = Targets::default();

    if minify {
        sheet
            .minify(MinifyOptions {
                targets,
                ..MinifyOptions::default()
            })
            .map_err(|error| Error::Css {
                path: path.to_path_buf(),
                message: error.to_string(),
            })?;
    }

    let code = sheet
        .to_css(PrinterOptions {
            minify,
            targets,
            ..PrinterOptions::default()
        })
        .map(|output| output.code)
        .map_err(|error| Error::Css {
            path: path.to_path_buf(),
            message: error.to_string(),
        })?;

    Ok(Bundled {
        code,
        sources: provider.sources(),
    })
}

/// A [`FileProvider`] that remembers what it was asked to read.
///
/// The bundler is the only thing that knows which files an entry point pulls
/// in, so this asks it rather than guessing. Watching the whole directory
/// instead would rebuild on every editor swap file, and watching only the
/// entry point would miss edits to an imported one.
struct Recording {
    inner: FileProvider,
    sources: Mutex<Vec<PathBuf>>,
}

impl Recording {
    fn new() -> Self {
        Self {
            inner: FileProvider::new(),
            sources: Mutex::new(Vec::new()),
        }
    }

    /// Takes a copy rather than consuming, because the bundled stylesheet
    /// borrows from this provider and so outlives the question.
    fn sources(&self) -> Vec<PathBuf> {
        self.sources
            .lock()
            .map(|sources| sources.clone())
            .unwrap_or_default()
    }
}

impl SourceProvider for Recording {
    type Error = std::io::Error;

    fn read<'provider>(&'provider self, file: &Path) -> Result<&'provider str, Self::Error> {
        if let Ok(mut sources) = self.sources.lock() {
            sources.push(file.to_path_buf());
        }

        self.inner.read(file)
    }

    fn resolve(
        &self,
        specifier: &str,
        originating_file: &Path,
    ) -> Result<ResolveResult, Self::Error> {
        self.inner.resolve(specifier, originating_file)
    }
}
