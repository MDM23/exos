# Refreshing

Part of a page that reads itself again while somebody is looking at it: the
tab fetches, the server only says when.

Status: not built. Proposed 2026-11-18 from an application's monitoring page,
whose session figures are windows of fifteen minutes to a week over data no
event describes. Extended 2026-11-19 by signals, from lists whose content
depends on who reads them and how they filtered.

## Where push stops

A live fragment is pushed: something happens, the server renders once and
every tab watching gets the same patch. That needs two things, a moment to
publish at and markup that is the same for everybody watching, and plenty of
what a page shows has only one of them or neither.

- **Time is the change.** "Active in the last fifteen minutes" falls as the
  clock moves, with no write behind it. The only trigger is a timer, and a
  timer on the server runs whether or not anybody is watching, since exos
  cannot tell whether a topic has subscribers.
- **The content is the viewer's.** A list whose links depend on what the reader
  may open breaks the topic invariant: two readers would share one topic and
  receive each other's markup. Making the permission an argument works and
  multiplies topics by every distinction a view makes.
- **The content is the query's.** A list of games with a search, facets and a
  page number cannot be a topic per query, and the code that saves a game
  cannot know which queries are open. It does know that the games of a league
  changed.

Pull answers all three. The tab asks as a request, so the session, the scope,
every permission check and the query string apply exactly as they did when the
page was served. What is left for the server is saying when to ask, and for
time it does not even need to do that.

## One concept, two triggers

```rust
<section id="sessions" {refresh(every(Duration::from_secs(60)))}>
    { figures(&sessions) }
</section>

<section id="games" {refresh(on(GamesOf(league.id)))}>
    { games_table(&games) }
</section>
```

`refresh` marks an **island**: a subtree that the runtime reads again by
fetching the current URL, as a repair does, and morphing in only the element
with the same `id` from the answer. On `<main>` it is a page that refreshes
itself; on a card it is an island. What differs is only when it fetches:

- **`every(period)`** fetches on a timer in the tab. For values that change
  with time and have no event.
- **`on(signal)`** fetches when the server says the signal went stale. For
  values that have an event but whose markup is the viewer's.

```rust
#[derive(Hash)]
struct GamesOf(Uuid);

// After saving a game, anywhere:
exos::live::stale(GamesOf(game.league)).await;
```

A signal is a value naming what changed, hashed into a topic the way a
fragment's arguments are. `stale` sends no markup and renders nothing, so it
needs no locale, no scope and no lock. The page subscribes with a signed token
like a fragment's, so a tab cannot listen to a signal it was not served.
Hearing one tells it only that something changed, never what.

Both triggers can stand on one island, `refresh((on(GamesOf(id)),
every(period)))`, for a list that changes by events and by time alike.

### Which one to use

This table is the part that moves into the guide once built, beside live
fragments, because the three are easy to confuse and each is wrong for the
others' cases.

| The markup is | and it changes | use |
| --- | --- | --- |
| the same for every reader of a topic | on an event | `#[exos::live]` fragment, `publish` |
| the reader's or the query's | on an event | `refresh(on(signal))`, `stale` |
| anything | with time, no event | `refresh(every(period))` |
| one person's, pushed as an effect | on an event | `live::send` to an audience |

The first row is the cheapest when it fits: one render for any number of
tabs. A signal costs one render per tab watching, which is the price of each
tab seeing its own. Reaching for a signal because a fragment's invariant is
inconvenient, when the markup really is shared, trades that render away for
nothing.

## What is already there

The runtime can already read a page again. `repair` fetches the current URL
with the navigation header and morphs the answer in **without re-seeding**,
which is how a tab recovers after its stream dropped: the fragments come back,
and the field somebody is typing in keeps what they typed.

A refresh is a repair limited to its islands, started by a timer or a signal
instead of a reconnect. Nearly everything it has to get right, a repair
already does: the page moved on while the fetch was in flight, a response that
is not a page, focus and caret, signals that must not be reset.

## The rules the runtime keeps

- **No route of its own.** The island is found again in the page it came from,
  so there is nothing to mount, no arguments to carry back and nothing to sign
  but the signal.
