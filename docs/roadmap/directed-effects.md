# Directed effects

Pushing an `Effect` to a person, rather than a patch to a screen region.

Status: half built. Stage 1 is done, which was the link everything here waited
on: a connection now carries who it is, resolved from the session name when the
stream opens, and `connected` came with it so an audience is something a program
can observe rather than a field nothing reads. Stage 2 was already done. What is
left is stage 3 onwards, which is the sending itself, and it now waits on
nothing.

Two corrections to what follows, both recorded rather than edited away. The
resolver sketched below takes a session and reads it; exos holds no session
contents, only the name, so it takes the name and is async and fallible. And it
takes an `Option` of that name, because a stream cannot start a session and a
nameless connection is therefore a real state rather than one to design away.
See [sessions and identity](sessions-and-identity.md) stage 5 for what the built
version looks like.

The smaller items it names in passing, the missing step names, the reconnect
gap and the topic index, have moved to [loose ends](loose-ends.md), because
none of them should wait for audiences to exist. Two of the three are now done,
and the third is the one nothing has felt yet.

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
the topic id and a configured key, so it proves *somebody* was served the
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

**Built**, in [identity.rs](../../crates/exos/src/identity.rs). The stream is
opened with an ordinary `GET`, so it carries cookies. That is the one place
identity can be established without inventing a second channel.

```rust
#[derive(Hash)]
struct Viewer(u32);

impl exos::Audience for Viewer {
    const NAME: &'static str = "viewer";
}

exos::identify(async |name| {
    let Some(name) = name else {
        return Ok(Audiences::none());
    };

    Ok(match data::<Sessions>().viewer(&name).await? {
        Some(who) => Audiences::of(&Viewer(who.id)).and(&Team(who.team)),
        None => Audiences::none(),
    })
});
```

`Audience` is a trait, not a string and not a macro. An implementor is any
`Hash` type, and `Topic::new(Self::NAME, self)` reduces it to the same shape a
fragment topic has, which is what keeps one rule for how a name and its
arguments become a key. A `#[derive(Audience)]` filling in `NAME` from the type
name is obvious sugar and can come later.

That reuse turned out to buy less than this paragraph expected. It said one
registry and one match loop would serve both, and the first of the consequences
below is the reason they cannot: the sets are deliberately separate, so what is
shared is the arithmetic and not the loop. Two keys that spell the same string
are harmless precisely because of that separation.

Returning a set rather than one value is deliberate. It costs nothing and it is
the difference between addressing a user and addressing every admin, everyone
on a team, or every tab in a workspace.

This paragraph used to say the resolver was a pure function of what the session
already held, and therefore neither async nor fallible. That was true of a
framework that held session contents, and exos holds only the name, so the
resolver does the lookup and is both. It runs once per connection rather than
per request, which is what makes that affordable. A resolver that fails refuses
the stream, on the argument that opening one with no audiences is the silent
version of the same failure.

Three consequences, all built in from the start as this stage asked:

- **The client can never name an audience.** *Built.* Audiences live in their
  own set on the `Connection`, written only by the server at connect. They do
  not share the `topics` set that `/_exos/subscribe` overwrites, even though
  both hold the same kind of string. One field is client-claimed and
  token-proved; the other is server-derived and unforgeable. Merging them would
  make the distinction depend on a token check nobody can see from the type. A
  test hands `/_exos/subscribe` an audience key as a topic, with a valid token
  for it, and checks that the connection ends up watching a topic and being
  nobody.
- **The connection id is minted by the server.** *Built.* The client used to
  generate a UUID that `/_exos/subscribe` trusted, so guessing one let an
  attacker overwrite another tab's subscriptions. The server now mints it, sends
  it as the stream's first event, and the client subscribes with what it was
  given. That was worth doing on its own and it is also the precondition for
  this stage: a connection that carries identity cannot be named by whoever
  asks.
- **Identity is captured at connect and never refreshed.** Still true, and now
  it matters. The runtime morphs the body on navigation without reopening the
  `EventSource`, so a tab that logs in as somebody else keeps the previous
  audience. `Effect::reload()` at sign-in drops the stream and closes it, and
  the guide now says so as a rule rather than leaving it to be discovered. A
  `reconnect` step that closes and reopens the stream without a document load
  is still the better answer, and is now a thing to build rather than a thing to
  choose between.

## Stage 2: the request scope

