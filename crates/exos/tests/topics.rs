//! What names a live fragment, from outside the crate that derives it.
//!
//! A topic is the one name a client and a server agree on without either being
//! told it, so what goes into it is a decision rather than an implementation
//! detail. Two of them are worth asserting from out here, because both are
//! invisible until they are wrong and neither fails loudly.

use exos::Markup;

mod orders {
    #[exos::live]
    pub fn status() -> exos::Markup {
        exos::Markup(String::from("<span>shipped</span>"))
    }
}

mod users {
    #[exos::live]
    pub fn status() -> exos::Markup {
        exos::Markup(String::from("<span>online</span>"))
    }
}

/// The same name in two modules is two fragments.
///
/// Without the module in the hash they are one: one DOM id, one subscription,
/// and a publish of either pushing its markup into the other's element on
/// every tab watching. Nothing errors, which is the reason this is a test
/// rather than a rule in a document.
#[test]
fn two_modules_may_each_have_a_status() {
    assert_ne!(orders::status().topic(), users::status().topic());
    assert_eq!(orders::status().topic(), orders::status().topic());
}

/// And the name stays the readable half of the id, which is what lets a
/// debugger, a log line and a node reading a frame off the bus say which
/// fragment a topic is without a registry to look it up in.
#[test]
fn the_name_is_still_in_the_id() {
    assert!(
        orders::status()
            .topic()
            .as_str()
            .starts_with("live-status-")
    );
    assert!(users::status().topic().as_str().starts_with("live-status-"));

    // And the module is not, since an id ends up in HTML and this half is for
    // reading rather than for deciding anything.
    assert!(!orders::status().topic().as_str().contains("orders"));
}

/// A fragment is named by where it is declared and what it is called with,
/// and by nothing about the caller.
#[test]
fn the_arguments_still_decide_the_rest() {
    #[exos::live]
    fn detail(id: u32) -> Markup {
        let _ = id;
        Markup::default()
    }

    assert_eq!(detail(7).topic(), detail(7).topic());
    assert_ne!(detail(7).topic(), detail(8).topic());
}
