# Highlights

Everyday web problems that exos solves in a few lines, because of a decision
made for some other reason. This is the raw material for the website, so each
entry states the problem, shows the code and names the decision it follows
from. [The rule](../.claude/rules/highlights.md) says when to add one.

## Rename a field and both sides stop compiling

A request body is a `#[model]`, a route generates its own typed caller, and a
template reaches a signal only through its handle. Rename `picked`, change a
route's path or alter a payload type, and every call site is a compile error,
in Rust and in the JavaScript the template produced alike.

```rust
<button {on_click(|_| remove::post(selection))}>"Remove"</button>
```

Follows from: handlers are Rust closures that run at render time and record
what they do, so the browser's code is written by the compiler.

## A row rendered later is already wired

A patch, a live update or a navigation can insert markup at any moment, and
none of it needs initialising. Events are delegated from `document`, and the
bindings that need per-element state are applied by a `MutationObserver` as
elements arrive and disposed of as they leave.

Follows from: the runtime never assumes the DOM stopped changing.

## A background job updates the page with no new machinery

The fragment written for click-to-update is the one a background job
publishes. A handler's reply and the live stream are the same server-sent
events, so there is one parser in the browser and one code path on the server.

```rust
publish(presence(user.id));
```

Follows from: effects are executed over streams, which is the name.

## A slow handler shows progress for free

A handler too slow to answer at once streams its steps as it computes them,
and the browser applies each as it lands. Nothing about the effects changes,
only when they are sent.

Follows from: the same streams as above.

## One effect, three deliveries

"Your bid is in", "you were outbid" and the new price are the same `Effect`
delivered three ways: returned to the tab that asked, sent to one person on
every tab they have open, and published to whoever is watching. The auction
example shows the difference in one gesture: reload, and the price is still
there while the message is not.

Follows from: an effect is a value, and where it goes is a separate decision.

## A confirmation is a refused request

Deleting a post that invoices refer to asks first, and deleting one nothing
refers to does not. One handler, no client state and no dialog rendered in
advance:

```rust
if invoices > 0 && !removal.confirmed {
    return Err((StatusCode::CONFLICT, Effect::patch(confirm(id, invoices))));
}
```

The dialog's button posts again with `confirmed: true`. See
[dialogs and panels](site/content/dialogs.md).

Follows from: an effect is applied whatever status carries it, a patch that
matches nothing is appended to the body, and `<dialog>` is the platform's.

## A detail panel is a route

`/orders/7` is the list with a panel beside it. A reload keeps it, a link
shares it, the back button closes it and puts the reader where they were, and
the morph inserts only the panel.

```rust
<a {order::link(row.id).keep_scroll()}>{ &row.name }</a>
```

Follows from: navigation morphs the body keyed by id instead of replacing it.

## Branching on browser state is a compile error at the exact spot

`gone.get()` is `Js<bool>`, not `bool`, so an `if` over it does not compile. A
restriction found at compile time on one line, rather than a subset of Rust
discovered one surprise at a time.

```rust
when(selection.picked.get().any(), |()| archive::post(selection));
```

Follows from: native control flow deliberately cannot record.

## Optimistic updates correct themselves

A heart paints before the server answers, and the next patch either confirms or
undoes it. There is nothing to roll back, because the speculative write has no
second copy to disagree with the server's.

```rust
on_click(move |_| {
    attr_now("data-favorite", !favourited);
    favorite::post(entry_id, selection);
})
```

Follows from: server state is markup, and a patch always wins.

## A form is not sent twice

A double click, or Enter pressed twice on a slow connection, sends one request.
Nothing is written for it and no control is disabled, so focus stays where it
was. A click or submit on an element still waiting on its own request is
dropped, and other elements on the page stay usable.

Follows from: `aria-busy` is owned by the requests waiting on an element, so
the runtime always knows which elements are mid-request.

## A dropped connection repairs itself

A reconnect fetches the current URL and morphs it in, and every fragment on
screen is right again at once, without the server remembering what the tab
missed.

Follows from: a live fragment is state, so the page can always render it again.

## Mounted anywhere with no configuration

`Router::new().nest("/admin", exos::app().into())` just works. exos works out
where it was mounted on the server, and the browser runtime works it out from
the URL it was loaded from.

Follows from: the application converts into a plain `axum::Router`.
