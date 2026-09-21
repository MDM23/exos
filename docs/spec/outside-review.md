# What an outside review found

Nine findings from a review of the tree on 2026-09-07, all of them built or
declined. The review itself was an outside document and is not in the tree, so
each entry says what the defect was rather than pointing at where it was
reported. The three declines are kept with their reasons, so they are not
re-proposed by the next reader who has the same good idea.

## Six defects, all fixed

- **A lagged stream was told it would catch up.**
  [stream.rs](../../crates/exos/src/live/stream.rs) dropped `Lagged` in a
  `filter_map`, which is true of a price tick and false of everything else: a
  patch is the whole current state of a fragment, so one that settles after its
  last publish stays wrong on that tab forever. The fix was the repair, not a
  bigger channel: `map_while` ends the stream, `EventSource` reopens, and the
  [reconnect repair](../roadmap/loose-ends.md#the-reconnect-gap-is-repaired-from-the-client)
  refetches the page. Sixty-four is now a tuning number rather than a silent
  correctness boundary.
- **A subscription that failed was remembered as sent.** `syncSubscriptions`
  wrote `subscribed` before the fetch and unwound it only on a throw or a 410,
  so a 500 left the client believing the server was watching, and nothing asks
  again until the visible set changes. Any response that is not `ok` and not 410
  now clears it. Backoff and request ordering are the larger version of this and
  are a different entry.
- **An older navigation could land on a newer one.** `navigate` awaited a fetch
  and morphed without checking it was still the navigation the tab wanted.
  Navigations are counted rather than compared by URL, since navigating twice to
  the same URL is a thing people do, and a page an action hands over takes a
  number too.
- **A typed URL did not encode what it interpolated.**
  [route.rs](../../crates/exos-macro/src/route.rs) built `url` with a bare
  `format!`, so a `String` carrying `/`, `?`, `#` or a percent sign wrote a
  different URL than the type promised. The sharpest of the six, because the
  typed caller exists precisely so a link cannot be wrong. `exos::segment`
  percent-encodes one parameter; `exos::segments` keeps the separators and
  encodes each name between them, because `{*rest}` is a path rather than a
  segment. A route test builds a URL and asks for it, so the round trip through
  axum's own decoding is what is asserted.
- **A textarea's value is not an attribute.** The property reapply read
  `to.getAttribute("value")`, which a `<textarea>` never has, so an unbound one
  a user had typed into kept the user's text where an `<input>` took the
  server's. The incoming value is read per element kind now, keeping the one
  exception it means to have, a control a binding owns.
- **A navigation changed the body and nothing else about the document.** `lang`,
  `dir` and the head did not move, so a page in another language was served as
  one and read as the previous one, which is why this was not cosmetic. One
  function now puts a whole document up, so navigation, a page an action handed
  over, and a repair after a reconnect all move the same amount of it. Head
  reconciliation in general is still a design rather than a fix.

## Duplicate ids, which were undersold

Raised as validity and accessibility, and worse than that. `Fragment::to_markup`
wrote the topic as the wrapper's `id`, so one fragment rendered twice emitted
two elements with one name. `morphChildren` indexes surviving children by `id`,
the second overwrote the first, and both wrappers were rebuilt on every patch
rather than morphed: focus, selection, an open `<details>`, a `data-preserve`
widget and playing media, lost on both copies every time either published.
Probed against the jsdom harness both ways.

The fix was to stop overloading `id`: the topic is the subscription's name and
lives in `data-topic`, leaving `id` free to be unique or absent. The morph's
keying went with it and further than the wrapper, so a name two children share
names neither of them and both fall back to position, which holds for a
duplicated `id` an application writes by hand as well.

## Two grants that outlived their authority

Raised as urgent and as one sentence: cryptographically valid is not the same as
still authorized. Read against the code they were one defect, one missing lever,
and one thing that was never as bad as it read. The defect was a registration
race and the lever is `disconnect`; both are described in [sessions and
identity](sessions-and-identity.md#identity-on-the-stream), along with why
`Topic::verify` does not consult the resolver.

### What a generation counter would have cost

Worth writing down before anybody proposes it again. The counter closes the last
microseconds of the race, between the middleware reading the cookie and the
handler taking the registry lock. It needs revocations to be *remembered* rather
than performed: a map keyed by session name with a retention policy, a tunable,
a sweeper, and a new silent failure when the sweep is too eager.

On a cluster it is worse than awkward. Ending a stream is idempotent and needs
no agreement: a revocation crosses as a key, every node applies it to its own
registry, and a node that never held one of that browser's tabs does nothing.
Remembering one is shared state with a lifetime: every node holding the same set
of retired names for the same window, a frame that is a fact to be replicated
rather than a fan-out, a node that joins late or misses a frame holding a set
with a hole in it and no way to know, and clocks agreeing on when an entry may
go. That is a coordination problem in a system that has carefully avoided
having one, since [more than one
instance](../roadmap/more-than-one-instance.md) is built on frames nobody has
to acknowledge.

What is left of the race is a window with no await in it: a browser whose stream
request read a cookie that a rotation retired between that read and the registry
lock costs that tab a stale identity until it reconnects.

## What is declined

- **Making the runtime belong to `App`.** The global registries are the thesis,
  not an oversight: [context.rs](../../crates/exos/src/context.rs) states the
  trade in its module docs, and `data::<T>()` reading without being threaded
  through every signature is most of what makes a handler short. Two configured
  applications in one process is a use nobody has asked for. The real cost
  behind the proposal is test isolation, and that is worth attacking directly,
  where `with_scope` already shows the shape.
- **Splitting runtime.js by responsibility.** Size is not the argument; a
  boundary is, and the file has none a module would follow. Navigation, morphing
  and bindings are one machine on purpose: the morph rebinds, the navigation
  morphs, the subscription reads what the morph left. Splitting it produces four
  files importing each other in a cycle and one more build step for a runtime
  that ships as a single script with no build at all. Revisit when a part of it
  has genuinely stopped talking to the rest.
- **A CRDT island.** Optional and expensive by its own account, and no
  application in the tree wants it.

## Worth wanting, not now

View transitions are cheap, opt-in, and wrap the DOM commit that already exists.
A real-browser suite is the honest way to test focus, IME and cookie policy, and
the jsdom harness is knowingly short of all three. Neither is a defect.

## Where the rest went

- **CSRF, and the review was right.** The header is required now; see [sessions
  and identity](sessions-and-identity.md#csrf-is-one-rule).
- **Publishing scans every connection**, and the bus spawning a task per frame:
  [loose ends](../roadmap/loose-ends.md#publishing-scans-every-connection) and
  [more than one instance](../roadmap/more-than-one-instance.md), with the index
  they ask for already specified there.
- **`new Function` and strict CSP**: [a known loose
  end](../roadmap/loose-ends.md#expressions-are-compiled-with-new-function),
  with the precompiled table named as the answer.
- **Async resources**: [async fragments](../roadmap/async-fragments.md), whose
  stage 1 is the recorder thread-local the review independently arrived at.
