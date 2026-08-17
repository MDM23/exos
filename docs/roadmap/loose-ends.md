# Loose ends

The small things. Each one is known, each one is independent, and none of them
is big enough to argue with itself for a document.

Status: open unless an entry says otherwise. The two design documents,
[sessions and identity](sessions-and-identity.md) and [directed
effects](directed-effects.md), describe systems that are unfinished: the first
is built as far as the session, the second not at all. This one describes work
on the parts that are finished, which is why the entries are short: the thinking
is already done and what is left is the doing.

Ordered by what would break if it stayed undone, not by effort.

## The reconnect gap leaves fragments stale

`EventSource` reconnects on its own, the server has forgotten the connection,
and the client re-subscribes. Anything published in between is gone, and a
fragment that changed during the gap stays wrong until the next publish, which
may be never. That is a bug in what is shipped today rather than a limitation
of something planned.

The server cannot repair it alone. A topic is a hash of a name and its
arguments, and nothing can re-invoke the function from it, which is the same
wall [directed effects](directed-effects.md) hits in its stage 5.

The cheap fix is on the client: on a reopen that *follows a drop*, rather than
on the first open, re-fetch the current URL and morph. One call to
`navigate(location.href, false)` repairs everything state-backed in one
request.

The detail to get right: a navigation re-seeds the signals the arriving
document declares, because a page says what its own state starts as. A repair
is the same page arriving again rather than a different one, so it has to morph
*without* re-seeding, or a dropped connection empties the field somebody is
typing into.

## The stream carries five of the eight steps

[runtime.js](../../crates/exos/js/runtime.js) registers listeners for
`navigate`, `page`, `patch`, `remove` and `signals`. `focus`, `reload` and
`scroll` are missing.

That is invisible while the stream only carries patches and becomes arbitrary
the moment it carries effects, which is what [directed
effects](directed-effects.md) makes it do. `focus` and `scroll` from a
background job are rude, but they are rude in exactly the way an application
chooses, and a framework that refuses to deliver them is a surprise that shows
up as silence.

So: register all eight, or make the exclusion deliberate and say why in the
comment next to the list. Either is fine and the present state, which is
neither, is not.

## Publishing scans every connection

`publish` takes a `Mutex` over the whole registry and walks it. At presence
volumes that is invisible, and it is the wrong shape for one send per
notification against thousands of connections.

The fix is an index from topic to connection ids, maintained on subscribe and
on close, so both `publish` and a future `send` become a lookup. Two
constraints on whoever writes it: `await_holding_lock` is on, so the send path
stays synchronous, which `broadcast::Sender::send` allows; and the index has to
be dropped in `close` or it outlives the connections it names.

Not worth doing before something feels it. Worth knowing where it is when
something does.

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
The handshake and the 410 path have coverage now. What still does not:

- a patch arriving for a fragment on screen, and none arriving for one that is
  not, which is the entire point of the topic model and is checked nowhere
- the coalescing that makes a drag silent, which
  [runtime.js](../../crates/exos/js/runtime.js) explains at length and nothing
  checks
- the reconnect repair above, once it exists

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
