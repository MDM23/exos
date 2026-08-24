# Observability

What a running exos application can be asked, and what it answers with silence
today.

Status: not built. Nothing in the crate emits a span, a metric or a log line.
The whole of what an operator is currently told is two `eprintln!` calls: one in
[keys.rs](../../crates/exos/src/keys.rs) when no signing key was configured, and
one in [stream.rs](../../crates/exos/src/live/stream.rs) when a resolver refused
a stream. Neither can be correlated with anything, including with itself an hour
later.

This document is about why the usual answer covers about half of exos, what the
other half needs, and which single decision has to be made before the bus in
[more than one instance](more-than-one-instance.md) is written rather than
after.

## A request-shaped tracer sees about half

A layer that opens a span per request and closes it per response is the standard
answer, and it does cover the page load: a `GET` arrives, handlers and views run
inside it, HTML goes back. Four things it does not cover, and they are the four
exos exists for.

- **A publish.** [`publish`](../../crates/exos/src/live/stream.rs) renders a
  fragment and fans it out from wherever it was called, which is frequently a
  handler and may be a background job. There is no request to hang it from, and
  the fan-out is the interesting part.
- **A delivery.** The patch lands on a tab whose own story started at a page
  load hours ago. It belongs to two causes at once and to neither exclusively.
- **A subscription.** Every DOM mutation that changes the visible fragment set
  posts `/_exos/subscribe`. It is a request, so a request tracer sees it, and
  what it sees is a `204` that says nothing about the topics silently dropped
  inside it.
- **A stream.** One `GET /_exos/live` per tab, open for hours. As a span it is
  worse than useless: it exports when it ends, so a healthy application reports
  nothing and a restart reports a flood of multi-hour spans at once.

The page load is the half that already works everywhere. The half that does not
is the half the framework is about.

## Most of the chokepoints already exist

Nothing here needs a new place to hook. Five decisions made for other reasons
put one funnel in front of each thing worth measuring.

- **One outermost layer.** [scope.rs](../../crates/exos/src/scope.rs) already
  wraps everything exos serves, the stream and the assets included, and already
  does the one thing that needs a request but does not belong to one.
- **One publish path.** `publish` holds a single process-wide lock across the
  render and the send, so every fragment that goes out passes one line of code.
- **The route template is a literal.** [route.rs](../../crates/exos-macro/src/route.rs)
  has `/files/{id}` at compile time because it generates the caller from it, so
  a low-cardinality span name is free here where axum has to recover it from
  `MatchedPath` at runtime.
- **One client request path.** [runtime.js](../../crates/exos/js/runtime.js) has
  one `request`, one `consume`, one `apply` and one `morph`, and already
  dispatches `exos:busy`, `exos:idle` and `exos:error` around every round trip.
  That is the same property that made delegated events work: nothing binds
  itself to an element, so nothing bypasses the funnel.
