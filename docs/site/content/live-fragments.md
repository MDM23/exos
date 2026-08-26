# Live fragments

Markup that keeps itself up to date. The server owns the topic, so you never
name one:

```rust
#[exos::live]
fn presence(user: u32) -> Markup {
    let online = data::<Presence>().online(user);
    view! { <span class="dot" data-online={ online }></span> }
}
```

One definition, two uses:

```rust
{ presence(user.id) }          // in a template: renders it
publish(presence(user.id));    // anywhere: re-renders and pushes to watchers
```

Calling `presence(2)` renders nothing. What it answers with is the fragment's
name and the way to produce its markup, which is why it can be put in a template
and handed to `publish` alike, and it is worth a sentence because the difference
is what keeps a screen correct. A patch is state replacement, so what has to be
true is that the **last** patch a tab receives is the newest one. Reading the
state first and publishing second gives that away: a publisher that read first
can reach the wire second, and the stale markup then sits on the screen until
that topic is published again, which for the last write of the day is never.
Since the fragment has not read anything yet, `publish` does the read and the
send under one lock, and there is no rendered fragment to hand it instead.

That is also why the lock is per topic: a fragment says what it is called before
it does any work, so a publisher only ever waits for another publisher of the
same fragment.

The topic derives from the function name and the argument values, so
`presence(2)` always names the same fragment. One event stream per tab carries
everything, and the client re-derives its visible set from the DOM after every
mutation, so a fragment that scrolls in subscribes and one a patch removed
unsubscribes.

The tab does not name its own connection. The server mints an unguessable id
when the stream opens and sends it as the first event, and the tab reports its
visible set under that. An id a client could choose is one another client could
guess, and naming a connection is what replaces the set of topics it watches.
A reconnect is a new connection with a new id, which is why the client
re-reports its topics every time the stream comes back.

## The invariant

A topic must completely determine its content: the same topic means the same
HTML, for everybody.

Presence satisfies this. Anything depending on the viewer, such as their
session, their permissions or their draft input, does not, and must not be a
live fragment, because two users would share a topic and receive each other's
content.

If content depends on the viewer, either make the viewer part of the topic
(`inbox_count(user_id)`), or answer with an `Effect`, which reaches only the
requester.

## Authorization is structural

The rendered wrapper carries a token only the server can produce:

```html
<exos-live style="display:contents" id="live-presence-cab0087c" data-token="e80842d24f1b7a95c3e0d6118f27ba43">
```

Since the server only renders fragments it decided you may see, being able to
subscribe is the authorization. There is no second permission check to write
and none to forget, and a subscription with a forged token receives nothing.

The token is HMAC-SHA256 truncated to 128 bits, under one configured key:

```rust
exos::keys(exos::Keys::from_secret(std::env::var("EXOS_SECRET")?));
```

Everything signed derives its own subkey from that secret by label, so the live
token and whatever is signed later are independent, and none of them is the
secret. Unconfigured, exos mints a random key per process and says so on
stderr: right for `cargo run`, where a restart drops every stream anyway, and
wrong for a deploy, where two instances would never agree and a restart would
invalidate every token in flight.

What is signed is the topic **and the session it was served to**, so the token
proves this viewer was shown this fragment rather than merely that this server
rendered it once. An id and a token that escape a page together, by a
screenshot or a shared browser profile, are worth nothing to whoever finds
them: presented by another browser they verify against a different name and the
topic is dropped. Three things follow.

Being served a live fragment starts a session. That is a cookie and nothing
else, since exos keeps no store, so an anonymous visitor costs a header rather
than a row. A browser that refuses cookies has nothing to bind to and receives
no live updates.

Signing out or rotating invalidates every token in flight, which is what should
happen: `rotate` and `end` already close that browser's streams, the runtime
reconnects and fetches the page back, and the grants it comes back with are the
ones that verify.

A published patch carries no token, because a publish renders outside every
request and there is no viewer there to grant anything to. It does not need
one: the grant was made when the page was served, a patch has never been able
to make one, and the client keeps what the element already holds.

## Who a stream belongs to

A subscription says what a tab is showing. It cannot say who the tab is, and
some things are addressed to a person rather than to a region of a screen: a
notification, an alert, a nudge meant for one viewer's tabs and nobody else's.

An audience is that other half, and it is an ordinary `Hash` type with a name:

```rust
#[derive(Hash)]
struct Viewer(u32);

impl exos::Audience for Viewer {
    const NAME: &'static str = "viewer";
}
```

exos works out which ones a connection is in once, when the stream opens. An
`EventSource` is opened with an ordinary `GET`, so it arrives carrying the
session cookie, and that is the one place identity can be established without
inventing a second channel:

