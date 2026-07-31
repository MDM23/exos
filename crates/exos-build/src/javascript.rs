//! Script minification.

use std::path::Path;

use crate::{Error, Result};

/// Strips comments and redundant whitespace.
///
/// Deliberately conservative: no AST rewriting, so it cannot change what the
/// code means. The aggressive minifiers buy perhaps another third, before
/// compression, in exchange for the chance of miscompiling a program at build
/// time. For a build step that has to be invisible, "cannot break the code" is
/// worth more than the last few kilobytes.
pub(crate) fn minify(source: &[u8], path: &Path) -> Result<Vec<u8>> {
    let text = core::str::from_utf8(source).map_err(|error| Error::Javascript {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;

    // A minifier that cannot parse the file is reporting a syntax error in it.
    // Failing the build beats shipping something that breaks in the browser.
    minifier::js::minify(text)
        .map(|minified| minified.to_string().into_bytes())
        .map_err(|error| Error::Javascript {
            path: path.to_path_buf(),
            message: error.to_owned(),
        })
}