- **A name already reduces to a key.**
  [identity.rs](../../crates/exos/src/identity.rs) turns an audience into a
  hash, and [more than one instance](more-than-one-instance.md#the-frame)
  already decided that the reduction is what crosses a boundary rather than the
  name.
  Telemetry is the same boundary one step further out, and wants the same rule.

## Silence is deliberate, in six places, and that is the argument

exos drops things on purpose. Each of these is the right behaviour and none of
them is a bug:

- **An unverifiable topic is dropped** and the rest of the subscription applies,
  because one stale fragment left over from a previous page should not cost a
  tab its other subscriptions.
- **A lagged tab skips ahead**, through the `filter_map` on the broadcast
  stream, because a client that cannot keep up should not stall the publisher.
- **A send to a closed receiver is discarded**, because that is a tab that went
  away between the check and the send.
- **A publish nobody is watching is free and silent**, which is what lets a
  handler publish unconditionally rather than asking first.
- **A `410 GONE` is not an error.** The client tears the stream down and opens a
  fresh one, which is exactly right on one node. On two nodes with no bus it is
  the reconnect loop [more than one
  instance](more-than-one-instance.md#what-breaks-first-and-it-is-not-the-publish)
  describes, where "nothing logs an error, both nodes are behaving exactly as
  designed".
- **A resolver failure refuses the stream**, prints a line, and `EventSource`
  retries on its own, forever, at three second intervals.

Two more are not drops but land the same way. A topic renamed by a rolling
deploy stops updating "with nothing to see anywhere". An unconfigured signing
key behind a load balancer produces fragments that stop updating after a
reconnect, which "reads as a network glitch". Both are written down as known
failure modes and neither is detectable from inside a running process.

That list is the design working. A framework that shouted at a user's flaky
network would be worse than one that skips ahead quietly. But **silence is only
defensible when it is counted**, and none of these is counted. The instrument
set below is derived from that list rather than from a general wish to have
tracing.

## Stage 1: one span around the two things exos itself runs

A handler, and the resolver. That is the whole of stage 1, and it buys the
entire external-dependency answer.

exos never sees a database. [context.rs](../../crates/exos/src/context.rs) holds
whatever was provided and hands it back; the query is the application's own line
of code in the application's own connector, and `sqlx`, `tokio-postgres` and
`diesel` all emit spans of their own already. **What exos owes them is nothing
but a live parent at the moment that line runs.** With one, a slow page reads as
a handler containing a query containing a pool acquire. Without one, the same
spans arrive at the collector as orphan roots, and the connector's excellent
instrumentation is a list of queries that belong to nothing.

Three requirements, and each of them is a way to get this wrong.

- **Entered across awaits.** A guard held over an `.await` is either wrong or a
  lie depending on the executor; the span has to wrap the future. A handler that
  awaits a query is the only case there is here, so getting this wrong means
  getting nothing.
- **Nothing is added to `Scope`.** `tracing` keeps its current span in a
  task-local, and [scope.rs](../../crates/exos/src/scope.rs) keeps the request
  store in another one. They compose without exos carrying a context at all,
  which means the carrier for this is a thing that already exists in a crate
  that is not ours.
- **The fragment mask does not apply.** [`detached`](../../crates/exos/src/scope.rs)
  makes `scope()` panic inside a live fragment, because a fragment's arguments
  are its whole input and reading the request would break the topic invariant. A
  span is not an input. A fragment rendered inline sits inside the request that
  rendered it and a fragment rendered by a publish sits inside the publisher,
  and both of those are the truth about who caused it.

### What the connector gives, and the one thing it hides

Worth naming because it is the most common misattribution in a Rust service: a
span that says a query took 400ms where 390ms of it was waiting for a pool
connection. Whether the two are told apart is the connector's business and not
exos's, and an application whose connector does not separate them will read
saturation as slow SQL. exos cannot fix that and should not pretend to; what it
can do is put the handler above it so the question is at least askable.

### The resolver is the one external call exos makes itself

[identity.rs](../../crates/exos/src/identity.rs) says it outright: turning a
session name into whoever it stands for is a database call, and a database call
can fail. It runs once per connection, on the stream's `GET`, and it is the
highest-value single span in the system for a reason that has nothing to do with
its cost. It is on the critical path of every tab, nobody is waiting on it, and
when it is slow or failing the symptom is that the site is entirely fine and
nothing updates. Today that is a line on stderr.

## Stage 2: counting what is dropped on purpose

The list above, turned into numbers. No new mechanism, and this is where most of
the value is.

| What | Why it matters |
| --- | --- |
| subscriptions with a topic rejected | a rolling deploy, a stolen page, or a bug |
| stream lag events | the `CAPACITY` of 64 is a guess nobody has checked |
| publishes matching **zero** connections | see below |
| render duration, by fragment name | which fragment is expensive |
| wait for the publish lock | whether the loose end is real yet |
| connections open, opened, closed | capacity, and reconnect storms |
| resolver duration and failures | the paragraph above |

**Publishes that reached nobody is the single most diagnostic number in the
system.** A healthy application publishes into an empty room constantly, so the
raw count is noise; the ratio is not. It detects the rolling-deploy topic
rename, an unconfigured signing key behind a load balancer, and a cluster
running without a bus, which are three separately documented failure modes that
are otherwise individually invisible. It is currently documented as free and
silent, which is right about what it costs and wrong about what it knows.

The lock wait is worth its own line. [Loose
ends](loose-ends.md#publishing-scans-every-connection) says the registry walk is
the wrong shape for thousands of connections and that it is "still not worth
doing before something feels it, and nothing has". Feeling it is exactly what
this measures, and until it exists that entry stays a judgement call.

Cardinality is the rule that keeps this from becoming a bill: **a fragment name
is a label, a topic hash is not.** `presence` is one series and
`live-presence-8eb61815f6eafc86` is one series per record.

## Stage 3: the push path is not a tree

A publish reaching five thousand tabs is not a span with five thousand children,
and a delivery is not a child of the publish in any useful sense: the tab it
lands on belongs to a trace that started at a page load hours ago and has been
open ever since.

The shape that works is the one message queues settled on. A publish is **one**
span, carrying how many connections matched, with **links** rather than
children. A delivery is an event in the receiving stream's context, linked back.
This is the only stage that requires thinking about the model rather than about
where to put a call, and getting it wrong is not subtle: it is either a trace
that never ends or a span with a five-thousand-item child list.

There is a second thing this makes visible for the first time. `send`
documents an ordering guarantee, that a publish followed by a send arrives in
that order at any tab receiving both, and [more than one
instance](more-than-one-instance.md#what-ordering-costs) says that guarantee
narrows across a bus. Instrumentation is the first thing that could ever
contradict either claim.

## Stage 4: the browser half, on a 7 KB budget

The constraint decides the design. [Loose
ends](loose-ends.md#morphing-stays-in-house) measured the release bundle at
21,617 B raw and 7,046 B gzipped and rejected a library that would have deleted
3 KB of hand-written morph code, over +2.3 KB gzipped. An OpenTelemetry
browser SDK is two orders of magnitude past that. **The client half is not an
SDK, in any version of this.**

What it is instead: the events that already exist, plus a correlation id. And
the id goes the opposite way round from the usual RUM model. In exos the server
decides everything, so **the server names the trace and the browser attaches to
the name it was given**, which needs no sampler, no context propagation and no
id generation on the client.

Where the name fits is the one place the wire format has no free slot, and it is
worth being precise about:

- **An action's answer is an ordinary `fetch` response**, so a `traceparent`
  response header is free and adds nothing to the step vocabulary.
- **The stream has no per-event header.** SSE's `id:` field is spoken for by
  `Last-Event-ID` and taking it would change what a reconnect sends. A comment
  line is invisible to `EventSource`, and the keep-alive already uses those. So
  a push either gets a step of its own or gets nothing, and a new step name is a
  string rather than a format version, which loose ends already established when
  the stream grew the other four.

What the browser can then answer that no server can: the time from a patch
arriving to the DOM actually changing, how often an optimistic paint had to be
corrected by the patch that followed it, and whether a slow action was slow on
the wire or slow in the morph. The first and the third are what an application
would otherwise buy a vendor for. The second does not exist as a product
anywhere, because no other framework knows which paints were speculative.

## Stage 5: the bus, and one field that has to be decided before it exists

This is the entry that has a deadline.

[More than one instance](more-than-one-instance.md#the-frame) defines
`Frame { kind, key, steps }`, insists the codec is exos's rather than the
caller's, says two nodes disagreeing about it is "a cluster that looks connected
and delivers nothing", and asks for a golden test on the exact bytes. All of
that is right, and all of it means a trace context on the frame is a codec
change, and a codec change halfway through a rolling deploy is the precise
failure that document exists to prevent.

**So the field costs nothing today and cannot be added quietly later**, which is
what the README already says about the bus itself, one level in.

For it: the cross-node delivery lag is exactly the number the local-first
default trades away, and it is unmeasurable without a context on the frame.
Against it: the frame deliberately carries nothing that identifies anybody, and
this is a new field on every message. The resolution is that a trace context
identifies a trace and not a person, and an unsampled frame carries an empty
one, so the rule that a broker's operator, logs and backups never hold anything
that logs anybody in survives intact.

## What must never reach a span

The same argument the bus document makes about a broker, applied to the store
with the loosest access control in the building and the longest retention.

- **A connection id.** 128 unguessable bits, and a bearer name: whoever holds
  one can replace what that connection watches. In a log it is a credential with
  a one-year retention policy.
- **A session name.** The cookie itself. Same class, worse blast radius.
- **What crosses instead** is the reduction that already exists for the bus,
  which is the second use for that field and the one that pays for it.
- **An audience key** is already a hash and is safe to carry, with one thing
  said out loud rather than discovered: it is stable, so it is a pseudonym that
  joins one person's traces across sessions, tabs and machines. That is exactly
  what makes it useful in an incident and exactly what a privacy review will
  ask about.
- **A topic hash** is public, since it is an id in the DOM. It is also derived
  from a fragment's arguments, so it names a record. Names go on metrics, hashes
  go on spans, and the guide has to say so or somebody will label a counter with
  one and find out from a bill.

## What must not be built

- **A second `tracing`.** An `exos::observe(|event| ..)` hook, the shape
  [`bus`](more-than-one-instance.md#stage-1-a-bus-is-two-functions-and-no-dependency)
  takes, is the wrong analogy and it is tempting for exactly the reasons that
  usually hold here. A broker is a transport whose choice is the application's.
  A span format is a lingua franca whose entire value is that the connector, the
  HTTP client and the runtime already speak it, and a bespoke hook cannot join
  up with any of them. **This is the one place the crate's no-dependency reflex
  is wrong**, and it is worth writing down because it points the other way
  everywhere else. The arithmetic is small: `tracing` brings `tracing-core`, and
  `log`, `once_cell` and `pin-project-lite` are already in the lockfile, so it
  is two crates, and a span with no subscriber installed is an atomic load and a
  branch.
- **A metrics endpoint.** exos hands back an `axum::Router`, so `/metrics` is a
  route the application adds. No exporter, no format, no opinion.
- **A span per delivery**, per the shape in stage 3.
- **A log format, a level convention, or anything that assumes stdout.**
- **Timing a push by subtracting two clocks.** The one-way delay between a
  server and a browser is not measurable without clock sync, and a number
  computed by subtracting a browser clock from a server clock is skew wearing a
  latency's name. What is measurable is server-side enqueue and client-side
  apply, correlated by id and never subtracted.

## What it costs

- **A dependency**, and it would be the first one added for something other than
  a capability. Gating it behind a feature means the ungated build has to keep
  working, which is a CI matrix entry and a second thing to break.
- **A decision on the frame codec before the bus exists**, per stage 5.
- **Attribute discipline forever.** One line added at 3am by somebody debugging
  a connection puts a bearer token into a log store, and nothing in the type
  system stops it. That is the whole argument for the reduction being the only
  thing a connection can be named by from inside the crate: not because naming
  it correctly is hard, but because the correct name should be the only one
  reachable.

## Testing

- **A recorded span carries no connection id and no session name.** Writable as
  a subscriber that walks attributes, and it is the one rule here that cannot be
  reviewed reliably by eye.
- **A handler's span is still current after an await**, which is the mistake a
  guard makes, and which a test catches once and forever.
- **A fragment rendered by a publish is inside the publisher's span** and inside
  no viewer's, which is the tracing shape of the invariant
  [live.rs](../../crates/exos/src/live.rs) already enforces.
- **A build with the feature off compiles, links, and puts the same bytes on the
  wire**, byte for byte, which is the same test the bus document asks for when
  no bus is registered.
- **The client half does not move the bundle past its budget**, as a number
  checked in CI rather than a habit.

## Open questions

- **Feature or dependency.** A feature is two builds to keep working; a
  dependency is a crate every application pays for and most never look at.
- **Whether the frame carries a trace context**, and what an absent one means:
  unsampled, or an older node. The two need telling apart, and the frame has no
  version.
- **Whether a stream is a span at all**, or an open event, a close event and a
  gauge between them. The second is almost certainly right and the first is what
  everybody writes.
- **Whether a topic hash may be a span attribute**, given that it names a
  record. It is already in the DOM, which is an argument, and a trace store is
  not a browser, which is the other one.
- **Whether exos says anything at all about the client half**, or ships the
  events it already has and lets the application decide where they go.
