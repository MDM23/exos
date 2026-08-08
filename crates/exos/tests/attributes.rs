//! Attribute blocks and the typed handlers that hang on them.

use exos::{Js, Signal, bind, class, on_click, show, signal, signals, text, view, when};

#[test]
fn a_signal_handle_declares_itself_on_its_element() {
    let gone = signal!(gone = false);
    let markup = view! { <li id="row" {&gone}></li> };

    assert_eq!(
        markup.as_str(),
        "<li id=\"row\" data-signals=\"{&quot;gone&quot;:false}\"></li>"
    );
}

#[test]
fn repeated_blocks_merge_into_one_attribute() {
    let open = signal!(open = false);
    let busy = signal!(busy = false);

    let markup = view! {
        <div id="panel" {(&open, &busy)} {class("is-open", open.get())} {class("busy", busy.get())}>
        </div>
    };

    assert_eq!(markup.as_str().matches("data-class").count(), 1);
    assert_eq!(markup.as_str().matches("data-signals").count(), 1);
}

#[test]
fn a_handler_compiles_to_javascript() {
    let gone = signal!(gone = false);

    let markup = view! {
        <button {on_click(|_| gone.set(true))}>"Delete"</button>
    };

    assert_eq!(
        markup.as_str(),
        "<button data-on-click=\"$.gone = true\">Delete</button>"
    );
}

#[test]
fn a_binding_carries_the_signals_type_across() {
    let picked: Signal<Vec<u32>> = Signal::new("picked", Vec::new());
    let markup = view! { <input type="checkbox" value="1" {bind(&picked)}> };

    assert!(markup.as_str().contains("data-bind=\"picked\""));
    assert!(markup.as_str().contains("data-bind-kind=\"number\""));
}

#[test]
fn a_derived_value_reads_the_signal_rather_than_copying_it() {
    let picked: Signal<Vec<u32>> = Signal::new("picked", Vec::new());

    let markup = view! {
        <div id="bar" {&picked} {show(picked.get().any())}>
            <span {text(picked.get().len())}></span>
        </div>
    };

    assert!(
        markup
            .as_str()
            .contains("data-show=\"$.picked.length &gt; 0\"")
    );
    assert!(markup.as_str().contains("data-text=\"$.picked.length\""));
}

#[test]
fn branching_in_the_browser_records_a_conditional() {
    let count: Signal<Vec<u32>> = Signal::new("count", Vec::new());

    let markup = view! {
        <button {on_click(|_| {
            when(count.get().any(), |()| count.clear());
        })}>"Clear"</button>
    };

    assert!(
        markup
            .as_str()
            .contains("data-on-click=\"if ($.count.length &gt; 0) { $.count = [] }\""),
        "{}",
        markup.as_str()
    );
}

#[test]
fn a_signal_used_but_never_declared_is_inferred() {
    // `gone` is referenced in a raw expression attribute and nothing declares
    // it, so the macro reads the name out and declares it as null.
    let markup = view! { <li id="row" data-show="!$._gone"></li> };

    assert!(
        markup
            .as_str()
            .contains("data-signals=\"{&quot;_gone&quot;:null}\""),
        "{}",
        markup.as_str()
    );
}

#[test]
fn an_explicit_starting_value_wins_over_an_inferred_default() {
    let markup = view! {
        <li id="row" data-signals={ signals! { fav: true } } data-show="!$._gone"></li>
    };

    let rendered = markup.as_str();
    assert!(rendered.contains("&quot;fav&quot;:true"));
    assert!(rendered.contains("&quot;_gone&quot;:null"));
}

#[test]
fn raw_javascript_is_the_escape_hatch() {
    let coarse = Js::<bool>::raw("matchMedia('(hover: none)').matches");
    let markup = view! { <div {show(coarse)}></div> };

    assert!(markup.as_str().contains("matchMedia"));
}
