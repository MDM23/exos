# What an outside review found

Nine findings from a review of the tree on 2026-09-07, read against the code,
and what is left of them.

Status: built, all of it, and each entry says what closed it and what was
declined on the way. The review itself was an outside document and is not in
the tree, so this is written to stand without it: every entry says what the
defect was rather than pointing at where it was reported. Entries that belong
to a document that already exists say so rather than being restated, and the
three proposals this project declines are kept with the reason, so they are not
re-proposed by the next reader who has the same good idea.

Everything below was checked against the source, and the two findings that were
read rather than run when this was written have since been run: the race one of
them describes has its own test.

## Worth fixing

Six defects, each small, each with a fix that fitted in the file it was in.
They are ordered by what a user would notice, and all six are built.

### A lagged stream is told it will catch up, and it will not. Done

[stream.rs](../../crates/exos/src/live/stream.rs) drops the `Lagged` error in a
`filter_map`, under a comment saying the next publish brings the tab back in
line. That is true of a price tick and false of everything else. A patch is the
whole current state of a fragment, so a fragment that settles after its last
publish stays wrong on that tab until something publishes it again, which may
be never. The tab is not told, the server is not told, and the repair path that
already exists for a dropped connection never runs.

The fix was the repair, not a bigger channel: `map_while` ends the stream on
`Lagged`. `EventSource` reopens, the greeting is not the first one, and
[loose ends](loose-ends.md#the-reconnect-gap-is-repaired-from-the-client)
already refetches the page. Sixty-four is now a tuning number rather than a
silent correctness boundary, which is what the constant's own docs say. A test
lags a real stream past the capacity and reads the end of its body.

### A subscription that failed is remembered as sent. Done

`syncSubscriptions` in [runtime.js](../../crates/exos/js/runtime.js) writes
`subscribed` before the fetch and only unwinds it on a thrown error or a 410. A
500 or a 503 leaves the client believing the server is watching what it asked
for, and the comparison above the fetch means nothing will ask again until the
visible set changes. One line: any response that is not `ok` and not 410 now
clears `subscribed`, so the next mutation asks again.

Backoff and request ordering are the larger version of this and are worth
having, but they are a different entry from the one-line lie.

### An older navigation can land on a newer one. Done

`navigate` awaited a fetch and then morphed, with no check that it was still
the navigation the tab wanted. Click through two links quickly, have the first
answer last, and the page and the URL ended up on the first. The repair path
solved this already by capturing the URL before the fetch and comparing after
it; navigation counts instead, since navigating twice to the same URL is a
thing people do. A page an action hands over takes a number too: it is the
navigation the tab is on, and a fetch still in flight has been overtaken by it.

### A typed URL does not encode what it interpolates. Done

[route.rs](../../crates/exos-macro/src/route.rs) built `url` with a bare
`format!`, so a `String` parameter carrying `/`, `?`, `#` or a percent sign
wrote a different URL than the one the type promised. This was the sharpest of
the six, because the typed caller exists precisely so that a link cannot be
wrong: it proved the parameters were of the right type and not that they name
what they are given.

`exos::segment` percent-encodes one parameter, keeping the unreserved set and
escaping everything else as the bytes it is made of. The wildcard's own answer
is `exos::segments`, which keeps the separators and encodes each name between
them, because `{*rest}` is a path rather than a segment. The macro picks by the
shape of the parameter in the path, and a route test builds a URL and asks for
it, so the round trip through axum's own decoding is what is asserted rather
than the spelling alone.

### A textarea's value is not an attribute. Done

The property reapply in `syncAttributes` read `to.getAttribute("value")`, which
a `<textarea>` never has: its value is its text content. An unbound textarea a
user had typed into therefore kept the user's text when the server sent a new
one, while an `<input>` in the same position took the server's. The incoming
value is now read per element kind, and the branch keeps the one exception it
means to have, which is a control a binding owns.

### A navigation changes the body and nothing else about the document. Done

`navigate` morphed `document.body` and set the title. `lang`, `dir`, and
anything in the head did not move, so a page in another language was served as
one and read as the previous one. Localization is the reason to care and is why
this was not cosmetic: [localization](localization.md) puts the locale on the
document, and navigation unput it.

`lang` and `dir` move now, and a page saying nothing about either clears them
rather than inheriting what was there. One function puts a whole document up,
so a navigation, a page an action handed over and a repair after a reconnect
all move the same amount of it. Head reconciliation in general, stylesheets and
scripts and their lifecycles, is still a design rather than a fix.

## The one that was undersold, and is done

Duplicate `id` values on repeated fragments were raised as a validity and
accessibility complaint. It is worse than that, and it is the only thing here
that was not already visible from reading.

`Fragment::to_markup` writes the topic as the wrapper's `id`, so the same
fragment rendered twice on one page emits two elements with one name.
`morphChildren` indexes surviving children by `id`, the second overwrites the
first in that map, and both wrappers are rebuilt on every patch rather than
morphed. Probed against the jsdom harness: with distinct ids, both nodes keep
their identity across a patch; with the shared id, neither does. Everything
node identity buys, focus, selection, an open `<details>`, a `data-preserve`
widget, playing media, is lost on both copies every time either publishes.

The fix was to stop overloading `id`: the topic is the subscription's name and
belongs in `data-topic`, leaving `id` free to be unique or absent. It moved
`to_markup`, the subscription selector and the map keyed off it, and what a
patch looks its target up by, which is now the topic where there is one and the
id everywhere else.

The morph's keying went with it, and further than the wrapper: a name two
children share names neither of them, so both fall back to position rather than
the second overwriting the first. That is what makes duplicate copies keep
their nodes, and it holds for a duplicated `id` an application writes by hand
as well.

## Two grants that outlive the authority behind them. Done

Both findings were raised as urgent and as one sentence: cryptographically
valid is not the same as still authorized, and exos has no way to say the
difference. Read against the code they turned out to be one defect, one
missing lever, and one thing that was never as bad as it read.

### The registration race, which was the defect

`stream()` resolved identity and then registered, with an await between. A
rotation landing in that window closed the streams it could see, and the one
being opened was not yet one of them, so it registered afterwards with the
audiences of the session that had just gone and held them for as long as the
tab stayed open. The window contained the application's database call, so it
was tens of milliseconds rather than instants.

The fix is the order, not a counter. A connection registers before the resolver
is awaited and is identified afterwards, and `identify` answers with whether
there was still a connection to identify: a revocation in the window now finds
something to end, and the stream is told so and answers the tab with an ended
stream rather than a status, since `EventSource` treats a status as a failure
and stops. Nothing can reach a connection in between, because a send matches
audiences and a publish matches topics and it has neither.

### The token, which the connection already fences

`Topic::verify` checks an HMAC over the topic and the session id, and consults
nobody about whether that session is still one. What that is worth is smaller
than it looks. `subscribe` does not resolve identity; it writes topics onto a
connection that was identified when it opened, so a revocation that ends the
stream ends the grant with it, and the tab comes back and is resolved again. A
browser whose cookie has actually been taken away verifies nothing at all,
because a token without a session verifies against nothing.

What is left is a browser that goes on sending a cookie the application has
retired, and what it can watch is a live fragment, whose content is
viewer-independent by
[this crate's own invariant](../../crates/exos/src/live.rs). The lever below
closes it wherever the application knows to say so; closing it in general would
take asking the resolver again in `subscribe`, which is the wrong trade twice
over. `Ok(Audiences::none())` today means *anonymous*, *a name nobody
recognises* and *signed out* at once, so the resolver would need a fourth
answer, which is a breaking change to every `identify`. And it would put a
database call on every change of a tab's visible fragment set, where the whole
design is one resolve per connection.

### The lever that was missing

A resolver runs once per connection, so authority taken away without the cookie
changing, a viewer removed from a team, an account disabled, reached nothing at
all: exos holds a name and nothing behind it, and had no way to be told. It
does now. `disconnect` was already written, already crosses the bus and already
had tests, and it is public: the half that knows says so, every tab under that
name ends, and each comes back asking who it is now.

### What a generation counter would have cost

The counter this document originally proposed closes the last microseconds of
the race, between the middleware reading the cookie and the handler taking the
registry lock. It needs revocations to be *remembered* rather than performed,
which is a map keyed by session name with a retention policy: a tunable, a
sweeper, and a new silent failure when the sweep is too eager.

On a cluster it is worse than awkward, and worth writing down before anybody
proposes it again. Ending a stream is idempotent and needs no agreement: a
revocation crosses as a key, every node applies it to its own registry, and a
node that never held one of that browser's tabs does nothing. Remembering a
revocation is shared state with a lifetime. Every node would have to hold the
same set of retired names for the same window, which means a frame that is not
a fan-out but a fact to be replicated, a node that joins late or misses a frame
holding a set with a hole in it and no way to know, and clocks agreeing on when
an entry may go. That is a coordination problem in a system that has carefully
avoided having one: [more than one instance](more-than-one-instance.md) is
built on frames nobody has to acknowledge.

So the counter stays unbuilt. What is left of the race is a window with no
await in it, and the residual case is a browser whose stream request read a
cookie that a rotation retired between that read and the registry lock, which
costs that tab a stale identity until it reconnects.

## Where it moves a document that already exists

- **CSRF, and the review was right.**
  [Stage 7](sessions-and-identity.md#stage-7-csrf-which-is-one-rule) argued
  that `SameSite=Lax`, the `X-Exos` header and JSON bodies were together a
  policy, with one narrow gap at form-encoded handlers. The case that argument
  missed is a good one: a handler taking no body at all is outside the JSON leg
  too, `X-Exos` was sent and never required, and Lax does not separate a
  sibling origin on the same site. Sign-out is exactly such a handler. The
  stage and the guide are amended and the header is required now, which is the
  answer the stage named plus the one thing it left out.
- **Publishing scans every connection** and the bus spawning a task per frame
  are [loose ends](loose-ends.md#publishing-scans-every-connection) and
  [more than one instance](more-than-one-instance.md), with the index they ask
  for already specified there.
- **`new Function` and strict CSP** is
  [a known loose end](loose-ends.md#expressions-are-compiled-with-new-function)
  with the precompiled table already named as the answer.
- **Async resources** is [async fragments](async-fragments.md), whose stage 1
  is the recorder thread-local the review independently arrived at.

## What is declined

- **Making the runtime belong to `App`.** The global registries are the
  thesis, not an oversight: [context.rs](../../crates/exos/src/context.rs)
  states the trade in its module docs, and `data::<T>()` reading without being
  threaded through every signature is most of what makes a handler short. Two
  configured applications in one process is a use nobody has asked for, and
  paying for it in every call site is the wrong direction. The real cost behind
  the proposal is test isolation, and that is worth attacking directly, where
  `with_scope` already shows the shape.
- **Splitting runtime.js by responsibility.** Size is not the argument; a
  boundary is, and the file has none that a module would follow. Navigation,
  morphing and bindings are one machine on purpose: the morph rebinds, the
  navigation morphs, the subscription reads what the morph left. Splitting it
  would produce four files importing each other in a cycle and one more build
  step for a runtime that currently ships as a single script with no build at
  all. Revisit when a part of it has genuinely stopped talking to the rest.
- **A CRDT island.** Optional and expensive by its own account, and there is no
  application in the tree that wants it. Nothing to decide.

## Worth wanting, not now

View transitions are cheap, opt-in, and wrap the DOM commit that already
exists. A real-browser suite is the honest way to test focus, IME and cookie
policy, and the jsdom harness is knowingly short of all three. Neither is a
defect, so both wait behind the six above.