**Built**, in [scope.rs](../../crates/exos/src/scope.rs), because
[sessions and identity](sessions-and-identity.md) needed it first. Rendering
`notification_count(id)` three components deep does not mean threading a user
id through every caller above it: it is a task-local set by a layer, on the
same argument [context.rs](../../crates/exos/src/context.rs) makes for `data`,
except per request rather than per process.

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
exos::send(&Viewer(user), &Effect::set(&Toast::signals().message, summary));
```

Every step becomes one event, exactly as `publish` sends one. Delivery is
best-effort fan-out to every open connection carrying that audience, which is
zero, one, or a tab per device.

The sender usually wants to know whether anyone is there, since the user's own
framing was that the server figures out who is connected:

```rust
if exos::connected(&Viewer(user)) { /* push */ } else { /* email */ }
```

**`connected` is built**, and came with stage 1 rather than waiting for this
stage, because an audience nothing can read is not a feature that shipped. That
answer is a race by nature, so it is a hint, not a guarantee. The honest version
of it is the rule in [stage 5](#stage-5-when-nobody-is-listening): persist
first, push second.

What is left here is `send` itself, which is the fan-out and the question of
what an `Effect` becomes on the way down. Nothing blocks it.

What is guaranteed is order, per connection. A connection has one channel and
sends happen under the registry lock, so a `publish` followed by a `send`
arrives in that order at every tab that gets both. Across connections nothing
is ordered, and nothing should be made to be.

### Client changes

None, which is the point of having closed it first. The runtime used to listen
for five of the eight step names, which was fine while the stream only carried
patches and would have become arbitrary once it carried effects. It registers
all eight now, as a [loose end](loose-ends.md) rather than as part of this
design, so every step this stage can send already has somewhere to land.

## Stage 4: things that are not state

The toast in the opening example is not a fragment, and it should not become
one. Two ways to say it, and they are not rivals.

**Signals.** `Effect::set` writes into the client store, so the sidebar's
template already owns the presentation and the server sends a value:

```rust
Effect::set(&Toast::signals().message, "Ada mentioned you in Q3 planning")
```

This works with no new step. Two things follow from `set` taking a handle
rather than a name.

The first has been settled since this was written. Signals arriving on the
stream merge into the global namespace, because there is no element to resolve
a scope against, and a `#[model]` field is now declared there wherever the
template puts the handle. So a receiving template declares it next to the
markup it belongs to, and the only rule left is that it declares it somewhere.

The second is an open question with two halves.

A toast is not a request body, so making it a `#[model]` purely to earn a name
is the tail wagging the dog, and today that is the only way to get a writable
one.

A toast also has to outlive a page, and a model does not. A document's
declaration says what that page's signals start as, and a navigation re-seeds
them, which is what stops a model reused with a second meaning from opening on
the previous page's value. A toast pushed while somebody is clicking a link
would be cleared by the page they land on.

Those are two axes rather than one knob:

| | reach: where the name resolves | lifetime: when the value resets |
| --- | --- | --- |
| `signal()` | the element that declared it | that element leaving the DOM |
| `#[model]` | the document | the next navigation |
| what a toast wants | the document | the tab closing |

So the shape that fits is a lifetime on the type rather than a third placement.
On the type, because one model kept in one template and reset in another would
be ambiguous about which it is:

```rust
#[exos::model(keep)]
#[derive(Debug, Default, Deserialize, Serialize)]
struct Toast {
    message: String,
}
```

Mechanically that is one more declaration attribute, say `data-signals-kept`,
which the re-seed skips and everything else treats identically.
[`Placement`](../../crates/exos/src/signal.rs) is `#[non_exhaustive]` so that
it can land additively.

Both halves are deliberately left until there are enough cases to see the
shape, and the toast is likely to be the first of them. One thing to settle
before building it: "survives a navigation" and "survives a reload" are
different features, the second being `localStorage` and a plugin that reads it
before the first paint. Guessing which one is meant is what this is waiting to
stop doing. Until then the generated names stay unpublished, which is what
keeps the answer open.

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

The third one was already a bug for live fragments, so it was fixed as a [loose
end](loose-ends.md) rather than here: a reconnect re-fetches the current URL and
morphs it in, and every fragment on screen comes back at once.

That repair is worth reading as a statement of the difference this whole
document is about. It works because a fragment is state and a page can render it
again. A directed effect sent into the gap has no fragment to re-render from and
stays lost, which is not a hole in the repair but the reason for the rule above
it.

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

The fix is an index from topic to connection ids, which is a [loose
end](loose-ends.md) with the constraints written down. Nothing here needs it to
land first; this is just the feature that will make it worth doing.

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
    exos::send(&Viewer(user), &Effect::set(&Toast::signals().message, event.summary()));
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
- **Anonymous visitors.** *Answered, and it fell out of the resolver as hoped.*
  The name reaches the resolver as an `Option`, so a logged-out visitor with a
  name can be addressed as that name, which is the queue position and the
  checkout timer, and a visitor with no name at all resolves to an empty set.
  Neither is a case in exos.
- **Should `Audience` be sealed?** *Answered: no.* Applications implement it,
  which is what makes a `NAME` collision theirs to avoid, and the derive would
  solve it for them.
- **Rate limiting.** Nothing here stops a job from sending a thousand effects
  to one connection and pushing everything else out of a 64-slot channel.
- **More than one instance.** The registry is a process-local `HashMap`, so a
  send reaches only the tabs connected to the instance that sent it. Swapping
  the session store fixes identity across instances and does nothing for this.
  It needs a bus, every instance subscribing and re-sending locally, and it is
  the one thing here that cannot be added quietly later.
