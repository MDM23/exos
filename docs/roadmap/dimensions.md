# Dimensions

What a fragment varies over besides its arguments.

Status: not built. This came out of asking whether live fragments and directed
effects should be one mechanism, with the server rendering per subscriber. That
answer is no, for reasons recorded below so the question is not reopened
without new information. What was worth keeping from it is this: a fragment can
legitimately vary over a small, declared set of facts about the viewer, and
today an application has to thread each of them through every signature by
hand.

[Localization](localization.md) is the first thing that wants it, and its stage
4 is written against this document.

## What it is for

A locale is not an argument of `lot(id)`. It is a fact about who is looking,
the same fact for every fragment on the page, and adding `locale: Locale` to
every live fragment in an application is the kind of boilerplate that is
tolerated once and resented by the fifth signature.

It is also not a fact the fragment may simply read. A fragment renders through
[`detached`](../../crates/exos/src/scope.rs), so `exos::scope()` panics inside
one, and that is not an inconvenience to route around: it is what makes the
topic invariant true. If a fragment could read ambient state, its topic would
stop determining its content and two viewers would receive each other's
markup.

A dimension is the reconciliation. It is ambient at the call site and explicit
in the topic, so a fragment can read it without the invariant weakening by one
inch.

## Why not merge the two mechanisms

The proposal was that a subscription carries the viewer's identity, and a
publish renders per subscriber. Three things decide against it, and each is
worth keeping written down.

**The topic is the memoization key.**
[`publish`](../../crates/exos/src/live/stream.rs) renders once and clones the
framed event to every matching connection. Render
per subscriber and a lot with ten thousand watchers renders ten thousand times,
inside the ordering lock that is held across the render on purpose. The fix is
to memoize by whatever actually affects the render, which is the topic. So the
merged model does not remove topics, it re-derives them as a cache key, and the
key is no longer verifiable: today a fragment's arguments are provably its
whole input, and a render that may read the viewer has no such proof.

