# Calling the server

A route attribute generates a typed caller from the handler's own signature:

```rust
#[exos::post("/files/{id}/favorite")]
async fn favorite(Path(id): Path<u32>, Model(body): Model<Selection>) -> Effect
```

gives you `favorite::post(id, selection)`. The URL, the path parameter type and
the payload type are all checked, so changing the route breaks every call site.

## Not on every keystroke

A handler on `input` runs per keystroke, which is right for a signal write and
wrong for anything that leaves the machine. `debounce` holds the body back
until the typing stops:

```rust
on_input(|_| debounce(300, || search::post(&filter)))
```

**The key is where it is written, resolved against the DOM.** It is generated
from the call site exactly as [`signal`](signals) names itself, and looked up
through the same scopes, so a helper called once per row gives every row its own
timer. Typing in one row cannot cancel what another row was about to save, and
nothing has to invent a name.

**A key also carries last-response-wins.** Once a newer call has gone out under
it, an older reply is dropped rather than applied. Debouncing alone does not
give you that: two requests can still be in flight together on a slow
connection, and the older one landing last paints the answer for a prefix of
what is now in the box. That is a bug nobody can reproduce, so it is not left to
the application.

Nothing else about a search field is new. The query is a model field, the
handler patches a fragment, and `aria-busy` on the element already says the work
is happening.

## Optimistic updates

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
[`signal`](signals#most-signals-have-no-name) cannot do that, for two reasons that
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

## While the server is working

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

Every round trip a call makes is announced on `document` as `exos:busy` and
`exos:idle`, actions included, with `detail.kind` saying which it was. The one
exception is a [`checked_by`](models#rules-on-a-model) field asking about what
is being typed into it: that says `aria-busy` on the control and nothing else,
because a page-wide indicator per keystroke is not one. The bar reads the pair
and ignores everything that is not a navigation, and an indicator of your own
reads the same one:

```js
document.addEventListener("exos:busy", (event) => {
    if (event.detail.kind === "request") showSkeleton();
});
```

For work the runtime does not make, a fetch of your own or a long computation,
`window.exos.progress.start()` and `.done()` drive the bar directly, so a
`done()` in a `finally` keeps them balanced.
