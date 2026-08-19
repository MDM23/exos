# Asynchronous fragments

Letting a live fragment await, and the two things that have to change first.

Status: not built, and unlike the rest of this roadmap it changes code that
exists rather than adding beside it.

It is here because the alternative is not that fragments stay pure. A fragment
is a plain function today, so an application whose data lives in Postgres
rather than in a process-global map has three options: build a synchronous
projection the write path keeps fresh, which is real architecture and not
everybody's to adopt; give up live fragments; or call `block_on` inside the
render. The third is what will happen, and it does not make a page slow, it
parks a runtime worker. Enough of them and the process stops serving anything,
including the streams that make the fragment live in the first place.

A rule enforced by documentation and paid for by whoever ignores it is the
wrong side of that trade.

## What already works

Three things that would have been the hard parts, and are not.

- **`view!` needs no change.** An interpolation expands to
  `::exos::Render::render_to(&(#tokens), &mut __out);` in
  [view.rs](../../crates/exos-macro/src/view.rs), an ordinary statement, so
  `{ lot(7).await }` compiles the moment the enclosing function is async. The
  macro never has to know.
- **The scope and the mask already survive a yield.**
  [scope.rs](../../crates/exos/src/scope.rs) holds its state in a
  `tokio::task_local!`, so `sync_scope` has an async twin in
  `scope(value, future).await`, and a fragment stays masked across an await
  rather than losing the mask at the first one.
- **The toolchain is ready.** The workspace MSRV is 1.88 and `identify`
  already takes an async closure, so an async render closure asks for nothing
  new.

## Stage 1: the recorder stops being thread-local

**The one real blocker, and it is small.** [js.rs](../../crates/exos/src/js.rs)
keeps recording frames in a `thread_local!` stack, pushed and popped around a
synchronous closure, and [attributes.rs](../../crates/exos/src/attributes.rs)
runs it while rendering an attribute block. Two ways that breaks as soon as a
render can yield:

- A task that awaits mid-render can resume on another worker thread, where its
  frame does not exist.
- Two renders interleaving on one thread push and pop each other's frames, so a
  handler recorded by one lands in the other's markup. The result is valid HTML
  wired to the wrong element, which is the worst shape a bug can have.

The fix is the one the scope already uses: a task-local. `record` keeps its
signature for synchronous bodies and gains an async twin.

Worth doing whether or not the rest follows. The invariant it restores, that a
frame belongs to a render, is currently true only by accident of rendering
being synchronous.

## Stage 2: a lock per topic

[publish](../../crates/exos/src/live/stream.rs) takes a global
`std::sync::Mutex` and holds it across the render, which is what makes the
newest patch win. Two things force it to change alongside the render:

- Holding a `std` lock across an await is forbidden, and the workspace's own
  `await_holding_lock` lint says so.
- A global *async* mutex held across a database call is a queue. Every publish
  in the process would wait behind the slowest render in it.

So the lock becomes per topic and asynchronous. [Dimensions](dimensions.md)
already names this as the next thing to build for the fan-out; async turns it
from a refinement into a precondition. Nothing is lost by narrowing it, because
the guarantee was always per topic: the last patch a tab receives being the
newest is a statement about one topic and never was about two.

## Stage 3: an async fragment

`#[exos::live]` on an `async fn` generates the awaiting wrapper. The topic is
computed from the arguments exactly as it is now, and the body runs inside the
masked scope through the async twin from stage 1.

```rust
#[exos::live]
async fn lot(id: LotId) -> Markup {
    let Some(lot) = data::<Db>().lot(id).await else {
        return Markup::default();
    };

    view! { … }
}
```

Synchronous fragments keep working and stay the cheaper default. The macro
generates one shape or the other from what it was handed, and a fragment that
does not await should not be written as though it might.

## Stage 4: publish takes an async render

`publish` becomes async and takes `impl AsyncFn() -> Fragment`, which is what
the fan-out needs anyway, since it calls the render once per watched
combination. A synchronous fragment becomes `publish(async || lot(7)).await`,
one keyword at the call site.

The consequence to state plainly: **`publish` stops being callable from a
synchronous context.** Today it can be called from anywhere, including a plain
thread with no runtime attached. Afterwards a caller needs a handle. Every call
site in the examples is already inside an async function, so this is a question
about what else people would want to publish from rather than an obstacle here.

