# Assets

One macro, written where the asset is referenced:

```rust
view! {
    <link rel="stylesheet" href={ exos::asset!("css/app.css") }>
}
```

There is no build script and nothing to register. The file is processed while
your crate compiles, its bytes go into the binary, and the macro returns an
`Asset` naming the hash they were embedded under. Files are served from memory
as `immutable` for a year, which is safe unconditionally because a changed file
is a different URL.

An `Asset` is that name and nothing else, so it is `Copy` and every call site is
a constant. In a view it renders as the URL it is served from, which is why the
line above needs no conversion; where a URL has to be a `String`, ask for one
with `.url()`. The name is not the URL, because the hash is known when your
crate compiles and the [prefix](routes#serving-under-a-prefix) it hangs under is not
known until the program is running.

## The bytes are still there

The file is in the binary, so anything derived from its content can be derived
from the binary rather than kept beside it by hand:

```rust
static BLURRED: LazyLock<String> = LazyLock::new(|| {
    placeholder(exos::asset!("img/cover.png").bytes())
});
```

`bytes()` walks what the binary embedded, so hold the result rather than calling
it per request. A `LazyLock` is the shape for it: the bytes never change, and
neither does anything computed from them.

Nothing is embedded twice to make this work. The second call site for a file
gets the same name as the first and finds the same bytes through it, which is
also true of a file a stylesheet pulled in on its own.

The path is relative to your crate root, and its extension decides everything
else. A `.css` file is bundled through its `@import`s, a `.js` file through its
`import`s, and anything else is embedded byte for byte. The extension also
picks the `Content-Type`; for one the web has no name for, say so:

```rust
exos::asset!("data/blob.xyz", "application/octet-stream")
```

Referencing the same file from several places is free. It is embedded and
registered once, and every call site gets the same handle back.

## What a stylesheet points at

A `url()` in a stylesheet is an asset too, and is treated as one:

```css
/* css/app.css */
body {
  background: url(../img/paper.avif);
}
```

The path is relative to the stylesheet that wrote it, not to the entry point,
so an `@import`ed file can be moved without rewriting the URLs inside it. The
file is embedded and hashed like any other asset, and the URL is rewritten to
the hashed name. Nothing has to be declared anywhere else, and a file that is
not there is a compile error rather than a broken background.

The rewritten URL is relative, which is what makes it work under a
[prefix](routes#serving-under-a-prefix): assets share one directory, so the browser
resolves it against the stylesheet's own URL. A query or a fragment belongs to
the URL rather than to the file, so `url(sprite.svg#pin)` keeps pointing at the
icon it picks out. Anything the crate root cannot reach is left alone: another
host, a `data:` URI, a path from the server root, and `url(#filter)` pointing
into the document.

One place this does not reach is inside a custom property:

```css
:root {
  --splash: url(../img/paper.avif); /* refused */
}
```

A browser keeps a custom property as tokens and resolves the URL where the
`var()` is used, against **the page** rather than against the stylesheet. A page
is any route, so there is no URL that is right everywhere, and no rewriting
fixes it. It is a compile error rather than an image that loads on `/` and
404s on `/files/3`. Put the `url()` in the rule that uses the variable:

```css
.hero {
  background-image: url(../img/paper.avif);
}
```

Release builds minify and debug builds do not, which the macro works out from
the profile it is being compiled under. Minifying during development buys a
slower edit cycle and unreadable stack traces.

Because there is no build script, nothing declares which files to watch. The
macro does it instead: it lists every file the bundler actually opened, so
editing an `@import`ed stylesheet rebuilds and editing an unrelated one does
not. A file that does not exist is a compile error at the call site rather than
a 404 at request time.

## Scripts

Scripts bundle the same way stylesheets do, by following imports:

```js
// js/app.js
import "./charts.js";
import "./tooltips.js";
```

`exos::asset!("js/app.js")` concatenates them in the order the imports give,
which is also the order they depend on each other in, and minifies the result
as one file. A file imported twice is included once, and a cycle is an error.

Only the side-effect form is supported. `import { thing } from "./other.js"`
needs a real bundler with scope hoisting, so it is refused by name at compile
time rather than misread.

The client runtime ships this way too. `exos::runtime()` is the same macro
applied to the runtime and its plugins inside the `exos` crate, so nothing is
copied into your project and there is no version to keep in step.
