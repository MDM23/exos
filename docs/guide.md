# The exos guide

Server-rendered HTML with a small client runtime. One binary, `cargo run`, no
bundler and no build step. Everything is ordinary Rust: the only JavaScript you
write is the escape hatch, and you rarely reach for it.

Two rules explain most of the design.

1. **The runtime never assumes the DOM stopped changing.** Events are
   delegated, bindings are applied by a `MutationObserver`. Markup that arrives
   ten minutes after page load is already wired.
2. **Anything crossing to the browser is a typed Rust value**, never a string
   you have to keep in agreement with something elsewhere.

## Hello, exos

```rust
use exos::{Page, view};

#[exos::get("/")]
async fn home() -> Page {
    Page(view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <link rel="stylesheet" href={ exos::asset!("css/app.css") }>
                <script defer src={ exos::runtime() }></script>
            </head>
            <body><h1>"Hello"</h1></body>
        </html>
    })
}

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await?;
    axum::serve(listener, exos::app()).await
}
```

`exos::app()` finds every route in the binary and returns an `axum::Router`, so
exos composes into an axum application rather than replacing one.

## Templates are HTML

`view!` takes real HTML: the tags and attributes you would write in a `.html`
file. Void elements are void (`<br>`, not `<br/>`). A braced block is Rust.

```rust
view! {
    <ul class="files">
        { entries.iter().map(row).collect::<Vec<_>>() }
    </ul>
}
```

It compiles to `String` pushes, so the static parts are string literals in the
binary and nothing is parsed at runtime.

Everything interpolated is escaped. [`Markup`] is the only type that is not,
and it is the only way to emit raw HTML, so "unescaped" is greppable.

### Attributes

`Option<T>` drops the attribute entirely when `None`, because `aria-current=""`
is not the same as no `aria-current`:

```rust
fn current(path: &str, href: &str) -> Option<&'static str> {
    (path == href).then_some("page")
}

view! {
    <a href="/" aria-current={ current(path, "/") }>"Files"</a>
}
```

A bare `bool` renders the *text* `"true"` or `"false"`, which is what you want
for `data-favorite="false"`, so CSS can match both states. For genuine HTML
boolean attributes, `Flag` gives present-or-absent:

```rust
view! {
    <input disabled={ Flag(is_locked) }>
}
```

That distinction is not cosmetic. Writing `.dot[data-online]` in CSS matches
`data-online="false"` too, which is a bug this project shipped once and had to
fix.

## Assets

One macro, written where the asset is referenced:

```rust
view! {
    <link rel="stylesheet" href={ exos::asset!("css/app.css") }>
}
```

There is no build script and nothing to register. The file is processed while
your crate compiles, its bytes go into the binary, and the macro returns a
`&'static str` with the content hash already in it. Files are served from
memory as `immutable` for a year, which is safe unconditionally because a
changed file is a different URL.

The path is relative to your crate root, and its extension decides everything
else. A `.css` file is bundled through its `@import`s, a `.js` file through its
`import`s, and anything else is embedded byte for byte. The extension also
picks the `Content-Type`; for one the web has no name for, say so:

```rust
exos::asset!("data/blob.xyz", "application/octet-stream")
```

Referencing the same file from several places is free. It is embedded and
registered once, and every call site gets the same URL back.

Release builds minify and debug builds do not, which the macro works out from
the profile it is being compiled under. Minifying during development buys a
slower edit cycle and unreadable stack traces.

Because there is no build script, nothing declares which files to watch. The
macro does it instead: it lists every file the bundler actually opened, so
editing an `@import`ed stylesheet rebuilds and editing an unrelated one does
not. A file that does not exist is a compile error at the call site rather than
a 404 at request time.

### Scripts

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

## Application state

Provided once, reachable by type. No `State<T>` threaded through signatures:

```rust
exos::provide(Files::seed());

let files = exos::data::<Files>();          // panics if missing
let maybe = exos::try_data::<Files>();      // Option<Arc<Files>>
```

A missing value is a wiring mistake made once at startup, not a per-request
condition, so `data` panicking and naming the type is the right default.

The type is the key, so wrap distinct things in distinct newtypes.

### Per request

`data` lives for the process. What belongs to the request being served goes in
the request scope, which is the same idea with a shorter lifetime:

