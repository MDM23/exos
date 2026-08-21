# Effects

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
| `title(text)` | retitle the document |
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

## Titles

A title is markup like the rest of the document, so it is composed where every
page already goes through:

```rust
fn title(page: Option<&str>) -> String {
    page.map_or_else(|| String::from("MyApp"), |page| format!("{page} - MyApp"))
}
```

exos has no opinion about an application's name or the separator in front of it,
and needs none: navigation fetches the document and takes the title out of it,
and `Effect::page` does the same with the one it carries.

`Effect::title` is for a title that has to change without a new document, a count
in it or a heading a patch has just rewritten, because a patch is element over
element and the head is never morphed. It sets the whole title, so it goes
through the same function the shell does or the two drift apart. Sent down the
live stream it lands in every subscribed tab whatever page each is showing, which
is the rule `focus` and `scroll` carry too.

## Refusing with an effect

An effect is applied whatever status it arrives with, so a handler can say no
and still say what to do about it:

```rust
#[exos::post("/drafts")]
async fn save(Model(draft): Model<Draft>) -> Result<Effect, (StatusCode, Effect)> {
    if data::<Drafts>().locked(draft.id) {
        return Err((StatusCode::CONFLICT, Effect::patch(locked(draft.id))));
    }

    /* ... */
}
```

A form has a shape of its own for this; see [rules on a
model](models#rules-on-a-model).

Answer with the status the outcome deserves. A refusal that had to be `200` in
order to be heard is a lie told to every log, proxy and test in front of it.

The rule has one other half worth knowing: **HTML arriving with a failure is
left where it is.** Markup on an error is a document *about* the error, and
morphing one in would let a 500 eat the page. So an error carrying an effect is
applied, an error carrying anything else is announced as `exos:error` and
logged, and only a success can patch with plain HTML.

## Streaming a slow answer

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

## Why `Page` is still its own type

`Effect::page` exists, but a `Page` returned from a `GET` is a real HTTP
document. It has to be, because a cold browser, a bookmark or a crawler gets no
JavaScript and nothing else works.

If pages were only effects, every page URL would serve two representations
depending on who asked, which means `Vary` on a custom header and two cache
entries forever. Navigation does not need the effect anyway: the runtime
fetches the document and morphs `<body>`. `Effect::page` is for the narrower
case where an action wants to hand over a new page and save a round-trip.
