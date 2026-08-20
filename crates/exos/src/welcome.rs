//! What a dev build answers with before there is anything to answer.
//!
//! A binary that links no routes at all is a first run rather than a
//! deployment, and the bare 404 it would otherwise serve says nothing about
//! which of the several possible mistakes was made: the wrong port, a crate the
//! binary never links, or simply not having written a handler yet.
//!
//! It loads the runtime, so with a watcher in front of it the page replaces
//! itself the moment the first route compiles. Which is also why the snippet it
//! shows carries the runtime's script tag: a first page written without one
//! works, and then nothing else ever does.

use crate::{Markup, Page};

/// Where the first steps are written down.
const GUIDE: &str = "https://github.com/MDM23/exos/blob/main/docs/guide.md";

/// The page itself, mounted as a fallback so that whichever path the first run
/// happens to knock on answers with it.
pub(crate) async fn page() -> Page {
    Page(Markup(format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>exos is running</title>
<style>
:root {{ color-scheme: light dark; }}
body {{
    display: grid;
    font: 16px/1.6 system-ui, sans-serif;
    margin: 0;
    min-height: 100vh;
    place-items: center;
}}
main {{ max-width: 44rem; padding: 2rem; }}
h1 {{ font-size: 1.5rem; margin: 0 0 1rem; }}
pre {{
    background: color-mix(in oklab, canvastext 8%, canvas);
    border-radius: 0.5rem;
    overflow-x: auto;
    padding: 1rem;
}}
code {{ font-family: ui-monospace, monospace; }}
.aside {{ font-size: 0.875rem; opacity: 0.6; }}
</style>
<script defer src="{runtime}"></script>
</head>
<body>
<main>
<h1>exos is running</h1>

<p>This binary has no routes. A route is an attribute on a function and
<code>exos::app()</code> mounts every one the binary links, so the first one is
the whole of the setup:</p>

<pre><code>#[exos::get("/")]
async fn home() -&gt; Page {{
    Page(view! {{
        &lt;!DOCTYPE html&gt;
        &lt;html lang="en"&gt;
            &lt;head&gt;&lt;script defer src={{ exos::runtime() }}&gt;&lt;/script&gt;&lt;/head&gt;
            &lt;body&gt;&lt;h1&gt;"Hello"&lt;/h1&gt;&lt;/body&gt;
        &lt;/html&gt;
    }})
}}</code></pre>

<p>The <a href="{GUIDE}">guide</a> walks through the rest.</p>

<p class="aside">Only a debug build with no routes serves this page. Add one and
it replaces itself.</p>
</main>
</body>
</html>
"#,
        runtime = crate::runtime(),
    )))
}
