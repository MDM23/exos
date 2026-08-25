# More than one instance

What a second process breaks, and the one piece of state that cannot be moved.

Status: stages 1 and 3 are built. A [`Frame`](../../crates/exos/src/live/bus.rs)
crosses, `exos::bus` says how, `exos::deliver` is what an application's
subscriber hands one back to, and a publish and a send fan out through it
local-first. What is **not** built is stage 2, which is what breaks first: a
subscription that lands on a node holding no such connection still answers
`410`, so a browser round-robined between two nodes still cannot settle. Two
instances are therefore not yet a deployment, and the section below says why
that is the ranking rather than the publish.

The README calls this the one gap that cannot be added quietly later, and this
document is why: it is not a feature beside the others but a set of guarantees
that shrink, and the shrinking has to be decided deliberately rather than
discovered by whoever runs two nodes first.

## Most of exos is already ready, by holding nothing

Three decisions made for other reasons turn out to be the whole of the easy
half.

- **A session is a name in a signed cookie and no store**, which [sessions and
  identity](sessions-and-identity.md#stage-3-and-no-store-at-all) argued for on
  its own merits. Every node reads the same cookie and learns the same thing,
  so there is nothing to replicate and nothing to expire.
- **A topic is named the same way by every build**, fixed in
  [fnv.rs](../../crates/exos/src/fnv.rs) for a rolling deploy, which is the same
  requirement one step smaller: two binaries have to agree on what a fragment is
  called without either being told.
- **A live token is an HMAC under a configured key**, so a token minted by one
  node verifies on another. [keys.rs](../../crates/exos/src/keys.rs) already
  says what happens when the key is not configured, and names a load balancer as
  the case it is wrong for.

Identity is resolved per connection by the application's own resolver, and every
node runs the same one. So the state that has to cross is exactly one thing: the
connection registry in
[stream.rs](../../crates/exos/src/live/stream.rs), a `HashMap` of local sockets.

And it is the one thing that cannot be shared, because a socket belongs to the
process holding it. **The thing to distribute is therefore not the registry, it
is the message.** Every node keeps its own map of its own connections, and the
only new question is how a message reaches a map it is not in.

## What breaks first, and it is not the publish

A tab holds one long-lived stream to one node, and every other request it makes
goes wherever the load balancer sends it. `/_exos/subscribe` names a connection
id, the registry that holds it is local, so a subscription that lands on any
other node answers `410 GONE`. The client does the right thing with that answer,
in [runtime.js](../../crates/exos/js/runtime.js): the server has forgotten us,
so it tears the stream down and opens a fresh one.

Round-robin two nodes and that is a loop. The new stream lands somewhere, the
next subscription lands elsewhere, and the tab reconnects on roughly every DOM
mutation, each time paying the reconnect repair's full page fetch. Nothing logs
an error, both nodes are behaving exactly as designed, and the symptom is a
browser that will not settle.

It is ranked first because it costs a single node nothing and is a broken
deployment on the second one, before any publish has been missed.

## The frame

One shape crosses, and it is small.

```rust
pub struct Frame {
    kind: Kind,
    key: String,
    steps: Vec<(String, String)>,
}
```

The `key` is what a connection is matched on, and both sets in the registry
already hold exactly that: `topics` and `audiences` are `HashSet<String>`
produced by the one reduction in
[identity.rs](../../crates/exos/src/identity.rs). So the receiving node's
delivery code is the code that exists today, walking connections and matching a
key against a set.

`Kind` is what keeps the two sets apart on the wire, for precisely the reason
they are two fields rather than one: merged, whether a key was proved or derived
would depend on a check nobody can see, and the first frame that forgot it would
let a tab be addressed as somebody.

The steps cross **already framed**, as the event name and payload pairs
[effect.rs](../../crates/exos/src/effect.rs) produces for the wire. Three things
fall out of that, and together they are the argument for it.

- Nothing gains a `Serialize` derive. `Step` holds `Markup` and a JSON value and
  is deliberately `#[non_exhaustive]`; the framed pair is two strings.
- The receiving node does not need to know what a step is. It pushes what it was
  handed into a broadcast channel, and the browser reads the same bytes it would
  have read from the node that sent it.
- A new step name is a new string rather than a new frame version, so adding one
  does not divide a cluster mid-deploy.

What a frame never carries: no session name, no connection id in the clear, no
fragment arguments, no application state, no token. A key is already a hash, and
the two things that would otherwise have to cross and are bearer secrets, a
session name and a connection id, cross as the same reduction rather than as
themselves. The cost is one extra field on the connection, holding the reduction
next to the value. The gain is that a broker's operator, its logs and its
backups never hold anything that logs anybody in.

## Stage 1: a bus is two functions and no dependency

**Done.**

exos ships no broker adapter, the same way it ships no session store, and for
the same reason: the choice is the application's and exos would learn nothing by
being told. What it ships is the two ends.

```rust
exos::bus(move |frame: exos::Frame| {
    let redis = redis.clone();

    async move {
        redis.publish("exos", frame.to_bytes()).await?;
        Ok(())
    }
});

// The application owns its own subscriber loop, and its reconnection.
tokio::spawn(async move {
    while let Some(message) = subscription.next().await {
        if let Some(frame) = exos::Frame::from_bytes(message.payload()) {
            exos::deliver(frame);
        }
    }
});
```

`deliver` is synchronous, because delivery is a registry walk and a channel
send, which is what [`publish`](../../crates/exos/src/live/stream.rs) and `send`
already are.

The codec is exos's rather than the caller's. A frame crosses between two
binaries, so how it is spelled is part of the format and not a preference, and
two nodes configured with different opinions about it would be a cluster that
looks connected and delivers nothing. That makes the encoding worth a golden
test rather than a round trip, for the same reason the topic hash has one.

**With no bus registered, nothing above exists at runtime.** No task is spawned,
no frame is built, and single-node delivery is the code path it is today. That
is not a courtesy to small deployments; it is what keeps the cluster path from
becoming a thing every application pays for.

## Stage 2: a subscription reaches the connection it names

The loop above, closed. The node that receives the request has the cookie, so
`Topic::verify` works there exactly as it does now: verify locally, then forward
the proved topic names. No token crosses and nothing is verified twice.

What the receiving node needs is the difference between *gone* and *not mine*,
and it cannot currently tell them apart. So the connection id gains a prefix
naming the node that minted it, keeping its 128 unguessable bits behind it.
Mine and unknown is `410`, and the client's reconnect is right. Not mine is a
forward and a `204`. Without the prefix every answer is a guess, and the guess
that says `GONE` tears down a stream that was fine.

The frame is keyed by the reduction of the connection id rather than by the id,
so this is a `Kind` and not an exception to the rule above.

Worth naming what this buys beyond the bug: **no sticky sessions**. An action
POST already works on any node, because a handler reads a cookie and publishes
through the bus like anything else, so the subscription is the only request in
exos that names a connection. Closing it is the difference between a framework
that runs behind an ordinary load balancer and one that quietly shapes a
deployment around itself.

## Stage 3: publish and send fan out

**Done**, local-first.

`publish` renders locally, once, and the frame carries the rendered patch.

**The wire carries the result, not the request.** A frame saying "re-render
topic X" is not something a receiving node could honour: a topic is a hash of a
name and its arguments and nothing can invoke the function from it. That is the
wall
[directed effects](directed-effects.md#stage-5-when-nobody-is-listening)
hits, and the one the reconnect repair went around from the client. Rendering
once for the whole cluster is also simply cheaper, and it is the only shape
available.

`send` is the same frame with a different `Kind`, and needs no thought beyond
that. It is the feature that wanted a cluster in the first place: a person with
tabs on three nodes is one `send` and three deliveries.

### What ordering costs

This is where a guarantee shrinks, and it should be written down rather than
noticed.

Today `publish` holds a lock across the render and the send, so the last patch a
tab receives for a topic is the newest one. That is a fact about serializing two
operations, and across processes there is nothing serializing them. A
cluster-wide lock is the wrong answer and worth rejecting explicitly: a node
that takes it and dies stalls that topic for everybody until something times
out, which trades a stale patch for an outage.

Two shapes, and they differ in where the serialization point is.

- **Local-first.** Deliver to local connections as today, then hand the frame to
  the bus. Fast, and it degrades to single-node behaviour when the broker is
  unreachable. The order two nodes' patches arrive in at a third is whatever the
  broker gives.
- **Loop-back.** Send nothing locally; the frame comes back through `deliver`
  and every node including the sender applies it in the order the bus produced.
  This restores the exact guarantee, on a broker that orders per key, which
  means partitioning by key: Kafka does, a single Redis does, NATS and Redis
  Cluster do not promise it. It costs a round trip before a publisher's own tabs
  see anything, and a broker hiccup then stops delivery even to the tab on the
  same machine.

**Local-first is the default.** A bus outage should cost a cluster its
cross-node liveness and not its liveness, and the failure that shape gives is
the one exos already survives. Loop-back stays available for a bus that promises
per-key order, and how that is declared is an open question below; what it must
not be is a boolean argument.

The other guarantee that shrinks is smaller and easier to miss. `send`'s
documentation promises that a publish followed by a send arrives in that order
at any tab receiving both, because one connection has one channel and both send
under the registry lock. Across a bus those are two keys and therefore two
orders, so it holds for a local connection and not for a remote one.

### What building the two ends found

**An adapter is a closure answering with a future, not an async closure.** This
document drew `bus(async |frame| ...)` and it does not compile against a
spawned send: the future an async closure returns borrows what the closure
captured, so it is neither `'static` nor provably `Send`, and the bound that
would say otherwise names an associated type stable Rust cannot. The shape that
works is the ordinary one, cloning the client into the future, which is what
every broker client is cheap to clone for. It is written down in the guide and
pinned by the test bus being written the same way.

**A frame is spawned onto a runtime, so registering names one.** A publish is
synchronous and an adapter is not, and a sync callback would be worse than the
plumbing: whatever it did with the frame would happen while `publish` holds the
ordering lock, so a network write there would serialize every publish in the
process behind it. `bus` therefore takes the handle at registration and panics
outside a runtime, and a send prefers the runtime it is publishing from, which
is what lets a test with a runtime per test register once.

**The receiving node's delivery is the sending node's, exactly as drawn.** The
walk over the registry became one function taking a kind, a key and the framed
steps, and `publish`, `send` and `deliver` are its three callers. That was the
whole of stage 3 beyond the frame: no second delivery path, and nothing to keep
in step between local and remote.

**The unconfigured key is now fatal rather than a warning**, which
["what a rolling deploy costs"](#what-a-rolling-deploy-costs) below called the
one entry that was work. `exos::bus` refuses to register without
[`exos::keys`](../../crates/exos/src/keys.rs), and it needs a flag of its own to
know: by the time anything asks, the fallback has usually filled the key in, so
what is recorded is whether an application said it rather than whether one is
there.

**The trace field is on the frame from the first version**, empty until
[observability](observability.md) has something to put in it. That document
calls this the entry with a deadline, and the golden test now pins the bytes it
is part of.

**Two nodes remain untestable in one process**, as this document said. What the
tests cover is the two ends: the codec as a golden value, a publish and a send
handing over a frame carrying what a local tab received, and a frame handed
back reaching the connections its kind and key name and no others. The
topology is what a broker adds.

## Stage 4: a rotation crosses too

Not optional once stage 3 lands, which is the reason it is a stage.

`rotate` and `end` in [session.rs](../../crates/exos/src/session.rs) call
`disconnect`, which walks the local registry and ends every stream that opened
under the old name. That is what stops a renamed browser from carrying its old
identity, and it reaches one process.

So: a browser with tabs on two nodes signs out on node A. The tab on node B
keeps streaming as the identity that just signed out, and once stage 3 exists,
any node's `send` reaches it. It stays that way until the stream drops on its
own, which for a tab left open is never.

A third `Kind`, keyed by the reduction of the session name, and `disconnect`
becomes local delivery of a frame every node produces the same way. The bus is
as much about revocation as about delivery, and this is the entry that says so.

## Stage 5: the broker does the filtering

Every node receiving every frame and discarding most of them is invisible at
presence volumes and is the wrong shape for a large one. It is the same shape as
the registry walk in
[loose ends](loose-ends.md#publishing-scans-every-connection),
and the two want the same bookkeeping: the index from key to connection ids is
exactly the set of subjects a node has a reason to subscribe to. A node
subscribes to a key when its first local connection watches it and drops the
subscription with the last one.

It is last because it needs that index, and because a broker with per-subject
subscription is a stronger requirement than one with a channel, so making it the
starting point would narrow what can be plugged in for a cost nothing has felt.

## What must not be built

- **A broker in the dependency tree.** The trait surface is two functions
  precisely so the answer to "which one" is never exos's.
- **A distributed lock**, for the reason under stage 3.
- **A replicated registry.** A connection is a socket. What would be shared is a
  list of names of things another process holds, and every use of it would be a
  question that has to be asked over the network anyway.
- **Cross-node re-rendering**, which is not available at any price until a topic
  can be re-invoked from its name.
- **Durability, replay or an outbox.** The rule the whole push design rests on
  is that a directed effect is an accelerator for state the server already
  persisted, so a lost frame is a case that already exists and is already
  answered by the next page load. A bus that guaranteed delivery would be a
  second, weaker copy of the database.

That last one has a corollary worth stating, because it decides what an adapter
may do: **at-least-once redelivery is safe for a patch and not for an effect.**
A patch is state replacement and applying it twice changes nothing. A toast
delivered twice is two toasts. An adapter that retries is therefore making a
choice about the second kind, and the honest default is not to retry.

## `connected` is the one call that stops being answerable

It answers from the local registry, so in a cluster it answers "connected here",
which is not the question it exists for. Three shapes are available: keep it
local, which is useless for choosing between a push and an email; ask every node
and wait, which needs a request-response channel, a timeout, and gives a wrong
answer whenever a node is slow; or a presence set the nodes write their audience
keys into under a heartbeat, where a node that dies leaves ghosts until its TTL
expires.

What rescues it is its own documentation. It already says it is a hint and never
a guarantee, because the last tab can close between the answer and whatever is
done about it. A TTL-stale answer is the same class of wrong it already admits
to, so the presence set needs no new promise and is the only one of the three
that answers the question asked.

It has to become async, which is a breaking change to public API and is the
reason this is named here rather than left for whoever runs into it.

Rejected: `send` returning how many connections it reached. Across a bus that
number arrives after the decision needed it, so it would be a local count
wearing a cluster's name.

## What a rolling deploy costs

Two of these are ops facts rather than work, and they belong here because the
first person to hit them will be reading this page.

- **A renamed topic divides a cluster.** The hash is stable across builds, and
  what is not stable is a change to a fragment argument's own `Hash`, which
  renames every topic it appears in. On one node that is a deploy that drops its
  documents. Across a rollover it is quieter: a tab served by an old node keeps
  the old name, new nodes publish the new one, and the fragment stops updating
  with nothing to see anywhere. The mitigation is to drain connections during
  the rollover. `Audience::NAME` and the resolver have the same property with a
  smaller blast radius.
- **A hashed asset URL is served by the binary that embedded it.** A page served
  by a new node asks for a file an old node does not have. Every content-hashed
  deployment has this, and solves it by keeping both versions reachable for the
  length of the rollover.

And one that was work and is **done**: **with a bus registered, an unconfigured
signing key is fatal.** Without one it is a random key per process and a line on
stderr, which is right for `cargo run`. Behind a load balancer a token minted by
one node verifies nowhere else, and the symptom is fragments that stop updating
after a reconnect, which reads as a network glitch and is the exact failure the
FNV entry in [loose ends](loose-ends.md) already went to trouble to eliminate.
Registering a bus is the moment exos can know this rather than warn about it, so
`exos::keys` goes first and `exos::bus` says so if it did not.

## What it costs

- **A public surface that is a hole rather than a feature.** Whoever runs a
  cluster writes and operates the adapter, and exos is then only as reliable as
  a piece of code it never sees. That is the right trade and it is still a
  trade.
- **`connected` becomes async**, and that is a breaking change for everyone,
  including applications that will only ever run one node.
- **Two ordering guarantees narrow**, per stage 3, and the documentation on
  `publish` and `send` has to say which half of each still holds.
- **Testing gets structurally harder.** The registry, the keys and the resolver
  are all process-global, so no test process can hold two nodes. Either the fake
  bus loops frames back into one process, which proves the frame and not the
  topology, or those statics become values a process holds one of, which is a
  refactor nothing else is asking for and which this should not smuggle in.

## Testing

- **A frame encodes to the same bytes in every build**, as a golden value rather
  than a round trip, since a round trip passes against a format that drifts.
- **A frame delivered as if from elsewhere reaches a local connection watching
  its key**, and no other connection.
- **A topic frame does not reach an audience and an audience frame does not
  reach a topic**, which is the separation the registry tests already assert,
  one level further out.
- **A subscription for a connection this node does not own is forwarded**, and
  one for a connection it does own and has never heard of is still refused.
- **A rotation frame ends the local streams that opened under the name**, and
  leaves every other browser alone, which is the existing test with the name
  arriving over the wire.
- **With no bus registered nothing crosses**, no task is spawned, and the
  single-node wire is byte for byte what it is today.

## Open questions

- **How loop-back is chosen**, given that it is a property of the broker rather
  than a preference. An associated constant on a bus trait, a second
  registration function, or the frame carrying its own sequence and the receiver
  dropping what it has already passed. Not a boolean argument.
- **Whether the node prefix on a connection id is plain or hashed.** Plain tells
  a client roughly how many nodes there are, which most deployments already
  announce in a header; hashed costs a lookup on every forward.
- **Whether `deliver` is the application's call at all**, or whether exos should
  take a stream of frames at registration and drain it. The closure is smaller
  and the stream is harder to misuse.
- **Whether `connected` gets the presence set**, or whether the answer is that
  it stays local and says so in its own documentation, leaving the push-or-email
  decision to an application that knows its own users.
