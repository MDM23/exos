//! The document every page is served in, and the two routes that serve one.

use axum::{extract::Path, http::StatusCode, response::Redirect};
use exos::{Markup, Page, attr, class, on_click, signal, view};

use crate::{
    content,
    markdown::{self, Heading},
    nav::{self, Entry},
    search,
};

#[exos::get("/")]
async fn index() -> Redirect {
    Redirect::to(&show::url(String::from(content::ENTRY)))
}

#[exos::get("/docs/{page}")]
async fn show(Path(slug): Path<String>) -> Result<Page, (StatusCode, Page)> {
    let Some(source) = content::page(&slug).filter(|_| slug != content::NAVIGATION) else {
        return Err((StatusCode::NOT_FOUND, missing()));
    };

    let document = markdown::render(&source);

    Ok(shell(
        &slug,
        &document.title,
        &document.outline,
        document.body,
    ))
}

/// The whole document.
///
/// Everything is server-rendered except the menu, which is client state
/// because nothing off the page has an opinion about whether a sidebar is
/// open on a phone.
pub(crate) fn shell(slug: &str, title: &str, outline: &[Heading], body: Markup) -> Page {
    let menu = signal(false);
    let (previous, next) = nav::neighbours(slug);

    Page(view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8">
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <title>{ format!("{title} - exos") }</title>
                <link rel="stylesheet" href={ exos::asset!("css/docs.css") }>
                <script defer src={ exos::runtime() }></script>
            </head>
            <body {&menu} {class("menu-open", menu.get())}>
                <header class="masthead">
                    <button
                        class="menu"
                        type="button"
                        aria-label="Contents"
                        {attr("aria-expanded", menu.get())}
                        {on_click(|_| menu.toggle())}
                    ><span></span></button>

                    <a class="brand" href={ exos::url("/") }>"exos"</a>
                    { search::field() }
                    <a class="source" href="https://github.com/MDM23/exos">"GitHub"</a>
                </header>

                <div class="shell">
                    // A link closes the menu on the way out, so a phone does
                    // not land on the next page with the sidebar still over it.
                    <nav class="sidebar" aria-label="Documentation" {on_click(|_| menu.set(false))}>
                        { sidebar(slug) }
                    </nav>

                    <main class="article">
                        { body }
                        { steps(previous, next) }
                    </main>

                    { aside(outline) }
                </div>
            </body>
        </html>
    })
}

/// What a slug nothing is filed under gets.
fn missing() -> Page {
    shell(
        "",
        "Not found",
        &[],
        view! {
            <h1>"Not found"</h1>
            <p>"There is no page here. The sidebar has every one there is."</p>
        },
    )
}

/// The groups and their pages, with the one being read marked.
fn sidebar(current: &str) -> Markup {
    view! {
        {
            nav::groups()
                .iter()
                .map(|group| view! {
                    <h2>{ group.title.as_str() }</h2>
                    <ul>
                        {
                            group
                                .pages
                                .iter()
                                .map(|entry| view! {
                                    <li>
                                        <a
                                            href={ show::url(entry.slug.clone()) }
                                            aria-current={ (entry.slug == current).then_some("page") }
                                        >{ entry.title.as_str() }</a>
                                    </li>
                                })
                                .collect::<Vec<_>>()
                        }
                    </ul>
                })
                .collect::<Vec<_>>()
        }
    }
}

/// The outline beside the page, where there is more than one section to show.
fn aside(outline: &[Heading]) -> Markup {
    if outline.len() < 2 {
        return Markup::default();
    }

    view! {
        <aside class="outline" aria-label="On this page">
            <h2>"On this page"</h2>
            <ul>
                {
                    outline
                        .iter()
                        .map(|heading| view! {
                            <li>
                                <a href={ format!("#{}", heading.id) }>{ heading.text.as_str() }</a>
                            </li>
                        })
                        .collect::<Vec<_>>()
                }
            </ul>
        </aside>
    }
}

/// Where to go next, and where this came from.
fn steps(previous: Option<Entry>, next: Option<Entry>) -> Markup {
    if previous.is_none() && next.is_none() {
        return Markup::default();
    }

    view! {
        <nav class="steps" aria-label="Pagination">
            { previous.map(|entry| step(&entry, "Previous", "back")) }
            { next.map(|entry| step(&entry, "Next", "forward")) }
        </nav>
    }
}

/// One of those two links.
fn step(entry: &Entry, label: &str, direction: &str) -> Markup {
    view! {
        <a class="step" data-direction={ direction } href={ show::url(entry.slug.clone()) }>
            <span class="label">{ label }</span>
            <span class="title">{ entry.title.as_str() }</span>
        </a>
    }
}

#[cfg(test)]
mod tests {
    use crate::tests::{get, status};
    use axum::http::StatusCode;

    #[tokio::test]
    async fn a_page_is_served_with_its_own_title_and_the_whole_sidebar() {
        let html = get("/docs/routes").await;

        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(html.contains("<title>Routes - exos</title>"));
        assert!(html.contains("<h1 id=\"routes\">Routes"));
        assert!(html.contains("Getting started"));
    }

    #[tokio::test]
    async fn the_page_being_read_is_the_only_one_marked_in_the_sidebar() {
        let html = get("/docs/effects").await;

        assert_eq!(html.matches("aria-current=\"page\"").count(), 1);
        assert!(html.contains("<a href=\"/docs/effects\" aria-current=\"page\">Effects</a>"));
    }

    /// The sidebar is the list of pages, so the file it is written in is not
    /// one of them.
    #[tokio::test]
    async fn the_navigation_file_is_not_a_page() {
        assert_eq!(status("/docs/navigation").await, StatusCode::NOT_FOUND);
        assert_eq!(status("/docs/nothing-here").await, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn the_root_leads_to_the_first_page() {
        assert_eq!(status("/").await, StatusCode::SEE_OTHER);
    }

    #[tokio::test]
    async fn a_cross_reference_between_two_pages_points_at_the_other_one() {
        let html = get("/docs/assets").await;

        assert!(html.contains("href=\"/docs/routes#serving-under-a-prefix\""));
    }

    /// One stylesheet and one script, like every other exos application.
    #[tokio::test]
    async fn the_site_ships_no_bundle_of_its_own() {
        let html = get("/docs/templates").await;

        assert_eq!(html.matches("rel=\"stylesheet\"").count(), 1);
        assert_eq!(html.matches("<script").count(), 1);
    }
}
