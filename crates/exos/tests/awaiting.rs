//! A live fragment written as an `async fn`, from outside the crate that
//! derives it.
//!
//! It is named the way a synchronous one is and only its render awaits, so
//! naming it stays free and a template puts it on the page by awaiting it.

use exos::Markup;

#[exos::live]
async fn count(user: u32) -> Markup {
    // A real yield, so the test proves the render is awaited rather than
    // ready the moment it is called.
    tokio::task::yield_now().await;

    Markup(format!("<span>{user}</span>"))
}

/// The arguments decide the topic, and awaiting decides nothing about it.
#[test]
fn an_async_fragment_is_named_like_any_other() {
    assert_eq!(count(7).topic(), count(7).topic());
    assert_ne!(count(7).topic(), count(8).topic());
    assert!(count(7).topic().as_str().starts_with("live-count-"));
}

/// Awaited in a template, it is the wrapper around what the render awaited,
/// exactly as a synchronous fragment is put on the page.
#[tokio::test]
async fn awaited_it_renders_inside_the_wrapper() {
    let awaited = count(7).await;
    let topic = format!("data-topic=\"{}\"", count(7).topic().as_str());

    assert!(awaited.as_str().starts_with("<exos-live "));
    assert!(awaited.as_str().contains(&topic));
    assert!(awaited.as_str().ends_with("><span>7</span></exos-live>"));
}
