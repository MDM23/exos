//! Attribute blocks and the typed handlers that hang on them.

use exos::{Js, bind, class, on_click, show, signal, text, view, when};

#[test]
fn a_signal_handle_declares_itself_on_its_element() {
    let gone = signal(false);
    let markup = view! { <li id="row" {&gone}></li> };

    assert_eq!(
        markup.as_str(),
        format!(
            "<li id=\"row\" data-signals=\"{{&quot;{}&quot;:false}}\"></li>",
            gone.name()
        )
    );
}

#[test]
fn repeated_blocks_merge_into_one_attribute() {
    let open = signal(false);
    let busy = signal(false);

    let markup = view! {
        <div id="panel" {(&open, &busy)} {class("is-open", open.get())} {class("busy", busy.get())}>
        </div>
    };

    assert_eq!(markup.as_str().matches("data-class").count(), 1);
    assert_eq!(markup.as_str().matches("data-signals").count(), 1);
}

#[test]
fn a_handler_compiles_to_javascript() {
    let gone = signal(false);

    let markup = view! {
        <button {on_click(|_| gone.set(true))}>"Delete"</button>
    };

    assert_eq!(
        markup.as_str(),
        format!(
            "<button data-on-click=\"$.{} = true\">Delete</button>",
            gone.name()
        )
    );
}

#[test]
fn a_binding_carries_the_signals_type_across() {
    let picked = signal(Vec::<u32>::new());
    let markup = view! { <input type="checkbox" value="1" {bind(&picked)}> };

    assert!(
        markup
            .as_str()
            .contains(&format!("data-bind=\"{}\"", picked.name()))
    );
    assert!(markup.as_str().contains("data-bind-kind=\"number\""));
}

#[test]
fn a_derived_value_reads_the_signal_rather_than_copying_it() {
    let picked = signal(Vec::<u32>::new());

    let markup = view! {
        <div id="bar" {&picked} {show(picked.get().any())}>
            <span {text(picked.get().len())}></span>
        </div>
    };

    assert!(
        markup
            .as_str()
            .contains(&format!("data-show=\"$.{}.length &gt; 0\"", picked.name()))
    );
    assert!(
        markup
            .as_str()
            .contains(&format!("data-text=\"$.{}.length\"", picked.name()))
    );
}

#[test]
fn branching_in_the_browser_records_a_conditional() {
    let count = signal(Vec::<u32>::new());

    let markup = view! {
        <button {on_click(|_| {
            when(count.get().any(), |()| count.clear());
        })}>"Clear"</button>
    };

    assert!(
        markup.as_str().contains(&format!(
            "data-on-click=\"if ($.{name}.length &gt; 0) {{ $.{name} = [] }}\"",
            name = count.name()
        )),
        "{}",
        markup.as_str()
    );
}

#[test]
fn a_signal_used_but_never_declared_is_inferred() {
    // `_order` belongs to the sortable plugin, so no handle can declare it.
    // The macro reads the name out of the expression and declares it as null.
    let markup = view! {
        <ul id="list" data-sortable="post('/reorder', { order: $._order })"></ul>
    };

    assert!(
        markup
            .as_str()
            .contains("data-signals=\"{&quot;_order&quot;:null}\""),
        "{}",
        markup.as_str()
    );
}

/// An element can declare a handle and mention a plugin's name in the same
/// breath. Two `data-signals` attributes would leave the browser keeping the
/// first and silently dropping the other.
#[test]
fn a_handle_and_an_inferred_name_share_one_declaration() {
    let gone = signal(false);

    let markup = view! {
        <li id="row" {&gone} data-sortable="post('/reorder', { order: $._order })"></li>
    };

    let rendered = markup.as_str();

    assert_eq!(rendered.matches("data-signals").count(), 1);
    assert!(rendered.contains(&format!("&quot;{}&quot;:false", gone.name())));
    assert!(rendered.contains("&quot;_order&quot;:null"));
}

#[test]
fn raw_javascript_is_the_escape_hatch() {
    let coarse = Js::<bool>::raw("matchMedia('(hover: none)').matches");
    let markup = view! { <div {show(coarse)}></div> };

    assert!(markup.as_str().contains("matchMedia"));
}