```rust
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

`identify` goes once, before serving, and an application that never calls it
has no audiences and pays for none. Answering with a set rather than one value
costs nothing and is the whole difference between addressing a user and
addressing every admin, everyone on a team, or every tab in a workspace.

The name arrives as an `Option` because a stream cannot start a session: its
response headers went out when it opened, so there would be no cookie to set.
What a name exos has never heard of stands for is the application's answer
rather than exos's, which is what lets a logged-out visitor still be addressed
by a queue position or a checkout timer.

Three rules are built in rather than asked for.

**The client can never name an audience.** Topics are client-claimed and
token-proved; audiences are server-derived and live in a set of their own on
the connection. A tab reporting a topic that spells an audience exactly is
still watching a topic and is still nobody.

**A resolver that fails refuses the stream**, rather than opening one with no
audiences, which is the same failure with none of the noise. `EventSource`
retries on its own, so a database that blinked costs a delay rather than a tab.

**Identity is captured when the stream opens and never refreshed.** The runtime
morphs the body on navigation without reopening the stream, so a tab that signs
in as somebody else keeps the audiences it had until the stream drops. That is
why sign-in answers with `Effect::reload()`, which drops the document and the
stream with it.

## Sending to a person

`publish` reaches whoever is watching a fragment. `send` reaches whoever *is*
somebody, whatever they happen to be looking at:

```rust
exos::send(&Viewer(user), &Effect::set(&Toast::signals().message, summary));
```

Every step becomes one event, exactly as a publish sends one, so anything an
`Effect` can say a directed effect can say. It reaches every tab that person
has open, because a toast in six tabs is six toasts and the page is the right
place to decide what to do about that. Sending to somebody who is not connected
is free and silent.

The two mechanisms are worth keeping apart in your head:

|                      | live fragment                        | directed effect                   |
| -------------------- | ------------------------------------ | --------------------------------- |
| addressed by         | topic: a name and its arguments      | audience: who the connection is   |
| content decided by   | the fragment function                | the sender, at the call site      |
| delivered while      | it is on screen                      | a stream is open                  |
| missed by the client | repaired by the next publish or load | lost                              |
| authorized by        | a token proving it was served        | the sender choosing who           |
| use it for           | state that is visible                | events, alerts, per-viewer nudges |

The last two rows are the ones that change how you write code.

**Authorizing is yours.** A subscription is authorized by construction, since a
topic can only be subscribed to by whoever was served it. `send` inverts that:
the server names the recipient, so exos guarantees that only connections whose
identity matched receive it and nothing at all about whether that person should
see the content. That check goes at the call site, in ordinary Rust, where it
can be read.

**A directed effect is an accelerator, never the record.** A patch is state
replacement and the next publish repairs a lost one. A directed effect has no
fragment to re-render from, so a recipient who is offline, whose tab lagged
past the channel's capacity, or who was inside a reconnect gap does not get it,
and none of the three is fixable by trying harder. Persist first, push second:

```rust
fn notify(user: u32, event: &Event) {
    // Durable first. Everything below is an accelerator.
    data::<Notifications>().record(user, event);

    // State, to whichever tabs are showing it.
    publish(notification_count(user));

    // The arrival, to the person.
    send(&Viewer(user), &Effect::set(&Toast::signals().message, event.summary()));
}
```

The badge sits in the layout, so it is on every page and every tab of that user
gets it from an ordinary publish. Only the last line needs an audience, and it
needs one because a toast is addressed to a person and has no correct second
delivery.

Asking first is allowed, and is how a push and an email are chosen between:

```rust
if exos::connected(&Viewer(user)) { /* push */ } else { /* email */ }
```

That answer is a hint and not a guarantee, since the last tab can close between
it and whatever is done about it, which is the rule above in another shape.

Order is guaranteed per connection and nowhere else. A connection has one
channel and both calls send under the same lock, so a publish followed by a
send arrives in that order at every tab that gets both. Two connections are
ordered against each other in no way at all.

## More than one instance

A connection is a socket, so the registry belongs to the process that opened
it. Publishing from a second process reaches its own tabs and nobody else's.
What crosses instead is the message: exos ships the two ends of a bus and no
broker, the same way it ships no session store.

```rust
exos::keys(Keys::from_secret(std::env::var("EXOS_SECRET")?));

exos::bus(move |frame| {
    let redis = redis.clone();

    async move {
        redis.publish("exos", frame.to_bytes()).await?;
        Ok(())
    }
});

// Your own subscriber loop, and your own reconnection.
tokio::spawn(async move {
    while let Some(message) = subscription.next().await {
        if let Some(frame) = Frame::from_bytes(message.payload()) {
            exos::deliver(frame);
        }
    }
});
```

A closure answering with a future rather than an async closure: the future an
async closure returns borrows what it captured, and a frame is sent from a
spawned task that outlives the call. Cloning the client into the future is what
every broker client is cheap to clone for.

`publish` and `send` then fan out. Local tabs are delivered to first and the
frame goes to the bus after, so a broker outage costs a cluster its cross-node
liveness rather than its liveness. What crosses is the rendered patch and never
a request to render one, because a topic is a hash of a name and its arguments
and no receiving node could invoke the function from it.

**No sticky sessions.** A tab holds its stream to one node and sends every
other request wherever the load balancer points. A subscription that lands on a
node holding no such connection is verified there, since that node has the
cookie, and forwarded to the node that does; the browser is told `204` and
never learns that nodes exist. What still answers `410` is a connection this
node minted and no longer has, which is a stream that really has gone.

**Signing out crosses too.** `Session::rotate` and `end` end that browser's
streams on every node, not on the one that took the request, so the tabs it did
not sign out in reconnect as whoever it is now wherever they are streaming
from. What crosses is what the session name reduces to and never the name.

Two things to know before running two of anything:

- **A signing key is no longer optional.** `exos::bus` refuses to register
  without `exos::keys`, because a random key per process is a token that
  verifies on the node that minted it and nowhere else.
- **Ordering narrows.** Within one node the last patch a tab receives for a
  topic is still the newest. Across nodes there is nothing serializing two
  publishes, and a publish followed by a send holds its order for a local
  connection and not for a remote one. A forwarded subscription is not ordered
  against a publish either: a tab can miss one patch of a fragment it has just
  claimed, and the next publish repairs it.

A frame carries no session name, no connection id, no fragment arguments and no
token. A key is already a hash, so a broker's operator, its logs and its backups
never hold anything that logs anybody in.
