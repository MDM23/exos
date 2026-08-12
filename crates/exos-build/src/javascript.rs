//! Script bundling and minification.

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use crate::{Error, Result, read};

/// A bundled script and the files that went into it.
pub(crate) struct Bundled {
    pub(crate) code: String,
    pub(crate) sources: Vec<PathBuf>,
}

/// Bundles an entry point and everything it imports into one script.
///
/// Imports are the only ordering mechanism: a module runs after everything it
/// imports, which is the same promise `@import` makes in a stylesheet and the
/// same one the browser makes for real modules. A file imported twice is
/// included once.
pub(crate) fn bundle(path: &Path, minify: bool) -> Result<Bundled> {
    let mut walk = Walk::default();
    walk.visit(path)?;

    let mut combined = String::new();

    for source in &walk.sources {
        combined.push_str(&strip_imports(&text(source)?));

        // Each file is its own scope, but a trailing statement without a
        // semicolon would fuse with the next file's opening paren and be
        // parsed as a call.
        combined.push_str("\n;\n");
    }

    let code = if minify {
        // A minifier that cannot parse the bundle is reporting a syntax error
        // in it. Failing the build beats shipping something that breaks in the
        // browser.
        minifier::js::minify(&combined)
            .map(|minified| minified.to_string())
            .map_err(|error| Error::Javascript {
                path: path.to_path_buf(),
                message: error.to_owned(),
            })?
    } else {
        combined
    };

    Ok(Bundled {
        code,
        sources: walk.sources,
    })
}

/// A depth-first walk of the import graph, collecting files in run order.
#[derive(Default)]
struct Walk {
    /// Files already placed, in the order they must run.
    sources: Vec<PathBuf>,
    /// What `sources` holds, for a cheaper membership test.
    done: HashSet<PathBuf>,
    /// The chain currently being resolved, so a cycle is caught rather than
    /// silently truncated.
    active: Vec<PathBuf>,
}

impl Walk {
    fn visit(&mut self, path: &Path) -> Result<()> {
        let canonical = path.canonicalize().map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })?;

        // Imported from two places, which is not a cycle. Its first appearance
        // already satisfies everyone waiting on it.
        if self.done.contains(&canonical) {
            return Ok(());
        }

        if self.active.contains(&canonical) {
            let chain: Vec<String> = self
                .active
                .iter()
                .chain(core::iter::once(&canonical))
                .map(|path| name(path))
                .collect();

            return Err(Error::Javascript {
                path: canonical,
                message: format!(
                    "import cycle: {}. Side-effect imports run in order, so a \
                     cycle asks for two files to run before each other",
                    chain.join(" -> ")
                ),
            });
        }

        let directory = canonical.parent().unwrap_or(Path::new("")).to_path_buf();
        self.active.push(canonical.clone());

        for import in imports(&text(&canonical)?, &canonical)? {
            self.visit(&directory.join(&import))?;
        }

        self.active.pop();
        self.done.insert(canonical.clone());
        self.sources.push(canonical);

        Ok(())
    }
}

/// The specifiers a module imports for their side effects.
///
/// The scan is line based, which is what a bundler that concatenates can
/// honour: an import has to sit on its own line, and only the side-effect form
/// is supported. Anything else is refused by name rather than misread.
fn imports(source: &str, path: &Path) -> Result<Vec<String>> {
    let mut found = Vec::new();
    let mut in_comment = false;

    for (index, raw) in source.lines().enumerate() {
        let line = raw.trim();
        let number = index + 1;

        if in_comment {
            if line.contains("*/") {
                in_comment = false;
            }
            continue;
        }

        if line.starts_with("//") {
            continue;
        }

        if let Some(start) = line.find("/*") {
            if !line[start..].contains("*/") {
                in_comment = true;
            }
            continue;
        }

        let Some(rest) = keyword(line, "import") else {
            if keyword(line, "export").is_some() {
                return Err(unsupported(
                    path,
                    number,
                    "`export` has no meaning once files are concatenated",
                ));
            }
            continue;
        };

        // `import(...)` is a runtime expression, not a static import. It stays
        // in the code and resolves in the browser.
        if rest.starts_with('(') {
            continue;
        }

        let Some(specifier) = quoted(rest) else {
            return Err(unsupported(
                path,
                number,
                "only `import \"./file.js\";` is supported, because a bundle \
                 that concatenates cannot rename or hoist bindings",
            ));
        };

        if !specifier.starts_with('.') {
            return Err(unsupported(
                path,
                number,
                "only relative specifiers are supported; there is no package \
                 resolution and nothing is fetched",
            ));
        }

        found.push(specifier);
    }

    Ok(found)
}