**Per-subscriber rendering needs per-subscriber context, out of band.**
`identify` runs once per connection when the stream opens, which is what makes
it affordable to be async and fallible. Rendering per subscriber means either
the framework holds session contents, which [sessions and
identity](sessions-and-identity.md#stage-3-and-no-store-at-all) argued its way
out of after building one, or it re-resolves per subscriber per publish, which
is a database call per viewer per price change.

**State and events do not merge whatever the addressing does.** A patch is
idempotent state replacement, repaired by the next publish and by the reconnect
path. A directed effect has no fragment to re-render from and is correctly lost
if the tab was closed. One mechanism carrying both needs a flag saying which
kind it is, which is two concepts wearing one name.

## The rule

A dimension is a type that satisfies four conditions. They are not
independent: each one is what makes the next affordable.

- **Its domain is finite and enumerable.** The same `exos::Enumerable` bound
  [localization](localization.md) uses to decide what may branch a message. A
  publish walks the domain, so an unbounded one is a fragment per viewer.
- **It is resolved once per request**, from the request scope, by the
  application, exactly where a locale is resolved today.
- **It is part of the topic.** `Topic::new` hashes the fragment name and its
  arguments; a dimension is hashed alongside them.
- **It is readable inside a fragment, and it is the only ambient thing that
  is.** The mask keeps blocking `exos::scope()`.

The invariant is not weakened, it is restated: **a topic is a fragment's name,
its arguments and its dimensions, and its content is a function of exactly
those.**

## Stage 1: declaring one

```rust
exos::topic_dimension!(Locale);
```

Which asserts the bound, registers the type so a topic knows to include it, and
generates nothing else. A dimension is a type the application already has, not
a new place to put data.

The name says where the effect lands. Declaring one changes every topic id in
the application, and a reader who has to guess what `dimension!` meant is a
reader who did not know that.

**It has to be a macro rather than a bare trait impl**, which closes a question
an earlier draft of this document left open. Stage 4 enumerates the declared
dimensions at run time to compute the combinations, and a trait impl cannot be
enumerated without a registry. exos already discovers routes through
`inventory`, so this registers there and the machinery is not new.

**[`locales!`](localization.md) registers `Locale` itself**, rather than asking
for a second declaration next to it. The failure mode decides it: an
application that declares its languages and forgets the dimension delivers
German markup to an English reader, silently. The automatic version costs a
wordless fragment one render per watched language; the manual version costs
correctness the first time somebody forgets.

The number of them is small by construction, because they multiply. Locale is
one. A unit system could be another. The auction's `lot(id, role)` in
[room.rs](../../examples/auction/src/room.rs) is a third, threaded by hand
today, and it is the example that shows this is not a hypothetical shape. A
viewer id cannot be one, and [what this does not
fix](#what-this-does-not-fix) says why.

## Stage 2: the topic carries it

Inside a request, rendering `lot(7)` folds the current value of every declared
dimension into the topic, so the element that lands in the page is addressed by
`(lot, 7, De)` rather than by `(lot, 7)`. The client keeps doing exactly what it
does now: it reads the id and the token the server wrote and claims them back.

This is what removes an entire class of bug rather than merely the boilerplate.
There is no way to render a page in German and then receive an English patch,
because the language is in the address. A disagreement between what the page
was rendered as and what the publisher believes cannot deliver the wrong
content; it can only deliver nothing, which is visible and debuggable rather
than silent and wrong.

## Stage 3: rendering inside the frame

A fragment body runs inside a frame holding the values its topic names, so
`exos::locale()` answers inside a fragment while `exos::scope()` still panics
there. `detached` gains the dimensions and loses nothing else.

That the frame holds exactly the declared dimensions is the whole safety
property, and it is enforced the same way the mask is: by there being no other
way in.

## Stage 4: publishing fans out

A publish knows the fragment and its arguments, so it enumerates the domain of
each declared dimension, computes the topic for every combination, and renders
only the combinations somebody is actually watching.

```rust
publish(lot(7));   // renders once per locale being watched, and no more
```

Two properties worth stating, because they are what make this cheap:

- **The connection needs to know nothing.** No dimension resolver on the
  stream, no new field on the `Connection` record, no second hook beside
  `identify`. The subscription set already encodes the answer, since the
  dimension is in the topic the client claimed.
- **Nobody watching costs a hash and a lookup**, not a render. An application
  shipping eight languages with two in use renders twice.

### Sequential, and still synchronous

The renders happen one after another, each under the lock for the combination
it is rendering, and each patch goes out as its render finishes rather than
being batched until the last one is done. So the first language's viewers are
not waiting on the eighth language's render, and nothing becomes concurrent.

The correctness requirement is per topic rather than global, which is what the
lock is keyed by since [loose ends](loose-ends.md) closed that entry. Each
combination is its own topic, so "the newest patch wins" only has to hold within
a combination, which is what makes sending as you go safe, and a fan-out holds
one combination's lock at a time rather than one lock for all of them.

`publish` is a sync `fn` today because a fragment cannot await at all, so it
renders from state readable without awaiting: a process-global store in the
examples, a projection the write path keeps fresh in an application with a
database. That is scheduled to change in [asynchronous
fragments](async-fragments.md), and the two designs meet at the same place. A
fan-out multiplies the render and an awaiting render lengthens it, so both would
have made a single lock the problem, and both were waiting on the same
precondition.

What must not change either way is where the read happens. Hoisting it out of
the fragment to save the repetition is the one thing a caller must not do,
because reading before the lock is the stale-patch race that a fragment carrying
its render was built to remove.

What does grow is the hold time, from one render to as many as there are watched
combinations, and it is now the combination's own publishers that wait for it.
That leaves the index [loose ends](loose-ends.md) wants for the registry walk as
the thing this makes more valuable, since a fan-out walks the registry once per
combination.

## What this does not fix

- **The live token.** A token is an HMAC over the topic id, and a topic that
  now includes a dimension is still just a topic. Binding it to a session
  remains the open problem it is in [sessions and
  identity](sessions-and-identity.md), for the reason that document gives:
  `publish` renders outside any request, so a patch introducing a new fragment
  has no session to bind to.
- **Per-viewer fragments.** A viewer id has an unbounded domain, so it stays
  what it is today, an ordinary argument, and `inbox_count(user)` keeps
  working exactly as it does. A dimension is for facts shared by many viewers,
  not for facts that distinguish one.
- **Directed effects.** They address a person rather than a screen region and
  carry things that are not state. Nothing here touches them, and
  [directed effects](directed-effects.md) stays the document for that.

## What it costs

- **The fan-out multiplies.** Two dimensions of three values each is nine
  possible renders per publish, and only the watched ones are free. Dimensions
  need a documented cap, and the honest guidance is that an application with
  three of them has probably mistaken an argument for a dimension.
- **Ambient state, deliberately introduced.** exos has spent several decisions
  removing ambient reads from fragments, and this adds one back under
  conditions that keep the invariant. The conditions are the whole design, and
  a future change that relaxes any of the four rules should be read as
  reopening it.
- **A publish becomes a loop.** The single render under the lock was easy to
  reason about, and a loop over combinations is less so.
- **A publish holds more locks**, one per combination it renders, in a design
  whose ordering guarantee rests on holding them. Taken and let go one at a
  time, since a combination is ordered against itself and against nothing else.

## Testing

- A fragment rendered in two locales produces two topics, and a publish reaches
  the tab watching each with the markup for that one.
- A publish with nobody watching a combination does not render it, which is
  worth a test because it is the property the whole design rests on and it is
  invisible in behaviour.
- `exos::scope()` still panics inside a fragment after the frame exists, which
  is the test that stops this from quietly becoming a way in.

## Open questions

- **Whether the fan-out should read once and render many.** The closure runs
  per combination, so an expensive read inside it repeats. Splitting it into a
  read phase and a render phase would fix that and costs a second closure at
  every call site, which is a poor trade while reads are lookups.
- **What a fragment does when a dimension is declared after it was written.**
  Every existing topic id changes, which is a cache invalidation with no
  cache, so probably nothing, but it deserves a sentence somewhere.
- **Whether the cap on the fan-out is enforced or documented.** The domains
  are const, so a const assertion is available, and the counter-argument is
  that a legitimate eight-language application would trip it.
