# Sessions

exos names the browser on the other end. What the name means is yours.

```rust
let session = exos::session();

session.id()     // Option<Id>: the name it arrived with, if any
session.start()  // Id: the name, minting one if there is none
session.rotate() // Id: a new name, whatever it was called before
session.end()    // no name, and the cookie goes back
```

That is the whole of it. There is no session store, nothing to configure,
nothing that can fail, and no `exos::viewer`.

## Why it stops there

Because the next step needs to await, and the step after that cannot.

Resolving a name to a user is a database call, and it belongs in a handler,
where awaiting one is legal. What comes out goes in the [request
scope](application-state#per-request), and every view underneath reads it synchronously, which
is the job the scope already exists for:

```rust
#[exos::get("/")]
async fn index() -> Page {
    if let Some(id) = exos::session().id() {
        if let Some(viewer) = data::<Sessions>().viewer(&id).await? {
            exos::scope().set(viewer);
        }
    }

    layout("Home", "/", view! { { header() } { body() } })
}
```

A framework that held the contents instead would have to load them *before* the
handler ran, because a `view!` fragment is a plain function and cannot await.
Every request carrying a cookie would then pay for a session whether or not it
used one. Handing back a name costs nothing and puts the round trip where you
decide it goes.

You also keep everything that comes with owning the table: an index, a join
against `users`, a device column, a last-seen column, your own expiry job, and
your own answer to two requests writing at once. None of that fits through a
trait exos invented.

## Requiring one

Written in a handler, that resolve is written in every handler, and the page
somebody adds next week is behind a session only if they remember to put it
there. Write it once instead:

```rust
async fn guard(request: Request, next: Next) -> Response {
    let viewer = match exos::session().id() {
        Some(id) => data::<Sessions>().viewer(&id).await?,
        None => None,
    };

    match (viewer, request.uri().path() == "/login") {
        (Some(_), true) => Redirect::to("/").into_response(),
        (None, false) => Redirect::to("/login").into_response(),
        (Some(viewer), false) => {
            exos::scope().set(viewer);
            next.run(request).await
        }
        (None, true) => next.run(request).await,
    }
}
```

It is ordinary axum middleware, and it goes on the application:

```rust
axum::serve(listener, exos::app().route_layer(from_fn(guard))).await
```

Which is not the same as putting it on a router. `exos::app()` hands back the
application *before* exos has put anything up around it, and those layers go on
when it is served, so everything added here ends up **inside** them:
`session()`, `locale()` and `scope()` answer in this middleware the way they do
in a handler. Mounted the other way round, above the layer that reads the
cookie, it would have to parse one of its own from the headers and name the
cookie itself.

It also wraps **your routes and nothing else**, since exos's own endpoints are
merged around it later: a [`checked_by` round trip](models#rules-on-a-model) and
a stream answer the client runtime rather than a browser that could follow a
redirect, and a live subscription is already bound to the session it was served
to.

`route_layer` rather than `layer` for the same reason axum draws that line: a
guard answers by itself, and one mounted with `layer` would report every
mistyped URL as somewhere to sign in instead of leaving it a 404.

What the guard puts in the scope is readable from a `view!` fragment, which is a
plain function and can extract nothing. That is the second half of why it runs
where it does.

Order is yours: middleware order carries meaning, and two calls here are two
layers in the order they are written. Anything that needs neither the session
nor the scope can go outside them instead, on the `Router` the application
converts into.

## Signing in and out

```rust
#[exos::post("/session")]
async fn sign_in(Json(form): Json<Credentials>) -> Result<Effect, Error> {
    let user = data::<Users>().authenticate(&form).await?;
    let session = exos::session();
    let sessions = data::<Sessions>();

    // Read before rotating, because rotating replaces it: an anonymous visit
    // may have left a cart under the old name worth moving or dropping.
    let previous = session.id();
    let id = session.rotate();

    sessions.bind(&id, user.id).await?;

    if let Some(previous) = previous {
        sessions.forget(&previous).await?;
    }

    Ok(Effect::reload())
}

#[exos::post("/session/end")]
async fn sign_out() -> Result<Effect, Error> {
    let session = exos::session();

    if let Some(id) = session.id() {
        data::<Sessions>().forget(&id).await?;
    }

    session.end();

    Ok(Effect::reload())
}
```

`rotate` is the session fixation defence, and it is exos's rather than yours
for two reasons: it is a cookie operation, and forgetting it is silent. Call it
at every privilege change. It always answers with a name, because the call site
that rotates is about to need one.

`end` is exos's half of signing out. Deleting what you stored is yours, and you
should, because a name the browser has stopped sending is not a name nobody
else has.

Both answer with a `reload` rather than a patch. Navigation morphs the body
without reopening the `EventSource`, so the tab that signed in would otherwise
keep whatever its stream was opened as, and dropping the document is what
closes that.

**The browser's other tabs are exos's to deal with, and it does.** A rotation
closes every live connection that opened under the name it replaced. Those tabs
made no request, so there is nothing to answer them with, and they need no
handling of their own: `EventSource` reconnects by itself with the cookie the
browser now holds, and the runtime re-fetches the page on a greeting that is
not the first, so each one comes back correctly identified and showing the
right markup. Signing out is the same and matters more, since a tab you did not
sign out of would otherwise hold a signed-in stream until you closed it. With a
[bus](live-fragments#more-than-one-instance) registered it reaches those tabs
wherever they are streaming from, not only the node that took the request.

That takes about a sixth of a second, not the three a browser waits after a
stream it thinks broke. A server-sent stream can name its own reconnection
time, so a stream being ended on purpose says "come straight back" on the way
out, and the greeting on the next one puts the ordinary wait back.

Worth knowing why it closes them rather than correcting them in place.
Re-resolving a live connection would carry it across the boundary rotating
exists to draw: somebody holding a stolen cookie with a stream open would be
*upgraded* to the new identity rather than cut off by it. Ending the stream is
what makes rotation mean what it says.

**Authority taken away without the cookie changing is yours to say.** A viewer
removed from a team or an account disabled leaves the name it was and every
stream under it resolved as whoever it used to be, and exos holds a name and
nothing behind it. `exos::disconnect(&name)` ends those streams the way a
rotation does, wherever they are, and each comes back asking your resolver who
that name is now.

Pushing those tabs a reload would not work either, and it is worth knowing why
before reaching for it in application code. The push would go out while the
response carrying the new cookie was still being written, so a tab acting on it
at once would re-request with the *old* cookie and come back as whoever it used
to be, with nothing to prompt it again.

## Sessions without a user

`start` mints a name for a visitor who has not signed in, which is what an
anonymous cart or a checkout timer wants:

```rust
#[exos::post("/cart/add")]
async fn add(Json(item): Json<Item>) -> Result<Effect, Error> {
    data::<Carts>().add(&exos::session().start(), item).await?;
    /* ... */
}
```

It is idempotent, so a visitor who already has a name keeps it. `id` never
mints, so looking costs no cookie unless something on the page asks for one.
Rendering a [live fragment](live-fragments) does ask: the subscription in its
wrapper is bound to the browser it was served to.

## What is in the cookie

A name, and nothing else:

```text
Set-Cookie: exos=<128 random bits, as hex>;
            HttpOnly; Max-Age=34560000; Path=/; SameSite=Lax; Secure
```

**The `Max-Age` is not the session's lifetime.** It is four hundred days,
which is as long as browsers will accept, deliberately, so that the cookie is
never the thing that ends a session. What a name still means is decided by what
you keep under it, and a cookie naming something you have forgotten is simply
anonymous and costs one lookup. A cookie running out first would sign somebody
out while your record was still perfectly good, and exos has no way to know
when that is.

`Secure` is not configurable. Every browser treats `localhost` and `127.0.0.1`
as a secure context, so `cargo run` is unaffected.

Nothing is sent until something asks for a name, and a browser that already has
the cookie is not sent it again. So an anonymous read, a crawler and a health
check all leave the response untouched, unless the page they read carries a
live fragment.

## Cross-site requests

exos ships no CSRF token, and the reason is that three things already stand
between an attacker's page and a state change.

- **`SameSite=Lax`** on the session cookie means a cross-site `POST` carries no
  session at all.
- **A custom header.** The runtime sends `X-Exos` on every request. A
  cross-origin request cannot set one without a preflight, and a preflight
  needs CORS you never turned on.
- **JSON bodies.** A cross-origin HTML form can only send the three form-safe
  content types, and `Json<T>` refuses all of them.

The gap is real but narrow: a handler accepting a form-encoded body steps
outside all three at once. Do not write one, or bring your own token until exos
has an opinion about them.

## Where it cannot be reached

`session()` is built on the request scope and inherits both of its rules.
Outside a request, asking panics. Inside a live fragment, asking panics: a
fragment renders again from whatever publishes it, so its arguments are its
whole input and the viewer is not one of them.

Under `with_scope` there is no layer, and `session()` mints and rotates exactly
as it would in a request, then goes away with the scope. So a test that needs a
signed-in viewer seeds its own table and the scope, and never has to start a
server:

```rust
exos::with_scope(|| {
    exos::scope().set(Viewer { id: 7, team: 3 });
    assert_eq!(badge().into_string(), "<span class=\"badge\">2</span>");
});
```
