# Loose ends

The small things. Each one is known, each one is independent, and none of them
is big enough to argue with itself for a document.

Status: open unless an entry says otherwise. The two design documents,
[sessions and identity](sessions-and-identity.md) and [directed
effects](directed-effects.md), describe systems that are unfinished: the first
is built as far as the session, the second not at all. This one describes work
on the parts that are finished, which is why the entries are short: the thinking
is already done and what is left is the doing.

Ordered by what would break if it stayed undone, not by effort. A finished entry
keeps its place rather than moving or leaving, because what it decided is worth
as much as what it fixed.

## The reconnect gap is repaired from the client

**Done**, in [runtime.js](../../crates/exos/js/runtime.js). `EventSource`
reconnects on its own, the server has forgotten the connection, and the client
re-subscribes. Anything published in between was gone, and a fragment that
changed during the gap stayed wrong until the next publish, which might be
never.

The server cannot repair that alone. A topic is a hash of a name and its
arguments, and nothing can re-invoke the function from it, which is the same
wall [directed effects](directed-effects.md) hits in its stage 5. So the repair
is the client's: a greeting that is not the first means the connection behind it
was dropped, and one fetch of the current URL brings back everything
state-backed on the page at once.

Four rules came out of writing it, and each is pinned by a test.

- **It morphs without re-seeding.** A navigation is a different page saying what
  its signals start as; a repair is the same page arriving again. Seeding it
  would empty the field somebody is typing into every time their connection
  hiccuped.
- **The first greeting repairs nothing.** The document arrived a moment ago, so
  there is no gap behind it.
- **A repair that lands after the tab moved on is dropped.** The URL is captured
  before the fetch and checked after it. A navigation fetches the page it lands
  on, so it has already repaired whatever the gap cost, and laying the page the
  tab left over the page it is on would be the worse bug of the two.
- **Only a page repairs a page.** A response that is not `ok` is left where it
  is, and a failed fetch is logged rather than falling back to a full load the
  way a navigation does. Neither a 404 nor an unsteady network should turn a
  hiccup into a lost document.

## A refusal could not be heard

**Done**, in [runtime.js](../../crates/exos/js/runtime.js). The client read a
response body only when the status was 2xx, so a handler could either be honest
about a refusal or be heard, never both. Everything a rejected action wanted to
say, which field was wrong, which signal to put back, where to move the caret,
is an `Effect` already, and all of it came back as a console line and a page
that did not change.

An effect is now applied whatever status carries it. Two rules fell out, and the
second is the one that keeps this from being reckless.

- **Only an effect.** HTML arriving with a failure is a document *about* the
  failure, so it is left where it is; morphing one in would let a 500 eat the
  page. A failure carrying neither is announced as `exos:error` and logged,
  which is what that path already did.
- **An effect means handled.** A response that says what to do is not also
  reported as an unhandled error, or every validation failure would be noise in
  a console somebody is trying to read.

This is the piece the README's form-validation gap was waiting on, and it was
the whole of the client's half. What is still missing is a way to express rules,
which is a design rather than a chore.

## An application could not be nested

**Done**, in [base.rs](../../crates/exos/src/base.rs). `Router::nest` routes
exos wherever it is told, so nesting looked like it worked and did not: `asset!`
baked a root-absolute literal, so the script tag 404ed, the runtime never
loaded, and every typed route caller posted into nothing. The stylesheet was
gone before any of that, which is the part no client could have repaired.

The answer went through two shapes and the first one is worth recording, because
it looked reasonable.

**Rejected: `exos::base("/admin")` moving the routes too.** One call, `app()`
mounting under it, deterministic, no discovery. It duplicates what `nest`
already does, so doing both silently gives `/admin/admin/...`, and it makes the
idiomatic axum composition the wrong move in a project whose pitch is that it
composes into an axum application. A design that quietly forbids `nest` is
fighting the tool.

**Built: the prefix is discovered.** axum records the arriving URI before
nesting rewrites it, so the mount point is `original` minus `current`, read on
the first request and kept for the process. Four things came out of it.

