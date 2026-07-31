//! Stylesheet bundling.

use std::path::Path;

use lightningcss::{
    bundler::{Bundler, FileProvider},
    stylesheet::{MinifyOptions, ParserOptions, PrinterOptions},
    targets::Targets,
};

use crate::{Error, Result};

/// Bundles an entry point and everything it imports into one stylesheet.
///
/// `StyleSheet::parse` only parses: it leaves `@import` rules standing, and
/// the browser then goes looking for a file the application never serves.
/// Resolving them is the bundler's job, and that is a separate API.
pub(crate) fn bundle(path: &Path, minify: bool) -> Result<String> {
    let provider = FileProvider::new();
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

    sheet
        .to_css(PrinterOptions {
            minify,
            targets,
            ..PrinterOptions::default()
        })
        .map(|output| output.code)
        .map_err(|error| Error::Css {
            path: path.to_path_buf(),
            message: error.to_string(),
        })
}
