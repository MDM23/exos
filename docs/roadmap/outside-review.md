# What an outside review found

Nine findings from a review of the tree on 2026-09-07, read against the code,
and what is left of them.

Status: triage. One entry is built, the duplicate id below; the rest is not.
The review itself was an outside
document and is not in the tree, so this is written to stand without it: every
entry says what the defect is rather than pointing at where it was reported.
Entries that belong to a document that already exists say so rather than being
restated, and the three proposals this project declines are kept with the
reason, so they are not re-proposed by the next reader who has the same good
idea.

Everything below was checked against the source. Two entries are marked **read,
not run**: their shape is plain in the code and the race they describe was not
reproduced here.

## Worth fixing

Six defects, each small, each with a fix that fits in the file it is in. They
are ordered by what a user would notice.

### A lagged stream is told it will catch up, and it will not

[stream.rs](../../crates/exos/src/live/stream.rs) drops the `Lagged` error in a
`filter_map`, under a comment saying the next publish brings the tab back in
line. That is true of a price tick and false of everything else. A patch is the
whole current state of a fragment, so a fragment that settles after its last
publish stays wrong on that tab until something publishes it again, which may
be never. The tab is not told, the server is not told, and the repair path that
already exists for a dropped connection never runs.

The fix is the repair, not a bigger channel: on `Lagged`, end the stream.
`EventSource` reopens, the greeting is not the first one, and
[loose ends](loose-ends.md#the-reconnect-gap-is-repaired-from-the-client)
already refetches the page. Sixty-four is then a tuning number rather than a
silent correctness boundary.

### A subscription that failed is remembered as sent

`syncSubscriptions` in [runtime.js](../../crates/exos/js/runtime.js) writes
`subscribed` before the fetch and only unwinds it on a thrown error or a 410. A
500 or a 503 leaves the client believing the server is watching what it asked
for, and the comparison above the fetch means nothing will ask again until the
visible set changes. One line: any response that is not `ok` and not 410 clears
`subscribed` so the next mutation retries.

Backoff and request ordering are the larger version of this and are worth
having, but they are a different entry from the one-line lie.

### An older navigation can land on a newer one

`navigate` awaits a fetch and then morphs, with no check that it is still the
navigation the tab wants. Click through two links quickly, have the first
answer last, and the page and the URL end up on the first. The repair path
solved this already by capturing the URL before the fetch and comparing after
it; navigation needs the same, but against a generation counter rather than a
URL, since navigating twice to the same URL is a thing people do.

### A typed URL does not encode what it interpolates

[route.rs](../../crates/exos-macro/src/route.rs) builds `url` with a bare
`format!`, so a `String` parameter carrying `/`, `?`, `#` or a percent sign
writes a different URL than the one the type promised. This is the sharpest of
the six, because the typed caller exists precisely so that a link cannot be
wrong: it currently proves the parameters are of the right type and not that
they name what they are given. Percent-encode each parameter as one path
segment. A wildcard parameter is the exception and needs its own answer.

### A textarea's value is not an attribute

The property reapply in `syncAttributes` reads `to.getAttribute("value")`,
which a `<textarea>` never has: its value is its text content. An unbound
textarea a user has typed into therefore keeps the user's text when the server
sends a new one, while an `<input>` in the same position takes the server's.
Read the incoming value per element kind and the branch keeps the one exception
it means to have, which is a control a binding owns.

### A navigation changes the body and nothing else about the document

`navigate` morphs `document.body` and sets the title. `lang`, `dir`, and
anything in the head do not move, so a page in another language is served as
one and read as the previous one. Localization is the reason to care and is why
this is not cosmetic: [localization](localization.md) puts the locale on the
document, and navigation currently unputs it.

The narrow fix is `lang` and `dir` on `<html>`, which is the part with an
obvious right answer. Head reconciliation in general, stylesheets and scripts
and their lifecycles, is a design and not a fix.

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

## Two grants that outlive the authority behind them

Both findings raised as urgent are one sentence: cryptographically valid is not
currently the same as still authorized, and exos has no way to say the
difference. **Read, not run.**

- `Topic::verify` in [live.rs](../../crates/exos/src/live.rs) checks an HMAC
  over the topic and the session id. Nothing consults the application about
  whether that session is still one. A browser that keeps a cookie and a token
  from before it was signed out can subscribe again, and the empty audience set
  the resolver correctly returns does not stop it, because a topic grant is not
  an audience.
- `stream()` resolves identity and then registers, with an await between. A
  rotation landing in that window closes the streams it can see, and the one
  being opened is not yet one of them, so it registers afterwards with the
  audiences of the session that has just gone.

[Sessions and identity](sessions-and-identity.md) stage 4 already knows the
second shape of this: it is why signing in answers with a `reload` rather than
a patch. What it does not have is a fence. One generation counter per session
name, checked under the registry lock at `open` and at `subscribe`, answers
both, and the cluster version of it is the awkward part rather than the
process-local one. This wants to become a stage of that document rather than
living here.

## Where it moves a document that already exists

- **CSRF.** [Stage 7](sessions-and-identity.md#stage-7-csrf-which-is-mostly-already-handled)
  argues that `SameSite=Lax`, the `X-Exos` header and JSON bodies are together
  a policy, with one narrow gap at form-encoded handlers. The case that
  argument misses is a good one: a handler taking no body at all is outside the
  JSON leg too, `X-Exos` is sent and never required, and Lax does not separate
  a sibling origin on the same site. Sign-out is exactly such a handler. The
  gap is wider than the stage claims and the guide repeats the claim, so both
  need amending, and the answer is still the one the stage names plus requiring
  the header exos already sends.
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
