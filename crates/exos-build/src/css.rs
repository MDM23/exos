//! Stylesheet bundling.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Mutex,
};

use lightningcss::{
    bundler::{Bundler, FileProvider, ResolveResult, SourceProvider},
    properties::{Property, custom::CustomPropertyName},
    rules::CssRule,
    stylesheet::{MinifyOptions, ParserOptions, PrinterOptions},
    targets::Targets,
    values::url::Url,
    visit_types,
    visitor::{Visit, VisitTypes, Visitor},
};

use crate::{Built, Error, Mode, Result};

/// A bundled stylesheet, the files that went into it, and the assets its
/// `url()`s name.
pub(crate) struct Bundled {
    pub(crate) code: String,
    pub(crate) sources: Vec<PathBuf>,
    pub(crate) assets: Vec<Built>,
}

/// Bundles an entry point and everything it imports into one stylesheet.
///
/// `StyleSheet::parse` only parses: it leaves `@import` rules standing, and
/// the browser then goes looking for a file the application never serves.
/// Resolving them is the bundler's job, and that is a separate API.
pub(crate) fn bundle(path: &Path, mode: Mode) -> Result<Bundled> {
    let provider = Recording::new();
    let mut bundler = Bundler::new(&provider, None, ParserOptions::default());

    let mut sheet = bundler.bundle(path).map_err(|error| Error::Css {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;

    // The visitor needs the file list while the sheet is borrowed mutably, and
    // it is a handful of paths.
    let sources: Vec<PathBuf> = sheet.sources.iter().map(PathBuf::from).collect();
    let mut references = References::new(mode, &sources);

    // Before minifying, so that what is printed is already the rewritten URL
    // rather than a placeholder to be found again in the output.
    sheet.visit(&mut references)?;

    // No browser targets: minify and bundle, but do not downlevel. Modern CSS
    // is the input and the output, and a transform that silently rewrites
    // light-dark() or nesting is a surprise rather than a service.
    let targets = Targets::default();
    let minify = mode.minify();

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

    let mut sources = provider.sources();

    // A referenced file's bytes decide its hash and its hash is written into
    // this stylesheet, so editing an image changes the CSS as surely as
    // editing a rule does.
    sources.extend(
        references
            .assets
            .iter()
            .flat_map(|asset| asset.sources.iter().cloned()),
    );

    Ok(Bundled {
        code,
        sources,
        assets: references.assets,
    })
}

/// Rewrites every `url()` to the asset it names, building it on the way.
struct References<'sources> {
    mode: Mode,
    /// The file behind each source index, so a URL can be resolved against the
    /// stylesheet that wrote it.
    sources: &'sources [PathBuf],
    /// Which of them the walk is inside.
    source: u32,
    /// The custom property being walked, if it is one.
    custom: Option<String>,
    /// The hashed name each file was built under, so a logo behind ten rules is
    /// read and hashed once.
    known: HashMap<PathBuf, String>,
    /// Everything built, flat: an asset's own references are lifted in beside
    /// it rather than left nested.
    assets: Vec<Built>,
}

impl<'sources> References<'sources> {
    fn new(mode: Mode, sources: &'sources [PathBuf]) -> Self {
        Self {
            mode,
            sources,
            source: 0,
            custom: None,
            known: HashMap::new(),
            assets: Vec::new(),
        }
    }

    /// The stylesheet currently being walked.
    ///
    /// A URL is resolved against the file that wrote it rather than against the
    /// entry point, which is the only reading under which an imported
    /// stylesheet can be moved without rewriting every URL in it.
    fn source(&self) -> &Path {
        self.sources
            .get(self.source as usize)
            .map_or(Path::new(""), PathBuf::as_path)
    }

    /// Embeds a file named relative to that stylesheet, and answers with the
    /// hashed name it is served under.
    fn embed(&mut self, target: &str) -> Result<String> {
        let source = self.source();
        let path = source.parent().unwrap_or(Path::new("")).join(target);

        // `images/dot.svg` from here and `../images/dot.svg` from an imported
        // stylesheet are one file, and the file is what the hash is of.
        let path = path.canonicalize().unwrap_or(path);

        if let Some(file) = self.known.get(&path) {
            return Ok(file.clone());
        }

        let mut built = crate::build(&path, None, self.mode).map_err(|error| Error::Reference {
            path: source.to_path_buf(),
            url: target.to_owned(),
            source: Box::new(error),
        })?;

        let file = built.file.clone();
        self.known.insert(path, file.clone());
        self.assets.append(&mut built.referenced);
        self.assets.push(built);

        Ok(file)
    }
}

impl<'i> Visitor<'i> for References<'_> {
    type Error = Error;

    fn visit_types(&self) -> VisitTypes {
        visit_types!(PROPERTIES | RULES | URLS)
    }

    fn visit_rule(&mut self, rule: &mut CssRule<'i>) -> Result<()> {
        // A `url()` does not carry the file it was written in, but the rule
        // around it does. After bundling, one file's rules are contiguous, so
        // the last rule to say holds until another one does.
        if let Some(source) = source_index(rule) {
            self.source = source;
        }

        rule.visit_children(self)
    }

    /// Remembers a custom property while walking what it was declared with, so
    /// that a `url()` inside one can be told from a `url()` in any other
    /// declaration.
    fn visit_property(&mut self, property: &mut Property<'i>) -> Result<()> {
        // An unknown property is also a `Custom` here, but the browser drops it
        // rather than substituting it anywhere, so it is not the same case.
        self.custom = match property {
            Property::Custom(declaration) => match &declaration.name {
                CustomPropertyName::Custom(name) => Some(name.as_ref().to_owned()),
                CustomPropertyName::Unknown(_) => None,
            },
            _ => None,
        };

        let visited = property.visit_children(self);
        self.custom = None;

        visited
    }

    /// The rewritten URL stays relative, and that is the whole trick. Every
    /// asset is served from one directory, so a stylesheet at
    /// `/_exos/app-1a2b.css` reaches its logo as `./logo-9f2c.png` whatever
    /// prefix the application ends up mounted under, and nothing has to be told
    /// at run time what that prefix is.
    ///
    /// Inside a custom property that trick fails, and so does every other one:
    /// a browser substitutes the tokens where the `var()` is used and resolves
    /// the URL against the page, not against the stylesheet. A page is any
    /// route, so there is no URL to write. Say so instead of writing one that
    /// works on the routes somebody tried.
    fn visit_url(&mut self, url: &mut Url<'i>) -> Result<()> {
        if url.is_absolute() {
            return Ok(());
        }

        let (target, suffix) = split(&url.url);

        if target.is_empty() {
            return Ok(());
        }

        if let Some(property) = &self.custom {
            return Err(Error::CustomProperty {
                path: self.source().to_path_buf(),
                property: property.clone(),
                url: url.url.to_string(),
            });
        }

        let rewritten = format!("./{}{suffix}", self.embed(target)?);
        url.url = rewritten.into();

        Ok(())
    }
}

