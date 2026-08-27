# Routes

```rust
#[exos::get("/files")]
async fn index() -> Page { /* ... */ }

#[exos::post("/files/{id}/favorite")]
async fn favorite(Path(id): Path<u32>, Model(body): Model<Selection>) -> Effect {
    /* ... */
}
```

The attribute is the registration. There is no second list, and no way to add a
handler and forget to mount it.

Registration is collected at link time, so routes in a crate that nothing links
do not exist. That is irrelevant in a binary and worth knowing if you split
routes into a library.

## Serving under a prefix

Nest it, and nothing else. `nest` takes a router, which is what the application
converts into:

```rust
Router::new().nest("/admin", exos::app().into())
```

`nest` does the routing. What it cannot do is fix the URLs written *into* a
page, because a URL in HTML is absolute and absolute needs the prefix. So exos
works the prefix out: `nest` rewrites the path it forwards, axum records what
arrived, and the difference between the two is the mount point.

```text
original  /admin/_exos/live
current         /_exos/live
prefix    /admin
```

It is read on the first request and kept for the process, because the answer
belongs to the process rather than to the request: `publish` renders fragments
where no request exists, and they carry asset URLs like any other markup.

**One rule covers everything.** A route attribute says the path the *server*
sees; the prefix is what the browser adds in front. exos puts it back on
everything it writes for you:

| written by | example | prefixed |
| --- | --- | --- |
| `asset!`, `runtime()` | `/admin/_exos/app-9f2c.css` | yes |
| the live endpoints | `/admin/_exos/live` | yes |
| a typed route caller | `favorite::post(3)` | yes |
| a route's own URL | `favorite::url(3)` | yes |
| a link you write | `<a href="/files">` | **no** |

The last row is yours, and there are two ways to write it. For a route, the
route attribute already generated one:

```rust
#[exos::get("/files/{id}")]
async fn show(Path(id): Path<u32>) -> Page { /* ... */ }

view! { <a href={ show::url(3) }>"Open"</a> }
```

That is built from the same path and the same `Path<T>` the caller is, so
renaming the route or changing its parameter type breaks every link to it rather
than leaving one that 404s. It is the same guarantee `show::get(..)` gives an
action, and it is the one to reach for.

**`link` writes the whole anchor.** It is that URL, plus `aria-current="page"`
where the URL is the page being rendered:

```rust
view! { <a {show::link(3)}>"Open"</a> }
```

Which page that is comes from the request, so a sidebar marks where the reader
is without every template that draws a link being handed the answer. Where there
is no page to be on, nothing is marked: a background job has no request, and a
[live fragment](live-fragments) renders again for every viewer a publish
reaches, none of whom is promised to be on the page that triggered it.

For anything that is not a route, `exos::url` joins a path to the base:

```rust
view! { <a href={ exos::url("/files") }>"Files"</a> }
```

At the root both add nothing, which is exactly why they are easy to forget until
the day something is mounted somewhere.

`<base href={ format!("{}/", exos::base_path()) }>` is the obvious alternative
and is worth knowing the shape of before reaching for it. It does not do what it
looks like it does:

| written | with `<base href="/admin/">` |
| --- | --- |
| `href="files"` | `/admin/files`, from any page |
| `href="/files"` | `/files`, because a root-absolute URL ignores the base |
| `href="#section"` | `/admin/#section`, which leaves the page |

So it only helps if every in-app URL is relative, one leading slash breaks
silently and only under a prefix, and in-page anchors have to be written out in
full. That is a convention for the whole application to buy into, not a tag to
add, which is why exos prefixes what it writes instead.

**The client is not told either.** The runtime is itself an asset served under
the prefix, so it reads the base out of its own script URL. That is
self-verifying: if the script is running, the URL it came from was right.

### When it has to be told

It is the outermost axum `Router` that records the arriving URI, so anything
that rewrites the path in front of one is invisible from here. A reverse proxy
serving you at `/admin` while forwarding `/` is the ordinary case: the server
never receives that prefix, so no amount of looking will find it.

```rust
exos::app().base("/admin")
```

Said explicitly it wins and discovery never runs. It goes on the application,
once: saying the same place again says nothing new, and a *different* one
panics, whether the first was another application saying it or a request that
had already answered the question.

## Redirecting

A route answers with whatever is a response, so a page that is sometimes not one
widens its return type:

```rust
#[exos::get("/inbox")]
async fn inbox() -> Result<Page, Redirect> {
    let Some(id) = exos::session().id() else {
        return Err(Redirect::to(&sign_in::url()));
    };

    /* ... */
}
```

`Redirect` is axum's. Its target is a URL the application writes, which makes it
the last row of the table above: build it from the route's own `url()` and it
carries the prefix, write `/sign-in` and it does not.

The client needs to know nothing. Navigation is a fetch for the document, so the
browser follows the redirect on its own, and what goes into history is the URL
it landed on rather than the one asked for.

**An action redirects with an effect instead.** A `303` answering a `post` is
followed by the same fetch that sent it, and the document that comes back is
handed to a patch, which morphs whatever ids it matches and appends the rest.
`Effect::navigate(sign_in::url())` says the same thing to the client that asked.