```rust
let scope = exos::scope();

scope.set(Principal { id: 7 });
let who = scope.get::<Principal>();     // Option<Arc<Principal>>
```

It is a task-local set by a layer rather than an extractor, for one specific
reason: a `view!` fragment is a plain function, not a handler, so it cannot
extract anything, and making it able to would mean threading a parameter
through every template.

Two rules are enforced rather than documented:

- Outside a request there is no scope, and `exos::scope()` panics rather than
  answering `None`. A background job reading the request is a mistake made
  once, not a case every caller handles, and `None` would quietly render the
  logged-out view of something and then publish it.
- A live fragment never sees it, so `exos::scope()` panics inside one whether
  or not a request is being served. A fragment's arguments are its whole input.

Tests use `exos::with_scope`, which runs a closure in a scope of its own. Being
per task rather than process-global, two tests can hold different scopes at
once, which `provide` cannot do.

## Sessions

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

### Why it stops there

Because the next step needs to await, and the step after that cannot.

Resolving a name to a user is a database call, and it belongs in a handler,
where awaiting one is legal. What comes out goes in the [request
scope](#per-request), and every view underneath reads it synchronously, which
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

### Signing in and out

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
sign out of would otherwise hold a signed-in stream until you closed it.

That takes about a sixth of a second, not the three a browser waits after a
stream it thinks broke. A server-sent stream can name its own reconnection
time, so a stream being ended on purpose says "come straight back" on the way
out, and the greeting on the next one puts the ordinary wait back.

Worth knowing why it closes them rather than correcting them in place.
Re-resolving a live connection would carry it across the boundary rotating
exists to draw: somebody holding a stolen cookie with a stream open would be
*upgraded* to the new identity rather than cut off by it. Ending the stream is
what makes rotation mean what it says.

Pushing those tabs a reload would not work either, and it is worth knowing why
before reaching for it in application code. The push would go out while the
response carrying the new cookie was still being written, so a tab acting on it
at once would re-request with the *old* cookie and come back as whoever it used
to be, with nothing to prompt it again.

### Sessions without a user

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
mints, so a page that only looks costs no cookie.

### What is in the cookie

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
check all leave the response untouched.

### Cross-site requests

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

### Where it cannot be reached

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

## Routes

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

## Client state: signals

A signal is a piece of state in the browser, declared once in Rust:

```rust
let gone = signal(false);   // Signal<bool>
```

Put the handle in an attribute block to declare it. That element becomes its
scope:

```rust
view! {
    <li id={ row_id } {&gone}>/* ... */</li>
}
```

Scoping is lexical with the DOM as the tree: the nearest ancestor that declares
a name wins. A row already needs an `id` for morphing, so a hundred rows can
each declare their own without colliding and you never invent `gone_3`.

### Most signals have no name

The handle is the whole interface. `signal` takes no name because nothing
outside the handle should be spelling one: the store is keyed by a name derived
from the declaration site, and that key is not published. Read it with
`gone.name()` while debugging, never in a template.

A name is a contract, so it exists only where something off the page needs one,
and then it comes from a type rather than a string:

- **`#[model]` fields**, below. These are what an action's body carries and what
  a handler writes with `Effect::set`. Their names are generated too, per model
  and field rather than per call site, so that every `signals()` agrees.
- **Names a plugin owns**, such as the sortable plugin's `_order`. Those are
  written in JavaScript and reach a template as a raw expression, so `view!`
  reads the names out of the expressions in a subtree and declares whatever it
  finds as `null`.

### Models: state that is also a request body

When the same fields are both client state and what an action sends, declare
them once:

```rust
#[exos::model]
#[derive(Debug, Default, Deserialize, Serialize)]
struct Selection {
    picked: Vec<u32>,
}

let selection = Selection::signals();   // selection.picked: Signal<Vec<u32>>
```

The handler takes `Model<Selection>`. Rename `picked` and both sides stop
compiling. `selection.` autocompletes.

Two models with a field of the same name are two signals, which also means
nesting one scope inside another cannot silently shadow.

A model's fields are declared on the **document**, wherever the element holding
the handle happens to sit. That follows from what they are for: a handler
answers with `Effect::set`, the client applies it against the document root,
and a field declared into an element's scope would be a different signal of the
same name. So put the handle wherever the markup it belongs to is, and the
write still lands:

```rust
view! {
    <form {&draft} {on_submit(|_| add::post(draft))}>
        <input {bind(&draft.title)}>
    </form>
}
```

The two kinds render as two attributes, `data-signals` for the element's own
and `data-signals-root` for the document's, so which is which is visible in the
markup rather than being a rule to remember. A `signal` handle is not reachable
from a handler at all, and a debug build says so rather than writing a signal
nothing reads.

### What a model's state lasts for

A model's state is the page's. One model is one value however many elements
declare it, they cannot disagree because the starting value is always the
model's `Default`, and it outlives the element that carried the declaration:
that is what lets a list declare one editing buffer and every row use it.

Two consequences worth knowing before reusing a model type:

- **One model is one instance.** Two comment boxes each with their own draft
  are two model types, not two declarations of one. Element signals are the
  ones that repeat.
- **A navigation re-seeds it.** The document that arrives declares what its
  signals start as, so a page does not inherit what the last one was holding.
  Names the new document does not mention keep their value, and a patch never
  re-seeds anything, since a patch is an update to the page you are on.

Anything `Serialize + Deserialize` can be a signal: `bool`, numbers, `String`,
`Vec<T>`, and nested models.

### The wire is private

A field name never leaves the server. Both the signal and the payload key are
named after the model and the field, so the call above compiles to:

```js
post('/files/archive', {"sc523a195": $.sc523a195, "s70c556ff": $.s70c556ff})
```

An action route is not a public API. Its response is a stream of DOM patches,
so there was never anything useful to integrate against, and the point of
keeping the request private is not to stop anyone: it is that nothing outside
the generated pair can depend on the shape, so the shape stays free to change.
Batching several actions into one request, sending only what changed,
versioning the envelope. A payload someone has written into a script is a
payload that cannot move again.

Two consequences worth stating plainly.

**This is not authorization.** An opaque key is a "do not depend on this"
marker, in the way an unstable ABI is. The keys are sitting in `data-signals`
for anyone who opens the inspector. Every route still authorizes for itself.

**A body written elsewhere keeps `Json`.** The sortable plugin posts
`{ order: $._order }`, which JavaScript writes by hand, so `Reorder` is a plain
`Deserialize` struct behind `Json<Reorder>` and its field names are legible on
purpose. The extractor a handler names is what says which of the two it is.

Where the server already knows a body, `exos::to_wire` builds one, which is
also how a test posts to its own action:

```rust
let body = exos::to_wire(&Selection { picked: vec![], fail: true });
```

Writing `Json<Selection>` on an action still compiles, because a model is an
ordinary `Deserialize` type. It fails at runtime with a missing field, since
the keys that arrive are not the ones serde is looking for.

## Handlers

A handler is a Rust closure. It runs at render time, on the server, and what it
records becomes JavaScript:

```rust
view! {
    <button {on_click(move |_| {
        gone.set(true);
        delete_file::post(entry_id, selection);
    })}>"Delete"</button>
}
```

That renders as:

```html
<button data-on-click="$.s1f4c20a9 = true; post('/files/3/delete', {...})">
```

### How that works

The closure body is ordinary Rust. `gone.set(true)` does not set anything, it
appends a statement to the script being built. `entry_id` is a plain `u32` at
render time and is baked into the JavaScript as a literal.

You get the whole Rust language at render time: `match`, `?`, `data::<T>()`,
string building, all of it runs on the server while rendering. Only values that
must survive to the browser are `Js<T>`.

Native control flow deliberately does not record. `gone.get()` is `Js<bool>`,
not `bool`, so this does not compile:

```rust
if gone.get() { /* ... */ }   // error: expected bool, found Js<bool>
```

That is the intended failure: loud, at compile time, at the exact spot. For
branching in the browser, use `when`:

```rust
when(selection.picked.get().any(), |()| archive::post(selection));
```

Sequencing is free, since consecutive statements record in order, which covers
the large majority of handlers.

### Which events there are

`on_change`, `on_click`, `on_dblclick`, `on_focusout`, `on_input`, `on_keydown`
and `on_submit` are the shorthands; `on(EventType::PointerUp, ..)` names the
rest. The set is closed on purpose. One listener per type sits on `document`,
so an event nobody listens for would be a handler in the DOM that never fires,
and a string would let that be a typo rather than a compile error.

Two of those names are the delegated form rather than the familiar one:
`focusout` bubbles and `blur` does not, and the same goes for `focusin` against
`focus`.

An event of your own is registered from JavaScript and named with `Custom`,
which is a promise that you called `listen` for it:

```js
window.exos.listen("swipe");
```

```rust
on(EventType::Custom("swipe"), |_| archive::post(selection))
```

### The event

```rust
on_change(|event| selection.fail.set(event.target().checked()))
```

`event.target().value()` is `Js<String>` and `.checked()` is `Js<bool>`.
Nothing is read at render time; these build expressions.

### Moving the focus

`focus_now` is the client half of `Effect::focus`, for a control the same click
has just revealed:

```rust
on_dblclick(move |_| {
    editing.set(true);
    focus_now(&format!("#edit-{id}"));
})
```

It waits for the bindings the handler scheduled. While the handler runs, the
field is still hidden, and a hidden element cannot take focus, so focusing it
there would silently do nothing.

## Expressions

`Js<T>` is a client-side expression of type `T`.

| on | methods |
| --- | --- |
| `Js<bool>` | `and`, `not`, `or` |
| numbers | `eq`, `ge`, `gt`, `le`, `lt`, `minus`, `ne`, `plus` |
| `Js<String>` | `contains`, `eq`, `is_empty`, `len`, `ne`, `trim` |
| `Js<Vec<T>>` | `any`, `contains`, `is_empty`, `len` |

`!` is overloadable, so `!gone.get()` works. `==` and `&&` are not, because
`PartialEq::eq` must return `bool`, hence `.eq()` and `.and()`. That is the one
place this API is uglier than the language it mimics, and there is no way
around it.

### The escape hatch

```rust
let coarse = Js::<bool>::raw("matchMedia('(hover: none)').matches");

view! {
    <div {show(coarse)}>"Tap to reveal"</div>
}
```

The type parameter is an assertion the compiler cannot check: you are promising
the expression yields a `bool`. That is the entire cost of the escape hatch,
and it is the only unchecked thing in the API.

## Binding to the DOM

Attribute blocks produce attributes, one value at a time, repeated as needed.

| block | emits |
| --- | --- |
| `{&handle}` | declares signals on this element's scope |
| `{on_click(...)}`, `{on(EventType::PointerDown, ...)}` | a delegated handler |
| `{text(expression)}` | text content |
| `{show(expression)}` | toggles `hidden` |
| `{class(name, expression)}` | one class toggle |
| `{attr(name, expression)}` | one attribute |
| `{prop(name, expression)}` | one property (`value`, `checked`, ...) |
| `{bind(&signal)}` | two-way binding for a form control |
| `{preserve()}` | never morph this element |

Raw `data-*` attributes still work, so none of this is a wall. The sortable
plugin is reached that way, since it is opt-in JavaScript rather than API
surface:

```rust
view! {
    <ul id="file-list" data-sortable="post('/files/reorder', { order: $._order })">
        <li data-sort-item={ entry.id }>
            <span data-drag-handle>"::"</span>
        </li>
    </ul>
}
```

## Calling the server

A route attribute generates a typed caller from the handler's own signature:

```rust
#[exos::post("/files/{id}/favorite")]
async fn favorite(Path(id): Path<u32>, Model(body): Model<Selection>) -> Effect
```

gives you `favorite::post(id, selection)`. The URL, the path parameter type and
the payload type are all checked, so changing the route breaks every call site.

### Optimistic updates

Paint first and let the server correct it:

```rust
on_click(move |_| {
    attr_now("data-favorite", !favourited);
    favorite::post(entry_id, selection);
})
```

Do not mirror server state into a signal. One attribute with two sources of
truth drifts the moment a patch lands: the morph writes the server's value
while the signal still holds the client's. A speculative write has no second
copy, so the next patch corrects it either way.

A binding is the other way round, and the runtime treats it that way: what
`class`, `show`, `text`, `attr` and `prop` write belongs to the binding, so a
morph re-applies them afterwards rather than leaving the incoming markup's
version in place. Server-owned state that a patch should win is markup, and a
speculative write is how you paint it early.

Signals are for state the server does not own: a modal, a draft input, a
selection.

**A speculative hide is not quite that, and the difference bites.** A row that a
click took off the page is client state right up until the server refuses, and
then it has to come back. An element's own
[`signal`](#most-signals-have-no-name) cannot do that, for two reasons that
compound: its declaration is applied when
the element is inserted and never again, so a patch re-renders the row and
leaves the signal holding exactly what it held, and a handler cannot clear it
either, because `Effect::set` only reaches a signal that lives on the document.
The row stays hidden until a reload.

So anything a reply may have to undo goes on a `#[model]` field, where the
handler can reach it:

```rust
#[exos::model]
#[derive(Debug, Default, Deserialize, Serialize)]
struct Selection {
    picked: Vec<u32>,
    going: Vec<u32>,
}

// The click hides the row.
on_click(move |_| {
    selection.going.push(id);
    remove::post(id);
})

// The row is shown again, and the patch decides whether there is one left.
Effect::patch(list()).and_set(&Selection::signals().going, Vec::new())
```

The test for this is whether a *refusal* puts things back, not whether the happy
path looks right. On success the row is gone from the markup anyway, so a hide
that can never be undone looks perfect until the first time the server says no.

### While the server is working

A navigation still outstanding after 150ms draws a bar across the top of the
window, and one that answers sooner draws nothing at all: a bar that flashes at
every trip reads as a rendering fault rather than as progress.

Actions do not draw it. The element a click came from carries `aria-busy` for
the duration, and a disabled button, a spinner or a skeleton says where the
work is happening better than a bar at the top of the window can.

What the bar looks like is CSS. The runtime writes how far along it is and the
rest is custom properties, so a theme sets values rather than rules:

| property | default |
| --- | --- |
| `--exos-progress-color` | `currentColor` |
| `--exos-progress-height` | `2px` |
| `--exos-progress-shadow` | `none` |
| `--exos-progress-z-index` | `9999` |
| `--exos-progress-duration` | `200ms`, how fast it advances |
| `--exos-progress-fade` | `200ms`, how long it takes to go |

The defaults are one rule prepended to `<head>`, so a page rule of the same
specificity wins by coming later and nothing has to reach for `!important`.
The bar itself carries `--exos-progress-value`, a number between 0 and 1, so a
theme that wants something other than a bar still has the figure to hand.

The threshold, and whether there is a bar at all, is markup on the document
root:

```html
<html data-exos-progress="off" data-exos-progress-delay="300">
```

Every round trip is announced on `document` as `exos:busy` and `exos:idle`,
actions included, with `detail.kind` saying which it was. The bar reads that
pair and ignores everything that is not a navigation, and an indicator of your
own reads the same one:

```js
document.addEventListener("exos:busy", (event) => {
    if (event.detail.kind === "request") showSkeleton();
});
```

For work the runtime does not make, a fetch of your own or a long computation,
`window.exos.progress.start()` and `.done()` drive the bar directly, so a
`done()` in a `finally` keeps them balanced.

## Effects

One type describes what the client should do, so adding a variant later changes
no signature:

```rust
#[exos::post("/files/archive")]
async fn archive(Model(selection): Model<Selection>) -> Effect {
    data::<Files>().update(|entries| store::archive(entries, &selection.picked));

    publish(file_list);
    Effect::set(&Selection::signals().picked, Vec::new()).scroll("#file-list")
}
```

| effect | does |
| --- | --- |
| `patch(markup)` | morph HTML into place, keyed by `id` |
| `set(&handle, value)` | write a signal in the client's store |
| `remove(selector)` | delete matching elements |
| `navigate(url)` | client-side navigation |
| `page(markup)` | replace the active page without a second fetch |
| `focus(selector)`, `scroll(selector)` | move the user |
| `reload()`, `none()` | the extremes |

Several steps compose with the `and_` methods, and consecutive `set` calls
become one merge on the wire. The wire format is the same server-sent event
format the live channel uses, so there is one parser rather than two, and the
action path and the live path are the same code.

`set` takes a handle, so the name and the type come from wherever the template
got them and no string has to agree with anything. The client resolves that
name from the document root, which is exactly where a `#[model]` field is
declared and is not where a `signal` handle lives, so model fields are the
writable ones. Handing this a `signal` handle is a mistake the types cannot
catch, and a debug build asserts rather than writing a signal nothing reads.

### Refusing with an effect

An effect is applied whatever status it arrives with, so a handler can say no
and still say what to do about it:

```rust
#[exos::post("/drafts")]
async fn save(Model(draft): Model<Draft>) -> Result<Effect, (StatusCode, Effect)> {
    if draft.title.trim().is_empty() {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Effect::set(&Draft::signals().error, String::from("A title is needed."))
                .focus("#title"),
        ));
    }

    /* ... */
}
```

Answer with the status the outcome deserves. A refusal that had to be `200` in
order to be heard is a lie told to every log, proxy and test in front of it.

The rule has one other half worth knowing: **HTML arriving with a failure is
left where it is.** Markup on an error is a document *about* the error, and
morphing one in would let a 500 eat the page. So an error carrying an effect is
applied, an error carrying anything else is announced as `exos:error` and
logged, and only a success can patch with plain HTML.

### Streaming a slow answer

Where a handler cannot finish before it has something worth saying, answer with
an `EffectStream` and each effect goes out as it is produced:

```rust
#[exos::post("/reports/build")]
async fn build() -> EffectStream<ReceiverStream<Effect>> {
    let (sender, receiver) = tokio::sync::mpsc::channel(8);

    tokio::spawn(async move {
        for stage in plan {
            let done = run(stage).await;
            drop(sender.send(Effect::patch(progress(done))).await);
        }
    });

    EffectStream::new(ReceiverStream::new(receiver))
}
```

`EffectStream::new` takes any `Stream<Item = Effect>` that is `Unpin`, which a
channel receiver, a boxed stream and `tokio_stream::iter` all are. The wrapper
above is `tokio_stream::wrappers::ReceiverStream`, so a crate that streams adds
`tokio-stream` to its own dependencies; exos does not re-export it, because
which stream type you want is yours to pick.

Nothing on the client learns about this. The response is the same server-sent
events a whole `Effect` would be, so it is read frame by frame by the parser
that was already there, and a browser cannot tell the two apart.

Reach for it when the progress belongs to the caller and to nobody else, which
is what makes it neither a fragment nor a directed effect: no topic, no
audience, and it ends when the request does.

### Why `Page` is still its own type

`Effect::page` exists, but a `Page` returned from a `GET` is a real HTTP
document. It has to be, because a cold browser, a bookmark or a crawler gets no
JavaScript and nothing else works.

If pages were only effects, every page URL would serve two representations
depending on who asked, which means `Vary` on a custom header and two cache
entries forever. Navigation does not need the effect anyway: the runtime
fetches the document and morphs `<body>`. `Effect::page` is for the narrower
case where an action wants to hand over a new page and save a round-trip.

## Live fragments

Markup that keeps itself up to date. The server owns the topic, so you never
name one:

```rust
#[exos::live]
fn presence(user: u32) -> Markup {
    let online = data::<Presence>().online(user);
    view! { <span class="dot" data-online={ online }></span> }
}
```

One definition, two uses:

```rust
{ presence(user.id) }          // in a template: renders it
publish(|| presence(user.id)); // anywhere: re-renders and pushes to watchers
```

`publish` takes the call rather than its result, and that is worth a sentence
because it looks like ceremony and is not. A patch is state replacement, so
what has to be true is that the **last** patch a tab receives is the newest one.
Rendering first and publishing second gives that away: a publisher that read the
state first can reach the wire second, and the stale markup then sits on the
screen until that topic is published again, which for the last write of the day
is never. Handing over the render lets exos do both under one lock, and the
signature is what stops the other order being written.

The topic derives from the function name and the argument values, so
`presence(2)` always names the same fragment. One event stream per tab carries
everything, and the client re-derives its visible set from the DOM after every
mutation, so a fragment that scrolls in subscribes and one a patch removed
unsubscribes.

The tab does not name its own connection. The server mints an unguessable id
when the stream opens and sends it as the first event, and the tab reports its
visible set under that. An id a client could choose is one another client could
guess, and naming a connection is what replaces the set of topics it watches.
A reconnect is a new connection with a new id, which is why the client
re-reports its topics every time the stream comes back.

### The invariant

A topic must completely determine its content: the same topic means the same
HTML, for everybody.

Presence satisfies this. Anything depending on the viewer, such as their
session, their permissions or their draft input, does not, and must not be a
live fragment, because two users would share a topic and receive each other's
content.

If content depends on the viewer, either make the viewer part of the topic
(`inbox_count(user_id)`), or answer with an `Effect`, which reaches only the
requester.

### Authorization is structural

The rendered wrapper carries a token only the server can produce:

```html
<exos-live style="display:contents" id="live-presence-cab0087c" data-token="e80842d24f1b7a95c3e0d6118f27ba43">
```

Since the server only renders fragments it decided you may see, being able to
subscribe is the authorization. There is no second permission check to write
and none to forget, and a subscription with a forged token receives nothing.

The token is HMAC-SHA256 truncated to 128 bits, under one configured key:

```rust
exos::keys(exos::Keys::from_secret(std::env::var("EXOS_SECRET")?));
```

Everything signed derives its own subkey from that secret by label, so the live
token and whatever is signed later are independent, and none of them is the
secret. Unconfigured, exos mints a random key per process and says so on
stderr: right for `cargo run`, where a restart drops every stream anyway, and
wrong for a deploy, where two instances would never agree and a restart would
invalidate every token in flight.

What the token proves is that this server rendered this topic, which is not yet
that *this viewer* was served it. Anywhere an id and token escape a page
together, by a screenshot or a shared browser profile, the holder can
subscribe. Binding the tag to a session id is what closes that. The session to
bind it to now exists, and the binding itself does not; see
[the roadmap](roadmap/sessions-and-identity.md).

### Who a stream belongs to

A subscription says what a tab is showing. It cannot say who the tab is, and
some things are addressed to a person rather than to a region of a screen: a
notification, an alert, a nudge meant for one viewer's tabs and nobody else's.

An audience is that other half, and it is an ordinary `Hash` type with a name:

```rust
#[derive(Hash)]
struct Viewer(u32);

impl exos::Audience for Viewer {
    const NAME: &'static str = "viewer";
}
```

exos works out which ones a connection is in once, when the stream opens. An
`EventSource` is opened with an ordinary `GET`, so it arrives carrying the
session cookie, and that is the one place identity can be established without
inventing a second channel:

```rust
exos::identify(async |name| {
    let Some(name) = name else {
        return Ok(Audiences::none());
    };

    Ok(match data::<Sessions>().viewer(&name).await? {
        Some(who) => Audiences::of(&Viewer(who.id)).and(&Team(who.team)),
        None => Audiences::none(),
    })
});
```

`identify` goes once, before serving, and an application that never calls it
has no audiences and pays for none. Answering with a set rather than one value
costs nothing and is the whole difference between addressing a user and
addressing every admin, everyone on a team, or every tab in a workspace.

The name arrives as an `Option` because a stream cannot start a session: its
response headers went out when it opened, so there would be no cookie to set.
What a name exos has never heard of stands for is the application's answer
rather than exos's, which is what lets a logged-out visitor still be addressed
by a queue position or a checkout timer.

Three rules are built in rather than asked for.

**The client can never name an audience.** Topics are client-claimed and
token-proved; audiences are server-derived and live in a set of their own on
the connection. A tab reporting a topic that spells an audience exactly is
still watching a topic and is still nobody.

**A resolver that fails refuses the stream**, rather than opening one with no
audiences, which is the same failure with none of the noise. `EventSource`
retries on its own, so a database that blinked costs a delay rather than a tab.

**Identity is captured when the stream opens and never refreshed.** The runtime
morphs the body on navigation without reopening the stream, so a tab that signs
in as somebody else keeps the audiences it had until the stream drops. That is
why sign-in answers with `Effect::reload()`, which drops the document and the
stream with it.

### Sending to a person

`publish` reaches whoever is watching a fragment. `send` reaches whoever *is*
somebody, whatever they happen to be looking at:

```rust
exos::send(&Viewer(user), &Effect::set(&Toast::signals().message, summary));
```

Every step becomes one event, exactly as a publish sends one, so anything an
`Effect` can say a directed effect can say. It reaches every tab that person
has open, because a toast in six tabs is six toasts and the page is the right
place to decide what to do about that. Sending to somebody who is not connected
is free and silent.

The two mechanisms are worth keeping apart in your head:

|                      | live fragment                        | directed effect                   |
| -------------------- | ------------------------------------ | --------------------------------- |
| addressed by         | topic: a name and its arguments      | audience: who the connection is   |
| content decided by   | the fragment function                | the sender, at the call site      |
| delivered while      | it is on screen                      | a stream is open                  |
| missed by the client | repaired by the next publish or load | lost                              |
| authorized by        | a token proving it was served        | the sender choosing who           |
| use it for           | state that is visible                | events, alerts, per-viewer nudges |

The last two rows are the ones that change how you write code.

**Authorizing is yours.** A subscription is authorized by construction, since a
topic can only be subscribed to by whoever was served it. `send` inverts that:
the server names the recipient, so exos guarantees that only connections whose
identity matched receive it and nothing at all about whether that person should
see the content. That check goes at the call site, in ordinary Rust, where it
can be read.

**A directed effect is an accelerator, never the record.** A patch is state
replacement and the next publish repairs a lost one. A directed effect has no
fragment to re-render from, so a recipient who is offline, whose tab lagged
past the channel's capacity, or who was inside a reconnect gap does not get it,
and none of the three is fixable by trying harder. Persist first, push second:

```rust
fn notify(user: u32, event: &Event) {
    // Durable first. Everything below is an accelerator.
    data::<Notifications>().record(user, event);

    // State, to whichever tabs are showing it.
    publish(|| notification_count(user));

    // The arrival, to the person.
    send(&Viewer(user), &Effect::set(&Toast::signals().message, event.summary()));
}
```

The badge sits in the layout, so it is on every page and every tab of that user
gets it from an ordinary publish. Only the last line needs an audience, and it
needs one because a toast is addressed to a person and has no correct second
delivery.

Asking first is allowed, and is how a push and an email are chosen between:

```rust
if exos::connected(&Viewer(user)) { /* push */ } else { /* email */ }
```

That answer is a hint and not a guarantee, since the last tab can close between
it and whatever is done about it, which is the rule above in another shape.

Order is guaranteed per connection and nowhere else. A connection has one
channel and both calls send under the same lock, so a publish followed by a
send arrives in that order at every tab that gets both. Two connections are
ordered against each other in no way at all.

## What exos does not do

Knowing the edges is more useful than a feature list.

- **No client-side loop.** Server-rendered lists plus morphing cover it. If you
  need a list bound to reactive client data, exos is the wrong tool.
- **No client-side validation rules.** They round-trip, debounced.
- **No client-side routing beyond fetch-and-morph.**
- **No arbitrary Rust in the browser.** Handlers record expressions, and
  anything the combinators cannot say needs `Js::raw`.
- **Expressions are compiled with `new Function`**, so a strict CSP without
  `unsafe-eval` blocks them. A precompiled mode is the answer and does not
  exist yet.

## Reading the examples

[`examples/playlist`](../examples/playlist) is a listening room several
browsers share, and exercises most of the surface in one page: a live fragment
republished by a clock, so the track changes with nobody having asked;
optimistic hearts and removals; selection with a batch action; and drag to
reorder, deciding what plays next.

What it does not have is a switch labelled "simulate a server error". The room
will not remove what it is playing, and that one rule is enough: ask it to,
watch the row go at once and come back, and an optimistic update that turns out
to be wrong has shown you what it does. Run it with `cargo run -p playlist` and
open two tabs.

[`examples/todos`](../examples/todos) is TodoMVC, and covers what the first one
does not: a live fragment per filter, because a topic has to determine its
content and a filtered list is not the list; filters as routes rather than as
client state; editing a row as viewer state from the double click through
escape and blur to the save; and a list that renders nothing at all when it is
empty, decided by an ordinary `if` on the server. Run it with
`cargo run -p todos`, also twice.

[`examples/auction`](../examples/auction) is the one about *who*. A sale room
where the price of a lot is state, published to every tab watching it, and
being outbid is an event, sent to one person on every tab they have open and
on whatever page they happen to be reading. It covers the whole identity
surface: a resolver turning a session name into audiences, a guest who has
claimed no account and is addressable as the name in their cookie anyway,
claiming one as a rotation that carries the lots you were winning across, a
role as an audience covering several people at once, `connected` choosing
between a push and an email, and the auctioneer closing a lot, which tells a
winner who asked for nothing. Run it with `cargo run -p auction`, in two
ordinary tabs and one private window, and then reload: the price is still
there and the message is not.

[`Markup`]: https://docs.rs/exos/latest/exos/struct.Markup.html