/// The source file a rule says it came from.
///
/// Only the rules that can hold a declaration are worth asking, since those are
/// the ones a `url()` can be inside. Everything else keeps whatever the last
/// answer was.
fn source_index<T>(rule: &CssRule<'_, T>) -> Option<u32> {
    let location = match rule {
        CssRule::CounterStyle(rule) => rule.loc,
        CssRule::FontFace(rule) => rule.loc,
        CssRule::FontPaletteValues(rule) => rule.loc,
        CssRule::Keyframes(rule) => rule.loc,
        CssRule::NestedDeclarations(rule) => rule.loc,
        CssRule::Page(rule) => rule.loc,
        CssRule::Style(rule) => rule.loc,
        CssRule::Viewport(rule) => rule.loc,
        _ => return None,
    };

    Some(location.source_index)
}

/// The file a URL names, and the query or fragment that belongs to the URL
/// rather than to it: `sprite.svg#pin` is one file and many icons.
fn split(url: &str) -> (&str, &str) {
    url.split_at(url.find(['?', '#']).unwrap_or(url.len()))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_url_keeps_the_fragment_that_picks_an_icon_out_of_a_sprite() {
        assert_eq!(split("sprite.svg#pin"), ("sprite.svg", "#pin"));
        assert_eq!(split("font.woff2?v=2"), ("font.woff2", "?v=2"));
        assert_eq!(split("logo.png"), ("logo.png", ""));
    }
}