- **An `id` is required**, because it is what finds the island in the answer
  and what morphing keys on anyway. A `refresh` without one is a compile error
  if the macro can see it and a console error if it cannot.
- **One fetch for everything due.** Islands due at the same moment, by timer
  or by signal, share a request and each takes its own subtree.
- **Signals fold.** An import touching five hundred games sends five hundred
  signals and a tab fetches once: a signal arriving while a fetch for it is
  pending or in flight marks it due again and nothing more.
- **Herds are spread.** One signal reaches every watching tab at the same
  moment, so each waits a random fraction of a second before asking. A timer's
  first tick lands somewhere inside the first period for the same reason.
- **A hidden tab does not fetch.** `document.hidden` skips a tick and holds a
  signal. Coming back to the tab fetches at once if either happened while it
  was hidden, so returning never shows something older than it should.
- **Failure keeps what is there** and waits for the next trigger, as a repair
  does.
- **A redirect is not a page.** A session that ended answers with the sign-in
  form, and `fetch` follows the redirect and reports success. A repair rarely
  meets that; a timer on a tab left open over a weekend meets it every time.
  `response.redirected` stops the island's refreshing and leaves the page as
  it is, so the next deliberate click finds the sign-in.
- **Only the islands are morphed**, not the body, so content a navigation
  inside the page already changed is left alone.
- **The trigger belongs to the element.** A morph that removes the island
  stops its timer and drops its signals, so nothing survives the page it was
  for.

## Stages

1. **`refresh(every(period))`.** The island, the shared fetch, morphing by
   `id`, the hidden-tab rule, jitter and the redirect guard. Runtime tests
   with fake timers carry most of it.
2. **`refresh(on(signal))` and `stale`.** Signal topics and their tokens on
   the existing stream, folding and herd spreading. With more than one
   instance, `stale` travels the same bus as `publish`.
3. **The server can tell.** The refresh request carries `X-Exos-Refresh` with
   the ids it wants, readable as `exos::refresh::asked()`. A handler can skip
   the expensive part of a page nobody asked for this time, and a session
   layer can decline to count an unattended tab as activity, which a timer
   would otherwise do every period until the session expires.

## What it costs

- **A whole page rendered for one island.** That is the price of having no
  endpoint, and stage 3 is how a handler that cares avoids most of it.
- **GET has to be safe.** A refresh runs the page's handler again. A page that
  writes on a GET, marking a message read when it is opened, writes again on
  every refresh. It should not have been writing on a GET in the first place,
  and a refresh makes that visible.
- **Load scales with open tabs**: a request per tab per period, and per tab
  per signal. Hidden tabs, folding and jitter keep that honest; a period of a
  few seconds, or a signal sent on every keystroke, is a choice the attribute
  cannot stop.

## Against the alternatives

- **A page that reloads itself** (`<meta http-equiv="refresh">`, or a signal
  that calls `location.reload()`) loses scroll, focus and whatever was typed,
  and flashes. `refresh` on `<main>` is the same intent without any of that.
- **A live fragment on a server timer** pushes to nobody most of the time and
  has to hold the topic invariant. Where the markup is shared and a few
  queries a minute cost nothing, it is fine and needs nothing new.
- **An action on a timer**, `every(period, handler)` answering an `Effect`,
  was stage 3 of the first draft. It brings back the route and the arguments
  that islands avoid, and adds a third thing to choose between. Not planned
  unless a page proves too expensive to render for one island even with
  stage 3.
- **A hand-written `setInterval`** in an application is the first script it
  owns, and gets the hidden tab, the removed element and the redirect wrong
  the first time.

## Open questions

- **The names.** `refresh`, `every`, `on` and `stale` say what happens; decide
  with the first call site. `stale` sits in `live` because it travels the
  stream, which may read oddly beside `publish` for something that pushes no
  markup.
- **A floor on the period.** One second is a page hammering its own server.
  Whether exos refuses short periods or leaves it to review.
- **Different periods on one page**: one fetch per distinct period, or one
  fetch at the shortest period taking every island due by then.
- **Signals inside fragments.** An island inside a live fragment gets its
  token from the fragment's render, which is shared. The token has to depend
  only on the signal, never on the reader, which it does if it signs the
  signal alone.
