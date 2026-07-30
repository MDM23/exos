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
fn script_content_is_verbatim_source() {
    // Inside a raw-text element the source is passed through untouched, so
    // `a && b` must not become `a &amp;&amp; b`. It is also not a Rust string
    // literal: quotes here would reach the page.
    let markup = view! { <script>a && b</script> };
    assert_eq!(markup.as_str(), "<script>a && b</script>");
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
