# Directed effects

Pushing an `Effect` to a person, rather than a patch to a screen region.

Status: design. Nothing here is implemented. It depends on an identity system
exos does not have, planned in
[sessions and identity](sessions-and-identity.md), and much of this document is
about what that system has to be for the rest to hold together.

## What it is for

A notification arrives. Every tab its recipient has open should show a new
badge count and a new row in the notification sidebar, and the tab in front of
them should say something happened. Nobody else sees any of it.

The pieces to build it are almost all there. The stream carries server-sent
events, `Effect` already serialises to exactly those events, and the client
already has one parser for both. What is missing is an answer to a single
question: **who**.

## Where the topic model stops

A live fragment is addressed by what is on screen. The client reports the
fragments it is displaying, the server matches a publish against that set, and
a patch goes to whoever is watching. That is the right shape for state, and it
is why presence dots need no bookkeeping.

It cannot express three things a notification needs.

- **Reaching a tab that is not displaying the fragment.** Delivery is
  conditioned on the subscription, and the subscription is derived from the
  DOM. A user reading a document with no sidebar on screen is not addressable.
- **Carrying anything but a patch.** [`publish`](../../crates/exos/src/live/stream.rs)
  takes a `Fragment` and sends one `Step::Patch`. A signal merge, a scroll, a
  navigation, none of them have a way down the stream.
- **Being transient.** A patch is state replacement: send it twice and nothing
  happens the second time. "You were mentioned" is an event. It has no
  fragment whose re-render produces it, and no correct second delivery.

## Per-viewer fragments are not the contradiction they look like

The invariant in [live.rs](../../crates/exos/src/live.rs) is that a topic
completely determines its content. `notification_count(user)` satisfies it: the
user id is in the topic, so the same topic still means the same HTML for
everybody who can subscribe, and that set has one member. The guide already
names this as the escape hatch, and it is the right one.

What is actually broken about it today is the proof. A token is derived from
the topic id and a process secret, so it proves *somebody* was served the
fragment, not that *this viewer* was. Anywhere a topic id and token escape a
page, by a screenshot, a copied DOM, a log line, a shared browser profile, the
holder can subscribe to another user's notifications and keep receiving them
for the life of the process. The README lists this under known gaps; it stops
being theoretical the moment fragments carry per-user content.

So identity is not only what unlocks directed effects. It is also what makes
per-viewer fragments safe, by letting a subscription be checked against who is
asking rather than only against what they can prove they once saw.

## Two mechanisms, kept apart

|                       | live fragment                          | directed effect                       |
| --------------------- | -------------------------------------- | ------------------------------------- |
| addressed by          | topic: a name and its arguments        | audience: who the connection is       |
| content decided by    | the fragment function                  | the sender, at the call site          |
| delivered while       | it is on screen                        | a stream is open                      |
| missed by the client  | repaired by the next publish or load   | lost                                  |
| authorized by         | a token proving it was served          | the sender choosing the recipient     |
| use it for            | state that is visible                  | events, alerts, per-viewer nudges     |

The row that matters is the last but one. A fragment's authorization is
structural, and exos can guarantee it because the client names the topic and
must prove it. A directed effect inverts that: the server names the recipient,
so exos can only guarantee delivery to connections whose identity matched. It
cannot know whether that person should see that content. **That check moves
into the application, at the send site**, and the API should not pretend
otherwise.

Note also what a handler's return value already is. An `Effect` answering a
request reaches exactly the tab that made it, with no identity involved at all.
Directed effects are for out of band only: a background job, a timer, another
user's action.

## Stage 1: identity on the connection

The stream is opened with an ordinary `GET`, so it carries cookies. That is the
one place identity can be established without inventing a second channel.

```rust
#[derive(Hash)]
struct Viewer(u32);

impl exos::Audience for Viewer {
    const NAME: &'static str = "viewer";
}

exos::identify(|session| match session.get::<Principal>() {
    Ok(Some(who)) => Audiences::of(Viewer(who.id)).and(Team(who.team)),
    _ => Audiences::none(),
});
```

`Audience` is a trait, not a string and not a macro. An implementor is any
`Hash` type, and `Topic::new(Self::NAME, self)` reduces it to the same shape a
fragment topic has, which is what lets one registry and one match loop serve
both. A `#[derive(Audience)]` filling in `NAME` from the type name is obvious
sugar and can come later.

Returning a set rather than one value is deliberate. It costs nothing and it is
the difference between addressing a user and addressing every admin, everyone
on a team, or every tab in a workspace.