- **The feature travels.** exos asks axum for `original-uri`, and since the
  `#[cfg]` is inside axum's own source and cargo compiles one axum for the union
  of every requested feature, a downstream crate that builds axum with
  `default-features = false` still gets it. It could not be otherwise: exos
  hands back an `axum::Router`, so nesting only typechecks against the same
  axum.
- **It is kept, not read per request.** `publish` renders fragments where no
  request exists, and they carry asset URLs like anything else.
- **The mount point itself is its own case.** A router nested at `/admin`
  forwards a request for `/admin` as `/`, and `/admin` does not end with `/`, so
  suffix stripping alone misses it. A path that does not line up at all answers
  nothing rather than guessing, since the guess would be kept for good.
- **Route callers had to move with it.** A route attribute says the path the
  server sees and a browser has to ask for the one it is served at, so
  `exos::call` prefixes. That is the same rule for nesting and for a
  prefix-stripping proxy, which is the sign it is the right one.

`exos::base` survives for the case discovery cannot see: it is the outermost
axum `Router` that records the URI, so a reverse proxy stripping `/admin` is
invisible from inside. Said explicitly it wins and discovery never runs.

What exos still does not touch is a URL the application writes: a link, a
redirect, an `Effect::navigate` target. Two things are there for those, and
which one to reach for is the interesting part.

`exos::url("/files")` joins a path to the base, and is the answer for a path
that is not a route's. A tuple form, `url(("files", id))`, was considered and
rejected: it is a worse `format!`, since the shape of the URL stops being
legible at the call site, and it needs an implementation per arity to buy
nothing the compiler can check.

What it was reaching for already existed one level up. The route attribute knows
the path template and the `Path<T>` types, because it generates the caller from
them, so it now generates `show::url(3)` as well. That is the typed answer:
renaming a route or changing a parameter breaks every link to it at compile
time, which is the guarantee the README already advertises for actions. It also
put the URL in one place rather than two, since the caller now asks for it
instead of building its own, and `exos::call` stopped prefixing.

`asset!` stopped handing back a `&'static str` here, and
`exos_build::Built::url` is gone: what a URL starts with is a runtime fact and a
build-time crate had no business claiming to know it. What it hands back now is
an `Asset`, which renders as that URL wherever a page needs one.

## The stream carries all eight steps

**Done.** [runtime.js](../../crates/exos/js/runtime.js) registered listeners for
`navigate`, `page`, `patch`, `remove` and `signals` only, while `apply` already
knew how to run all eight, so a `focus`, a `reload` or a `scroll` pushed down
the stream arrived and hit nothing.

That was invisible while the stream only carried patches and would have become
arbitrary the moment it carries effects, which is what [directed
effects](directed-effects.md) makes it do. `focus` and `scroll` from a
background job are rude, but they are rude in exactly the way an application
chooses, and a framework that refuses to deliver them is a surprise that shows
up as silence.

## A slow handler had to finish before it said anything

**Done**, in [streaming.rs](../../crates/exos/src/effect/streaming.rs).
[effect.rs](../../crates/exos/src/effect.rs) claimed a slow handler could stream
its steps as it computed them, and the wire format did allow it while nothing in
the API did: `Effect::into_response` built the whole body first.

`EffectStream` is the missing half, and it is small precisely because the format
was right. It frames through the same `Sse` the live stream uses rather than
writing bytes of its own, so the two stay identical by construction rather than
by two pieces of code agreeing, which is how they drifted the last time. The
client needed nothing: `consume` already read frames off a response as they
landed.

It takes a stream of `Effect` rather than of `Step`, because an effect is the
unit a handler thinks in, and it flattens them so an effect's own steps stay
together and in order. An empty effect is skipped rather than ending the
response, since a handler with nothing to say yet is still running.

## A stale patch could arrive last

**Done**, in [stream.rs](../../crates/exos/src/live/stream.rs). `publish` took a
rendered `&Fragment`, so a caller rendered and then asked to send, and two of
those racing left a tab wrong forever: the publisher that read the state first
could reach the registry lock second, so the older markup landed last and stayed
until that topic was published again, which for the last write of the day is
never.

