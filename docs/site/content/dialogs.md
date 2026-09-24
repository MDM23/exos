# Dialogs and panels

A confirmation, an alert, a detail sidebar and an edit form in a box all float
over the page, and none of them needs a component. One question sorts them:
**does the URL own it?**

## A panel is a route

A detail sidebar should survive a reload, open from a shared link and close
with the back button. That is what a URL is for, so the panel is a page: the
list with the panel beside it.

```rust
#[exos::get("/orders")]
async fn orders() -> Page { orders_page(None) }

#[exos::get("/orders/{id}")]
async fn order(Path(id): Path<u32>) -> Page { orders_page(Some(id)) }
```

```rust
view! {
    <a {order::link(row.id).keep_scroll()}>{ &row.name }</a>
}
```

Navigating from `/orders` to `/orders/7` morphs the body, and a morph keyed by
id changes only what differs, so the list keeps its DOM, its focus and its live
fragments while the `<aside>` is inserted beside it. `keep_scroll()` keeps
the scroll position too, where a navigation would otherwise start at the top.
Closing is a link back to `/orders`, or the back button, which puts the reader
where they were.

The server still renders the list for `/orders/7`, and the morph finds nothing
in it to change. That is a known cost, written down in the
[loose ends](https://github.com/MDM23/exos/blob/main/docs/roadmap/loose-ends.md)
until a page is slow because of it.

## A modal on the page

A question that belongs in no history is a `<dialog>`, shown as a modal while a
signal holds true:

```rust
let asking = signal(false);

view! {
    <li id={ format!("post-{id}") } {&asking}>
        <button {on_click(|_| asking.set(true))}>"Archive"</button>

        <dialog {modal(&asking)}>
            <p>"Archive this post?"</p>
            <form method="dialog">
                <button>"Keep it"</button>
                <button {on_click(move |_| archive::post(id))}>"Archive"</button>
            </form>
        </dialog>
    </li>
}
```

The browser does the hard parts: the page behind the dialog is inert, the focus
stays inside it and returns to the button that opened it, and `::backdrop` is
there to style. Escape, the `method="dialog"` form and a click outside under
`closedby="any"` all close it, and closing it any of those ways writes `false`
back to the signal.

The confirmation is not an API. It is the real handler, sitting on the dialog's
button.

## A modal a handler sends

Rendering a dialog into every row of a long list is waste when most rows are
never deleted, and often only the server knows whether there is anything to
ask. So ask the server first, and let it answer with the question:

```rust
#[exos::model]
#[derive(Debug, Default, Deserialize, Serialize)]
struct Removal {
    confirmed: bool,
}

#[exos::post("/posts/{id}/delete")]
async fn delete(
    Path(id): Path<u32>,
    Model(removal): Model<Removal>,
) -> Result<Effect, (StatusCode, Effect)> {
    let invoices = store::invoices_of(id);

    if invoices > 0 && !removal.confirmed {
        return Err((StatusCode::CONFLICT, Effect::patch(confirm(id, invoices))));
    }

    store::delete(id);
    Ok(Effect::remove(format!("#post-{id}")))
}

fn confirm(id: u32, invoices: usize) -> Markup {
    view! {
        <dialog id="confirm" {modal_now()}>
            <p>{ invoices } " invoices refer to this post."</p>
            <form method="dialog">
                <button>"Keep it"</button>
                <button {on_click(move |_| delete::post(id, &Removal { confirmed: true }))}>
                    "Delete anyway"
                </button>
            </form>
        </dialog>
    }
}
```

A post nothing refers to goes at once, and one that something does asks first,
from the one handler. The refusal carries the status it deserves and what the
page should do about it, which the runtime applies
[whatever status carries it](effects).

`modal_now()` shows the dialog as it arrives and removes it once it is closed.
It needs no slot on the page: a patch whose element matches nothing is appended
to the body.

## More than one

Modals stack. One opened from inside another goes on top of it, Escape closes
the top one only, and closing it returns to the one below with the focus where
it was. None of that is exos: it is what `showModal` does, so there is no
z-index to manage.

A sent dialog is named by its id like any other patch. Sending `#confirm` while
one is open morphs over it; sending it after the first was closed opens a fresh
one, since closing removed the old.

A patch that lands on an open modal leaves it open. The server never renders
`open`, so the runtime keeps that attribute for the binding rather than taking
the incoming markup's word for it.
