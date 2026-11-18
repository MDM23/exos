# Refreshing

Part of a page that reads itself again while somebody is looking at it, pulled
by the tab rather than pushed by the server.

Status: not built. Proposed 2026-11-18 from an application's monitoring page,
whose session figures are windows of fifteen minutes to a week over data no
event describes.

## Where push stops

A live fragment is pushed: something happens, the server publishes, every tab
watching gets the patch. That needs a moment to publish at, and plenty of what
a page shows has none.

- **Time is the change.** "Active in the last fifteen minutes" falls as the
  clock moves, with no write behind it. The only trigger is a timer, and a
  timer on the server runs whether or not anybody is watching, since exos
  cannot tell whether a topic has subscribers.
- **The content is the viewer's.** A list whose links depend on what the reader
  may open breaks the topic invariant: two readers would share one topic and
  receive each other's markup. Making the permission an argument works and
  multiplies topics by every distinction a view makes.
- **Nobody needs it now.** A figure over a week does not need to arrive within
  a second. Once a minute, while the tab is in front of somebody, is the whole
  requirement, and push is the expensive way to meet it.

Pull answers all three. The tab asks, so it only happens while a tab exists. It
asks as a request, so the session, the scope and every permission check apply
exactly as they did when the page was served. And it asks on its own schedule,
so the server does nothing on behalf of a page nobody has open.

## What is already there

The runtime can already read a page again. `repair` fetches the current URL
with the navigation header and morphs the answer in **without re-seeding**,
which is how a tab recovers after its stream dropped: the fragments come back,
and the field somebody is typing in keeps what they typed.

A refresh is a repair that happens on a timer instead of after a reconnect.
Nearly everything a refresh has to get right, a repair already does: the page
moved on while the fetch was in flight, a response that is not a page, focus
and caret, signals that must not be reset.

## The proposal: one attribute

```rust
<section id="sessions" {refresh(Duration::from_secs(60))}>
    { figures(&sessions) }
</section>
```

`refresh(period)` marks a subtree that reads itself again from the current URL
every `period`. The runtime fetches the page as a repair does and morphs only
the element with the same `id` out of the answer. On `<main>` it is a page that
refreshes itself; on a card it is an island. One attribute covers both.

- **No route of its own.** The island is found again in the page it came from,
  so there is nothing to mount, no arguments to carry back and nothing to sign.
  A dedicated endpoint would need all three, and the arguments would need
  proving the way a topic's token proves a subscription.
- **An `id` is required**, because it is what finds the island in the answer
  and what morphing keys on anyway. A `refresh` without one is a compile error
  if the macro can see it and a console error if it cannot.
- **One fetch per period, not per island.** Islands sharing a period share a
  request and each takes its own subtree from the answer.

### While nobody is looking

- **A hidden tab does not pull.** `document.hidden` skips the tick. This is the
  property push could not have: the cost follows attention.
- **A tab coming back pulls at once** if a tick was skipped while it was
  hidden, so returning to a tab never shows something older than its period.
- **Jitter.** The first tick lands somewhere inside the first period, so a
  hundred tabs opened by one announcement do not ask in step forever after.
- **Failure keeps what is there** and waits for the next tick, as a repair
  does. No backoff beyond the period itself, which already is one.

### What the runtime has to add to a repair

- **A redirect is not a page.** A session that ended answers the refresh with
  the sign-in form, and `fetch` follows the redirect and reports success. A
  repair rarely meets that; a refresh every minute on a tab left open over a
  weekend meets it every time. `response.redirected` stops the refresh and
  leaves the page as it is, so the next deliberate click finds the sign-in.
- **Only the islands are morphed**, not the body, so a page that refreshes one
  card leaves everything else alone, including content a navigation inside
  the page already changed.
- **The timer belongs to the element.** A morph that removes the island stops
  its timer, the way the relative-time ticker stops once nothing reads it, so
  no interval survives the page it was for.

## Stages

1. **The attribute.** `refresh(period)`, the timer per element, the shared
   fetch per period, morphing by `id`, the hidden-tab rule, the catch-up on
   return, jitter and the redirect guard. Runtime tests with fake timers
   carry most of it.
2. **The server can tell.** The refresh request carries `X-Exos-Refresh` with
   the ids it wants, readable as `exos::refresh::asked()`. Two reasons, both
   real in the application that prompted this: a handler can skip the
   expensive part of a page nobody asked for this time, and a session layer
   can decline to count an unattended tab as activity, which it otherwise
   would every minute until the session expires.
3. **An action instead of the page**, only if a page turns out too expensive
   to render once a minute for one card: `every(period, handler)`, an
   element-scoped timer calling an action that answers an `Effect`. It needs
   none of stage 1's machinery but the timer and the hidden-tab rule, and it
   gives up the property that makes stage 1 attractive, which is having no
   route at all.

## What it costs

- **A whole page rendered for one card.** That is the price of having no
  endpoint, and stage 2 is how a handler that cares avoids most of it.
- **GET has to be safe.** A refresh runs the page's handler again. A page that
  writes on a GET, marking a message read when it is opened, writes again on
  every tick. It should not have been writing on a GET in the first place,
  and a refresh makes that visible.
- **Load scales with open tabs**, at one request per tab per period. Hidden
  tabs and jitter keep that honest; a period of a few seconds on a busy page
  is a choice the attribute cannot stop.

## Against the alternatives

- **A page that reloads itself** (`<meta http-equiv="refresh">`) loses scroll,
  focus and whatever was typed, and flashes. `refresh` on `<main>` is the same
  intent without any of that.
- **A live fragment on a server timer** pushes to nobody most of the time and
  has to hold the topic invariant, which per-viewer content cannot.
- **A hand-written `setInterval`** in an application is the first script it
  owns, and gets the hidden tab, the removed element and the redirect wrong
  the first time.

## Open questions

- **The name.** `refresh` says what happens; `fresh` reads as a property of the
  island, `stale_after` as a policy. Decide with the first call site.
- **A floor on the period.** One second is a page hammering its own server.
  Whether exos refuses short periods or leaves it to review.
- **Whether a page-level form is worth having**, `Page::refresh(period)`, or
  whether `refresh` on `<main>` is enough. The second needs nothing new.
- **Different periods on one page**: one fetch per distinct period, or one
  fetch at the shortest period taking every island due by then.