It takes a fragment that has not rendered, `publish(lot(id, role))`, and holds a
lock across the read and the send. Three things came out of writing it.

- **The type is the fix.** A `Fragment` carries the render rather than markup,
  so there is no rendered one to hand over and the order that was wrong is the
  order that no longer compiles. Documenting it would have been a rule to
  remember.
- **It is a lock of its own, not the registry's.** Arbitrary rendering must
  never run while the registry is held, or a fragment that panics would poison
  it and every publish afterwards would panic too. The ordering lock guards
  `()`, so poisoning is recovered rather than propagated and a panicking
  fragment costs its own publish and nothing else.
- **The test is white-box, deliberately.** A behavioural test was written first,
  two threads counting one state up and asserting the last patch carried the
  final count. It passed against the broken implementation on every run, because
  a render that fast never loses the race, and a test that passes against the
  bug it names is worse than no test. What is checked instead is that the render
  observes the lock held, which fails the moment the mechanism is removed.

## A busy marker is taken off mid-request

**Done**, in [runtime.js](../../crates/exos/js/runtime.js). `aria-busy` was set
on the element an action was recorded on before the fetch and removed in a
`finally`, and nothing else on the page knew it was there. `syncAttributes`
takes off whatever the incoming markup does not carry, and the request path
writes this rather than a binding, so it was not in the set `reapply` puts back:
a patch landing over that element while its own request was in flight took the
marker with it, the spinner went, the button looked ready again, and the
`finally` afterwards removed an attribute that was already gone. It happened
most readily where it was worst, on a page whose live fragments publish while
somebody is clicking.

The marker is owned now, the way a binding's writes are, and the ownership is
held off the DOM rather than in it: a `WeakMap` of how many requests are waiting
on each element, which `reapply` reads. Three things came out of writing it.

- **Counted rather than flagged.** A debounced field and the form around it can
  both be in flight on one element, and the first to answer was taking the
  marker off the other. Owning it without counting would have left that half of
  the bug in place, one await further along.
- **What is waiting owns it, not what is on the element.** `reapply` only puts
  the attribute back, so markup that arrives carrying `aria-busy` still stands
  where exos is waiting for nothing, and a marker cannot outlive the request
  that set it. That is the difference from `data-token`, which survives a morph
  by being skipped: a token is something a publish is *silent* about, and a busy
  marker on an element a patch rewrote is a real disagreement between two
  writers.