/// The rest of the line after `word`, when the line starts with it as a word
/// rather than as a prefix of a longer identifier.
fn keyword<'line>(line: &'line str, word: &str) -> Option<&'line str> {
    let rest = line.strip_prefix(word)?;

    match rest.chars().next() {
        Some(next) if next.is_alphanumeric() || next == '_' || next == '$' => None,
        _ => Some(rest.trim_start()),
    }
}

/// The contents of the leading string literal, if the rest of the line is just
/// that literal and an optional semicolon.
fn quoted(rest: &str) -> Option<String> {
    let quote = rest.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }

    let body = &rest[quote.len_utf8()..];
    let end = body.find(quote)?;
    let tail = body[end + quote.len_utf8()..].trim();

    // Anything after the statement would be dropped along with the line.
    (tail.is_empty() || tail == ";").then(|| body[..end].to_owned())
}

/// A module with its import statements removed, since concatenation is what
/// replaces them.
fn strip_imports(source: &str) -> String {
    let mut kept: Vec<&str> = Vec::new();
    let mut in_comment = false;

    for raw in source.lines() {
        let line = raw.trim();

        if in_comment {
            if line.contains("*/") {
                in_comment = false;
            }
            kept.push(raw);
            continue;
        }

        if let Some(start) = line.find("/*") {
            if !line[start..].contains("*/") {
                in_comment = true;
            }
            kept.push(raw);
            continue;
        }

        let is_import = keyword(line, "import")
            .is_some_and(|rest| !rest.starts_with('(') && quoted(rest).is_some());

        if !is_import {
            kept.push(raw);
        }
    }

    kept.join("\n")
}

fn unsupported(path: &Path, line: usize, message: &str) -> Error {
    Error::Javascript {
        path: path.to_path_buf(),
        message: format!("{}:{line}: {message}", name(path)),
    }
}

fn name(path: &Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

fn text(path: &Path) -> Result<String> {
    String::from_utf8(read(path)?).map_err(|error| Error::Javascript {
        path: path.to_path_buf(),
        message: error.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(source: &str) -> Result<Vec<String>> {
        imports(source, Path::new("app.js"))
    }

    #[test]
    fn finds_side_effect_imports_in_order() {
        let found =
            parse("import \"./a.js\";\nimport './b.js'\nconst x = 1;").expect("both imports parse");

        assert_eq!(found, vec!["./a.js", "./b.js"]);
    }

    #[test]
    fn ignores_imports_inside_comments() {
        let found = parse("// import \"./a.js\";\n/*\nimport \"./b.js\";\n*/\nlet y;")
            .expect("comments hide nothing that matters");

        assert!(found.is_empty());
    }

    #[test]
    fn does_not_mistake_an_identifier_for_the_keyword() {
        assert!(
            parse("importantThing();")
                .expect("not an import")
                .is_empty()
        );
    }

    #[test]
    fn leaves_dynamic_import_to_the_browser() {
        assert!(
            parse("import(\"./late.js\");")
                .expect("an expression")
                .is_empty()
        );
    }

    #[test]
    fn refuses_named_imports_by_name() {
        let error = parse("import { run } from \"./a.js\";").expect_err("unsupported");
        assert!(format!("{error}").contains("only `import"), "{error}");
    }

    #[test]
    fn refuses_bare_specifiers() {
        let error = parse("import \"lodash\";").expect_err("unsupported");
        assert!(format!("{error}").contains("relative"), "{error}");
    }

    #[test]
    fn refuses_exports() {
        let error = parse("export const x = 1;").expect_err("unsupported");
        assert!(format!("{error}").contains("concatenated"), "{error}");
    }

    #[test]
    fn refuses_trailing_code_it_would_have_to_drop() {
        let error = parse("import \"./a.js\"; run();").expect_err("unsupported");
        assert!(format!("{error}").contains("only `import"), "{error}");
    }

    #[test]
    fn stripping_removes_the_import_and_keeps_everything_else() {
        let stripped = strip_imports("import \"./a.js\";\nconst x = 1;\n");

        assert!(!stripped.contains("import"));
        assert!(stripped.contains("const x = 1;"));
    }
}
