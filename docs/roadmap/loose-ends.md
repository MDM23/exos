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

Still not worth doing before something feels it, and nothing has. Worth knowing
where it is when something does.

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

## The client's live path is barely tested

jsdom implements neither `EventSource` nor `fetch`, so until the harness grew
both, the whole of `openStream` and `syncSubscriptions` ran in no test at all.
The handshake, the 410 path, the step vocabulary and all three rules of the
reconnect repair have coverage now. What still does not:

- a patch arriving for a fragment on screen, and none arriving for one that is
  not, which is the entire point of the topic model and is checked nowhere
- the coalescing that makes a drag silent, which
  [runtime.js](../../crates/exos/js/runtime.js) explains at length and nothing
  checks

The README says the reason `npm test` exists is that every bug that got past
review lived in the client. This is the corner of the client that reasoning
reaches least well.

## Expressions are compiled with `new Function`

A strict CSP without `unsafe-eval` blocks them, so exos cannot be used where
one is mandated. A precompiled mode is the answer: the expressions are known at
render time, so they can be emitted as a table the runtime indexes into rather
than as source it compiles.

Listed here because it is well understood, not because it is small. It touches
the macro, the recorder and the runtime at once, and it is the entry most
likely to need a document of its own.

## Not in here

Three things are deliberately absent, because they are designs rather than
chores and each has somewhere better to live.

- **Sessions, identity and CSRF**, in [sessions and
  identity](sessions-and-identity.md). The live token not being bound to a
  viewer is the README's first listed gap and is a stage of that document, not
  a loose end.
- **Directed effects and audiences**, in [directed
  effects](directed-effects.md), including the `#[derive(Audience)]` sugar,
  which cannot be a loose end before the trait it derives exists.
- **Running more than one instance.** The connection registry is a process-local
  `HashMap`, so a publish reaches only the tabs connected to the instance that
  sent it. Swapping a session store does nothing for it. It needs a bus, and it
  is the one thing here that cannot be added quietly later.

Form validation and localization are absent for the opposite reason: the
`Effect` shape is right for the first and the plan for the second is a
paragraph in the README, and neither has been designed enough to break into
chores.