- **One attribute, not a rule about attributes.** The client's writes do not
  generally survive a patch, and must not start to: a speculative `attr_now`
  write has deliberately no second copy, and [optimistic
  updates](../site/content/calling-the-server.md#optimistic-updates) rests on
  the patch being the thing that corrects it. What earns this one its place is
  that there is something to name as the owner for exactly as long as it is
  true.

## Nothing disables a busy control

`aria-busy` is advisory. It says work is happening and prevents none of it, so a
second click during a request sends a second request, and a form slow enough to
be doubted is a form that gets submitted twice.

Most of the answer is already an application's to write, and that is worth
recording before anybody builds machinery for it. The attribute is set
synchronously, before the `await`, so this blocks the second click rather than
racing it:

```css
[aria-busy="true"] {
    pointer-events: none;
}
```

Two things it does not cover. The keyboard goes straight past it: a focused
button still activates with Enter or Space, and Enter in a text field still
submits. And the element marked busy is the one carrying the handler, so for
`<form {on_submit(...)}>` that rule freezes every field in the form rather than
the button, which is either exactly right or far too much depending on how long
the request takes.

Whether exos should write `disabled` itself is the open question, and what keeps
it open is that the obvious version is wrong in both directions. A framework
that disables a control for the length of a request also disables it where a
second click was wanted, and a disabled element loses focus, which hands the
caret back to the body in the middle of somebody's typing. An application that
wants it today writes `prop("disabled", ...)` over a model field the handler
clears, which is a few lines and keeps the policy where the policy belongs.

## Publishing scans every connection

`publish` takes a `Mutex` over the whole registry and walks it. At presence
volumes that is invisible, and it is the wrong shape for one send per
notification against thousands of connections.

The fix is an index from key to connection ids, maintained on subscribe and on
close, so both `publish` and `send` become a lookup. Two constraints on whoever
writes it: `await_holding_lock` is on, so the send path stays synchronous, which
`broadcast::Sender::send` allows; and the index has to be dropped in `close` or
it outlives the connections it names.

`send` now exists and walks the registry the same way, so this has two callers
rather than one and a half. It also needs two indexes rather than one, because
topics and audiences are deliberately separate sets and merging them at the
index would give back exactly the distinction that keeps a client from claiming
an audience.

The second thing that was here, a publish serializing against every other
publish rather than against its own topic, is **done** and turned out not to be
the same job after all. It hung off the fragment rather than off the registry: a
publish could not name its topic before rendering it, so there was no key to
lock on, and no bookkeeping about who is watching would have supplied one.
[`Fragment`](../../crates/exos/src/live.rs) now carries its render rather than
its markup, which puts the name in front of the work, and
[stream.rs](../../crates/exos/src/live/stream.rs) keeps one lock per topic for
as long as somebody is publishing that topic.

So the walk is still a walk, and it is still not worth an index before something
feels it. Worth knowing where it is when something does.

## A topic was named differently by every build

**Done**, in [fnv.rs](../../crates/exos/src/fnv.rs). `Topic::new` hashed with
`DefaultHasher`, whose algorithm std explicitly declines to promise across
releases. A topic is the one name a client and a server agree on without either
being told it, so two binaries of one program built with different compilers
disagree about what a fragment is called, and the fragment then stops updating
for the life of that document with no error anywhere. A rolling deploy is enough
to produce it, and it reads as a network glitch.

FNV-1a, written down in the crate, with the integer writes forced little-endian
and the pointer-sized ones widened, so the answer depends on neither the
compiler nor the machine. [`signal`](../../crates/exos/src/signal.rs) was
already doing this by hand for the same reason and now shares it, byte for byte.

The test is a golden value rather than a round trip, because a round trip passes
against a hasher that drifts, which is the failure this exists to stop. What is
still not promised is that a value keeps its name when its own `Hash`
implementation changes: adding a field to a fragment argument renames every
topic it appears in, which is a deploy that has to drop its documents, and is
the application's to know about.

## Morphing stays in-house

**Decided.** [idiomorph](https://github.com/bigskysoftware/idiomorph) was
measured against the hand-rolled morph and rejected. Recorded here so the
question is not reopened without new information.

The arithmetic, against the release bundle rather than the source:

| | raw | gzipped |
| --- | --- | --- |
| the runtime today | 21,617 B | 7,046 B |
| with `idiomorph.min.js` | 31,320 B | 10,006 B |

Net of the ~3 KB of source it would let us delete, that is about **+7.5 KB raw
and +2.3 KB gzipped, a third larger**. Worth knowing about the number: only the
release build minifies, and the bundler in
[javascript.rs](../../crates/exos-build/src/javascript.rs) uses the `minifier`
crate, which strips whitespace without mangling names. So the unminified
`dist/idiomorph.js` at 49,659 B is not an option, and the vendored file would
have to be the minified one, which is a file nobody can read.

The size is not the main argument. The feature fit is:

- **Unreachable.** exos morphs the body only and sets `document.title` by hand,
  so the whole head-handling surface (`style`, `block`, `ignore`,
  `shouldPreserve`, `shouldReAppend`, `shouldRemove`, `afterHeadMorphed`) can
  never run. `morphStyle: "innerHTML"` cannot either, since a patch is always
  element over element.
- **Conflicting.** idiomorph preserves input state by *focus*
  (`ignoreActive`, `ignoreActiveValue`, `restoreFocus`). exos preserves by
  *binding ownership*, and deliberately lets the server's word win for anything
  no binding owns. Ours is the more precise rule, so theirs would have to be
  switched off and ours reimplemented in `beforeAttributeUpdated`.
- **Relocated, not deleted.** Only `morphChildren`'s reorder loop really goes.
  `data-preserve`, the ownership rule and the `reapply` call all survive as
  callbacks, and the bind/unbind wiring becomes indirect exactly where the
  README says every bug that got past review lived.

What idiomorph genuinely does better is id-set matching across descendants,
where exos indexes only direct children. That would reopen this, but the trigger
is a real morphing bug the flat keyed map cannot fix, not a general preference
for a library. exos's server puts ids on fragments and the guide asks for it, so
the design already steers around most of that ground.

The morph's invariants are now written down as tests instead, ten of them, each
checked by mutating the implementation to confirm it fails. The licence was
never the obstacle: idiomorph is Zero-Clause BSD.

## The client's live path was barely tested

**Done.** jsdom implements neither `EventSource` nor `fetch`, so until the
harness grew both, the whole of `openStream` and `syncSubscriptions` ran in no
test at all. The handshake, the 410 path, the step vocabulary and all three
rules of the reconnect repair were covered first. The two that were left are the
subject of this entry, and both are now checked in
[runtime.test.js](../../crates/exos/js/tests/runtime.test.js).

**The topic model, from the end the client owns.** The crate's own tests assert
that a frame reaches the connections watching its key and no others. What the
client contributes is the set those keys come from, and that it tracks the page
was asserted nowhere: a fragment removed drops out of what the tab watches, one
a patch brought with it joins, and a patch lands on the fragment it names while
its neighbour keeps the very node it had. That last one is the half a
subscription cannot enforce, and it is where a caret gets taken.

**The canonicalising that makes a drag silent.** Sorting the pairs and holding
them in a map rather than a list is what stops a pointer move from posting an
identical subscription, and both halves are pinned: reordering fragments says
nothing however often the page changes, and one fragment shown twice is watched
once. The microtask that coalesces a turn is a third test, because sorting
cannot do its half: a turn that changes the page twenty times subscribes once
and names what the turn ended with.

Each of the six was then checked against a mutated runtime, the way the morph's
invariants were. Reading the pairs in document order, holding them in a list,
syncing per announcement rather than per turn, and letting a patch morph every
fragment rather than the one it names each kill at least one. A seventh was
written and deleted: it asserted the opening state of the two after it and no
mutation could fail it alone, which is the difference between a test of this and
a test that passes beside it.

The README says the reason `npm test` exists is that every bug that got past
review lived in the client. What is thin there now is not the live path but the
browser under it. jsdom lays nothing out, so
[sortable.test.js](../../crates/exos/js/tests/sortable.test.js) hands its rows a
geometry to read and every test that would depend on real boxes has to do the
same or not exist.

## Expressions are compiled with `new Function`

A strict CSP without `unsafe-eval` blocks them, so exos cannot be used where
one is mandated. A precompiled mode is the answer: the expressions are known at
render time, so they can be emitted as a table the runtime indexes into rather
than as source it compiles.

Listed here because it is well understood, not because it is small. It touches
the macro, the recorder and the runtime at once, and it is the entry most
likely to need a document of its own.

## Not in here

Four things are deliberately absent, because they are designs rather than
chores and each has somewhere better to live.

- **Sessions, identity and CSRF**, in [sessions and
  identity](sessions-and-identity.md). The live token not being bound to a
  viewer is the README's first listed gap and is a stage of that document, not
  a loose end.
- **Directed effects and audiences**, in [directed
  effects](directed-effects.md), including the `#[derive(Audience)]` sugar,
  which cannot be a loose end before the trait it derives exists.
- **Localization**, in [localization](localization.md), whose first two stages
  are built: the locale set, how a request reaches one, what the document says
  it was rendered in, and the messages themselves, slots included.
  Locale-formatted numbers and everything that has to reach the browser are the
  rest of that document rather than loose ends.
- **Running more than one instance**, in [more than one
  instance](more-than-one-instance.md). The connection registry is a
  process-local `HashMap`, so a publish reaches only the tabs connected to the
  instance that sent it. Swapping a session store does nothing for it. It needs
  a bus, and it is the one thing here that cannot be added quietly later.

Form validation is absent for the same reason, and now has [forms](forms.md) to
be absent into. The `Effect` shape was always right for it; what was missing is
the way rules are expressed, and that document is the design rather than the
chores it has not been broken into yet.
