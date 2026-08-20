//! What a stylesheet's `url()`s turn into.
//!
//! The fixtures next door are a stylesheet importing another one, both naming
//! files beside them.

use std::path::Path;

use exos_build::{Built, Mode};

fn app() -> Built {
    exos_build::build(Path::new("tests/fixtures/app.css"), None, Mode::Debug)
        .expect("the fixtures build")
}

fn code(built: &Built) -> String {
    String::from_utf8(built.bytes.clone()).expect("a stylesheet is text")
}

/// The hashed name a referenced file was embedded under.
fn file(built: &Built, name: &str) -> String {
    built
        .referenced
        .iter()
        .find(|asset| asset.name == name)
        .unwrap_or_else(|| panic!("{name} was not embedded"))
        .file
        .clone()
}

#[test]
fn a_referenced_file_is_embedded_and_its_url_points_at_the_hashed_name() {
    let built = app();
    let dot = file(&built, "dot.svg");

    assert!(dot.starts_with("dot-") && dot.ends_with(".svg"), "{dot}");
    assert!(
        code(&built).contains(&format!("./{dot}")),
        "{}",
        code(&built)
    );

    let embedded = built
        .referenced
        .iter()
        .find(|asset| asset.name == "dot.svg")
        .expect("embedded");

    assert!(embedded.bytes.starts_with(b"<svg"), "its own bytes go in");
    assert_eq!(embedded.content_type, "image/svg+xml");
}

/// The rewritten URL has to stay relative. Assets share one directory, so a
/// stylesheet reaches them from its own URL whatever prefix the application is
/// mounted under, and nothing has to be told at run time what that prefix is.
#[test]
fn the_rewritten_url_is_relative_to_the_stylesheet() {
    let code = code(&app());

    assert!(
        !code.contains("url(\"/"),
        "an absolute URL would break under a base: {code}"
    );
}

/// `../images/dot.svg` in the imported stylesheet and `images/dot.svg` in the
/// entry point are the same file, said from two directories.
#[test]
fn a_url_resolves_against_the_stylesheet_that_wrote_it() {
    let built = app();
    let dot = file(&built, "dot.svg");
    let code = code(&built);

    assert_eq!(
        code.matches(&format!("./{dot}")).count(),
        3,
        "every reference lands on one embedded file: {code}"
    );

    assert_eq!(
        built.referenced.len(),
        2,
        "dot.svg and sprite.svg, each embedded once"
    );
}

/// The one place the relative rewrite cannot reach. A browser substitutes a
/// custom property where the `var()` is used and resolves the URL against the
/// page, which is any route, so there is no URL to write and the build says so
/// rather than shipping one that happens to work at the root.
#[test]
fn a_url_in_a_custom_property_is_refused_with_the_reason() {
    let error = exos_build::build(Path::new("tests/fixtures/tokens.css"), None, Mode::Debug)
        .expect_err("no rewriting can make this right");

    let message = format!("{error}");
    assert!(
        message.contains("--dot"),
        "it names the property: {message}"
    );
    assert!(message.contains("images/dot.svg"), "and the URL: {message}");
    assert!(
        message.contains("rule that uses the variable"),
        "and what to do instead: {message}"
    );
}

/// What that refusal leaves an author to write instead, and what a value
/// exos cannot parse looks like on the way through: two files behind one
/// declaration, both embedded.
#[test]
fn urls_inside_a_function_in_an_ordinary_declaration_are_rewritten() {
    let built = exos_build::build(
        Path::new("tests/fixtures/light-dark.css"),
        None,
        Mode::Debug,
    )
    .expect("an ordinary declaration is not ambiguous");

    let code = code(&built);

    for name in ["dot.svg", "sprite.svg"] {
        assert!(
            code.contains(&format!("./{}", file(&built, name))),
            "{name} is still written as {code}"
        );
    }
}

#[test]
fn a_query_or_fragment_belongs_to_the_url_and_survives() {
    let built = app();
    let code = code(&built);

    assert!(
        code.contains(&format!("./{}?v=2", file(&built, "dot.svg"))),
        "{code}"
    );

    assert!(
        code.contains(&format!("./{}#pin", file(&built, "sprite.svg"))),
        "{code}"
    );
}

#[test]
fn a_url_this_pipeline_does_not_reach_is_left_alone() {
    assert!(
        code(&app()).contains("https://example.com/dot.svg"),
        "another host is not ours to embed"
    );
}

/// A referenced file decides its own hash, and that hash is written into the
/// stylesheet, so editing an image has to rebuild whatever embeds it.
#[test]
fn a_referenced_file_is_a_source_of_the_stylesheet() {
    let sources = app().sources;

    for name in ["app.css", "panel.css", "dot.svg", "sprite.svg"] {
        assert!(
            sources.iter().any(|source| source.ends_with(name)),
            "{name} is missing from {sources:?}"
        );
    }
}

#[test]
fn a_url_that_names_nothing_says_which_stylesheet_wrote_it() {
    let error = exos_build::build(Path::new("tests/fixtures/missing.css"), None, Mode::Debug)
        .expect_err("the file it names is not there");

    let message = format!("{error}");
    assert!(message.contains("gone.svg"), "{message}");
    assert!(message.contains("missing.css"), "{message}");
}
