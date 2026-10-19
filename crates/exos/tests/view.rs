//! The `view!` macro, exercised only through the public API.

use exos::{Markup, view};

#[test]
fn renders_real_html_including_void_elements() {
    let markup = view! {
        <section class="files">
            <h1>"Files"</h1>
            <br>
            <input type="text" disabled>
        </section>
    };

    assert_eq!(
        markup.as_str(),
        "<section class=\"files\"><h1>Files</h1><br><input type=\"text\" disabled></section>"
    );
}

#[test]
fn interpolated_values_are_escaped() {
    let name = r#"<img src=x onerror="alert(1)">"#;
    let markup = view! { <span>{ name }</span> };

    // Runtime values escape quotes as well as angle brackets, so the same
    // value is safe in text and in attribute position without the caller
    // having to know which one it landed in.
    assert_eq!(
        markup.as_str(),
        "<span>&lt;img src=x onerror=&quot;alert(1)&quot;&gt;</span>"
    );
}

#[test]
fn attribute_values_cannot_break_out_of_their_quotes() {
    let injected = r#"" onclick="evil()"#;
    let markup = view! { <div title={ injected }></div> };

    assert_eq!(
        markup.as_str(),
        "<div title=\"&quot; onclick=&quot;evil()\"></div>"
    );
}

#[test]
fn markup_is_the_one_way_to_write_raw_html() {
    let raw = Markup(String::from("<b>bold</b>"));
    let markup = view! { <p>{ raw }</p> };

    assert_eq!(markup.as_str(), "<p><b>bold</b></p>");
}

#[test]
fn none_drops_the_attribute_entirely() {
    let here: Option<&str> = Some("page");
    let elsewhere: Option<&str> = None;

    assert_eq!(
        view! { <a aria-current={ here }></a> }.as_str(),
        "<a aria-current=\"page\"></a>"
    );
    assert_eq!(
        view! { <a aria-current={ elsewhere }></a> }.as_str(),
        "<a></a>"
    );
}

#[test]
fn script_content_is_the_value_of_its_literal() {
    // Unescaped, so `a && b` does not become `a &amp;&amp; b`, and without the
    // quotes. A backtick is not a Rust token, which is why the content is a
    // literal at all. Release builds minify it.
    let markup = view! { <script>r#"a && `b`"#</script> };
    let expected = if cfg!(debug_assertions) {
        "<script>a && `b`</script>"
    } else {
        "<script>a&&`b`</script>"
    };
    assert_eq!(markup.as_str(), expected);
}

#[test]
fn inline_styles_minify_in_release_builds() {
    // `:first-child` is what bare tokens came back as `: first - child`.
    let markup = view! { <style>"p > :first-child { color: red; }"</style> };
    let expected = if cfg!(debug_assertions) {
        "<style>p > :first-child { color: red; }</style>"
    } else {
        "<style>p>:first-child{color:red}</style>"
    };
    assert_eq!(markup.as_str(), expected);
}

#[test]
fn a_script_holding_data_is_never_minified() {
    let markup = view! { <script type="application/json">r#"{ "a": 1 }"#</script> };
    assert_eq!(
        markup.as_str(),
        "<script type=\"application/json\">{ \"a\": 1 }</script>"
    );
}

#[test]
fn a_collection_renders_in_order() {
    let rows = ["a", "b"]
        .iter()
        .map(|name| view! { <li>{ name }</li> })
        .collect::<Vec<_>>();

    assert_eq!(
        view! { <ul>{ rows }</ul> }.as_str(),
        "<ul><li>a</li><li>b</li></ul>"
    );
}