The resolver is a pure function of what the session already holds, because the
framework loads it once on the stream's `GET`. It is neither async nor fallible
for that reason, and an application that wants a lookup per connection should
do it when it writes the session instead.

Three consequences to build in from the start:

- **The client can never name an audience.** Audiences live in their own set on
  the `Connection`, written only by the server at connect. They must not share
  the `topics` set that `/_exos/subscribe` overwrites, even though both hold
  the same kind of string. One field is client-claimed and token-proved; the
  other is server-derived and unforgeable. Merging them would make the
  distinction depend on a token check nobody can see from the type.
- **The connection id should be minted by the server.** Today the client
  generates a UUID and `/_exos/subscribe` trusts it, so guessing one lets an
  attacker overwrite another tab's subscriptions. Unguessable in practice, and
  still the wrong shape once connections carry identity. Mint it on the server,
  send it as the first event on the stream, and have the client use what it was
  given.
- **Identity is captured at connect and never refreshed.** That is a leak on a
  session change: the runtime morphs the body on navigation without reopening
  the `EventSource`, so a tab that logs in as somebody else keeps the previous
  audience. Either sign-in and sign-out answer with `Effect::reload()`, which
  drops the stream, or the stream gains a `reconnect` step that closes and
  reopens it without a document load. The second is better and either must be
  documented as a rule, not left to be discovered.

## Stage 2: the request scope

Rendering `notification_count(id)` three components deep should not mean
threading a user id through every caller above it. That is the same argument
[context.rs](../../crates/exos/src/context.rs) makes for `data`, and the same
answer applies, except per request rather than per process. It is a task-local
set by a layer, and it belongs to
[sessions and identity](sessions-and-identity.md), which needs it first and
where it is designed.

One decision that belongs here rather than there: whether `#[exos::live]`
should fold the current viewer into a topic automatically, so that a per-user
fragment cannot be written without one. It is tempting and it is wrong. A
fragment whose topic depends on ambient state renders differently depending on
where it is called from, which is exactly the property the topic model exists
to prevent, and it would be unrenderable from the background job that publishes
it. The other document goes further and makes the session unreachable inside a
fragment on purpose.

## Stage 3: sending

```rust
exos::send(&Viewer(user), &Effect::signals(json!({ "toast": summary })));
```

Every step becomes one event, exactly as `publish` sends one. Delivery is
best-effort fan-out to every open connection carrying that audience, which is
zero, one, or a tab per device.

The sender usually wants to know whether anyone is there, since the user's own
framing was that the server figures out who is connected:

```rust
if exos::connected(&Viewer(user)) { /* push */ } else { /* email */ }
```