## Stage 5: load once, render many

Optional, and only worth it if the measurement says so. The fan-out calls the
render once per watched combination, so an async fragment that queries does it
per locale. Splitting the fragment into an async load and a synchronous render
would do the query once and the markup many times:

```rust
#[exos::live(load = fetch_lot)]
fn lot(id: LotId, lot: &Lot) -> Markup { view! { … } }
```

It costs a second function and an attribute argument, which is why it is a
stage rather than the design. The open question [dimensions](dimensions.md)
leaves about reading once and rendering many is the same question, and async is
what makes it worth asking.

## What does not change

- **The topic invariant.** Awaiting does not let a render read anything it
  could not read before: the mask still blocks the request scope, so a
  fragment's arguments and dimensions are still its whole input.
- **`exos::scope()` still panics inside a fragment**, and stage 1 is what keeps
  that true across a yield rather than by luck.
- **The ordering guarantee**, restated per topic, which is where it always
  lived.
- **Synchronous fragments**, which stay the common case.

## New failure modes

- **A render can hang.** With a per-topic lock it hangs that topic and whatever
  is queued behind it, rather than the process. Whether exos should impose a
  timeout is an open question, and my instinct is not before somebody asks.
- **Cancellation, which is safe and worth a test.** A dropped render future
  sends nothing, because the patch is framed and sent only after the render
  returns. That is worth asserting precisely because it is currently true for a
  reason that is about to change.
- **N+1 becomes easy.** A fragment per row, each awaiting a query, republished
  on every change, multiplied by watched dimensions. Nothing prevents it and
  nothing should. It is the cost of the capability, and it is the reason stage 5
  exists.
- **Failure has no policy.** A fragment returns `Markup`, so an error has to be
  absorbed by the body, as [room.rs](../../examples/auction/src/room.rs)
  already does for a missing lot. Whether a fallible render should be able to
  call off its own publish is open.

## What it reopens elsewhere

"A `view!` fragment is a plain function and cannot await" is load-bearing in
three places: [the guide](../guide.md),
[session.rs](../../crates/exos/src/session.rs), and [sessions and
identity](sessions-and-identity.md#stage-3-and-no-store-at-all), where it is
the *deciding argument* for exos holding a session's name and none of its
contents.

The conclusion survives and the argument has to be rewritten. It stops being
"a view cannot await" and becomes "a view that awaits is a database call in the
middle of markup, repeated per view and invisible at the call site". That is
still decisive, and the other four reasons that stage gives, no `type_name`
keys, no blind writes, no expiry policy, no error type, never depended on
awaiting at all.

Whoever builds this owes those three documents an edit, and the sentence to
write is the second one.

## What it costs

- **Async infection.** A view function that interpolates an async fragment
  becomes async, and so does everything between it and the handler. Handlers
  already are, so it stops there, but a library of plain `fn … -> Markup`
  helpers is where it will be felt.
- **A breaking change to `publish`**, which is public API and appears in every
  example.
- **The easiest wrong thing gets easier.** An expensive fragment is impossible
  to write today and one `.await` away afterwards. The capability and the
  footgun are the same feature.
- **Two shapes of fragment to explain**, and a guide that has to say when each
  is right.

## Testing

- **Two renders interleaving on one thread each record their own handlers.**
  That is the stage 1 bug, it fails today with a thread-local, and joining two
  renders on a current-thread runtime is enough to produce it.
- **A cancelled render sends nothing.**
- **Two publishes racing on one topic still leave the newest patch last**, with
  an awaiting render and a per-topic lock, which is the same property the
  synchronous version already claims and the same test shape.
- **Two topics are ordered against each other in no way at all**, asserted so
  that a future global lock cannot be reintroduced as a fix for something else.

## Open questions

- **Whether `publish` keeps a synchronous form** for a caller with no runtime
  handle, or whether that caller is hypothetical.
- **Whether a fallible fragment can call off its own publish**, or keeps
  absorbing errors in the body as it does today.
- **Whether stage 5 earns its second function**, which is a measurement rather
  than an argument.
- **Whether a render timeout is exos's to impose.** A hung fragment is an
  application bug, and a framework that cuts it off at two seconds has invented
  a policy nobody asked for.