That answer is a race by nature, so it is a hint, not a guarantee. The honest
version of it is the rule in [stage 5](#stage-5-when-nobody-is-listening):
persist first, push second.

What is guaranteed is order, per connection. A connection has one channel and
sends happen under the registry lock, so a `publish` followed by a `send`
arrives in that order at every tab that gets both. Across connections nothing
is ordered, and nothing should be made to be.

### Client changes

The runtime listens for five step names on the stream:

```js
for (const step of ["navigate", "page", "patch", "remove", "signals"]) {
```

`focus`, `reload` and `scroll` are missing, which is fine while the stream only
carries patches and becomes arbitrary once it carries effects. Either register
all of them or make the exclusion deliberate and say why in the comment.
`focus` and `scroll` from a background job are rude, but they are rude in
exactly the way an application chooses; a framework refusing to deliver them is
a surprise, and one that shows up as silence.

## Stage 4: things that are not state

The toast in the opening example is not a fragment, and it should not become
one. Two ways to say it, and they are not rivals.

**Signals.** `Effect::signals` merges into the client store, so the sidebar's
template already owns the presentation and the server sends a value:

```rust
Effect::signals(json!({ "toast": "Ada mentioned you in Q3 planning" }))
```

This works with no new step. One caveat to document: signals arriving on the
stream merge into the global namespace, since there is no element to resolve a
scope against, so the receiving template has to read an unscoped name.

**A custom event.** `Step::Event(name, payload)`, dispatched on `document` the
way `exos:busy`, `exos:idle` and `exos:mutated` already are, is the general
escape hatch: plugins and application scripts get somewhere to hang behaviour
that is not a DOM update, and the stream stops needing a new step per idea.

Worth knowing before writing it: a document-level event does not reach
`data-on-*` handlers, because delegation resolves the target with
`ev.target.closest`. Making it reach one means letting the step carry a
selector. Start without it.

## Stage 5: when nobody is listening

The rule the whole design rests on:

> A directed effect is an accelerator for state the server already persisted.
> It is never the record of what happened.

Three ways a push is lost, none of them fixable by trying harder.

- **No connection.** The recipient is offline. The notification exists in the
  store and their next page load renders it.
- **A slow tab.** The broadcast channel holds 64 messages and a lagging
  receiver skips ahead. Correct for patches, which the next publish repairs,
  and lossy for events, which it does not.
- **The reconnect gap.** `EventSource` reconnects on its own, the server has
  forgotten the connection, and the client re-subscribes. Anything published in
  between is gone.

The third one is already a bug for live fragments and directed effects make it
visible: a fragment that changed during the gap stays stale until the next
publish, which may be never. The server cannot repair it on its own, because a
topic is a hash of a name and its arguments and nothing can re-invoke the
function from it.

The cheap fix is on the client. On a reopen that follows a drop, rather than on
the first open, re-fetch the current URL and morph, which is one call to
`navigate(location.href, false)` and repairs everything state-backed in one
request.

The expensive fix is to make topics re-renderable: `#[exos::live]` registers a
renderer by name through `inventory`, the wrapper element carries its signed
arguments, and the server can then re-render any topic on demand. That also
buys `publish_topic` without a call site that knows the arguments, and
rendering for a late subscriber. It is a larger change than this document and
should not be smuggled into it, but it is the thing directed effects will keep
pointing at.

## What it costs

`publish` takes a `Mutex` over the whole registry and walks every connection.
At presence volumes that is invisible. One send per notification against
thousands of connections is a different shape, and directed effects will be the
first thing to feel it.

The fix is an index from topic to connection ids, maintained on subscribe and
on close, so both `publish` and `send` become a lookup rather than a scan. Two
constraints on whoever writes it: `await_holding_lock` is on, so the send path
stays synchronous, which `broadcast::Sender::send` allows; and the index has to
be dropped in `close` or it outlives the connections it names.

## The notification centre, end to end

```rust
#[exos::live]
fn notification_count(user: u32) -> Markup {
    let unread = data::<Notifications>().unread(user);
    view! { <span class="badge" data-unread={ unread > 0 }>{ unread }</span> }
}

#[exos::live]
fn notification_sidebar(user: u32) -> Markup {
    let items = data::<Notifications>().recent(user);
    view! { <ul class="notifications">{ items.iter().map(item).collect::<Vec<_>>() }</ul> }
}

fn notify(user: u32, event: &Event) {
    // Durable first. Everything below is an accelerator.
    data::<Notifications>().record(user, event);

    // State, to whichever tabs are showing it.
    publish(&notification_count(user));
    publish(&notification_sidebar(user));

    // The arrival, to the person.
    exos::send(&Viewer(user), &Effect::signals(json!({ "toast": event.summary() })));
}
```

The badge sits in the layout, so it is on every page and every tab of that user
gets it from an ordinary publish. The sidebar patch reaches the tabs that have
it open, and the ones that do not will render it fresh when they navigate. Only
the third line needs anything new, and it needs it because a toast is addressed
to a person and has no correct second delivery.

That is the summary of the whole design. Most of a notification centre is
already expressible; identity is what makes it safe, and directed effects are
what the remaining line needs.

## Open questions

- **Naming.** `send` reads well next to `publish` and says little. `deliver`
  and `dispatch` are the alternatives, and `dispatch` already means something
  in the client runtime.
- **Every tab, or one?** A toast in six tabs is six toasts. Delivering to the
  focused tab needs the client to report focus, which is state the server does
  not otherwise keep. Deliver to all and let the page decide, at least first.
- **Anonymous visitors.** An empty audience set is the obvious answer, and a
  session-scoped audience for logged-out users is a real use (a queue position,
  a checkout timer). It should fall out of the resolver rather than be a case.
- **Should `Audience` be sealed?** No, applications implement it. That makes
  `NAME` collisions their problem, which the derive would solve for them.
- **Rate limiting.** Nothing here stops a job from sending a thousand effects
  to one connection and pushing everything else out of a 64-slot channel.
- **More than one instance.** The registry is a process-local `HashMap`, so a
  send reaches only the tabs connected to the instance that sent it. Swapping
  the session store fixes identity across instances and does nothing for this.
  It needs a bus, every instance subscribing and re-sending locally, and it is
  the one thing here that cannot be added quietly later.
