# Sessions and identity

Somewhere to put who this is, and a way to reach them.

Status: built, and narrower than it was drawn. Stages 1 to 6 are done. exos
names the browser and carries the name in a cookie; what a name *means* turned
out to belong to the application, and [stage 3](#stage-3-and-no-store-at-all) is
the argument for that, made after a session store was built and then taken out
again. A live subscription now proves the browser presenting it was served the
fragment, which was the README's first listed gap.

What is left is [stage 7](#stage-7-csrf-which-is-mostly-already-handled), which
argues that most of it should stay unbuilt. Each stage below says where it
stands.

## Three questions, one mechanism

- **Who is this request?** A page that renders a notification badge needs a
  user id, three components deep, in a function that is not a handler.
- **Who is this stream?** An `EventSource` is opened with a `GET` and then
  stays open for the life of the tab. Whatever it is allowed to receive is
  decided once, from that request.
- **Is this token theirs?** `Topic::token` proves somebody was served a
  fragment. Binding it to a session is what makes it prove *this viewer* was.

They are the same mechanism seen from three sides, which is the argument for
building it once rather than letting each grow its own answer.

## What exos will not do

Worth fixing before the API, because most of what people mean by "auth" is not
in scope and never will be.

- **No password hashing, no OAuth, no OIDC, no magic links.** Those are the
  application's, or a crate's.
- **No user table and no login pages.** exos never learns what a user is.
- **No permission model.** No roles, no policies, no `can()`. Authorization
  happens at the call site, in ordinary Rust, where it can be read.
- **No "remember me" or device management UI.**
- **No session store**, which is the one this document argued itself into. See
  [stage 3](#stage-3-and-no-store-at-all).

What exos owns is narrower than this section originally allowed for: an opaque
name, the cookie that carries it, a request scope to put whatever the name
resolves to, and the mapping from that name to who a live stream belongs to.

## Stage 1: a request scope

**Built**, in [scope.rs](../../crates/exos/src/scope.rs). `exos::scope()`
answers with a per-request store keyed by type, `with_scope` gives a test one
without a server in front of it, and a layer in `app()` wraps everything, so
the stream and the asset routes are inside it too.

Both rules the design asked for are enforced rather than documented, and the
two failures say different things. Outside a request, asking panics, because a
background job reading the session is a mistake made once rather than a
condition every caller handles, and `None` would quietly render the logged-out
view of something and then publish it. Inside a live fragment, asking also
panics: `#[exos::live]` renders through `detached`, so a fragment cannot read
the request in either of the two places it renders, and its arguments stay its
whole input.

This stage turned out to carry more of the design than it looks. `Scope::get`
answered with an `Option` because nothing written yet is what anonymous looks
like, and that is still exactly where a viewer goes. What stage 3 eventually
concluded is that the scope is not a stepping stone towards a session store: it
*is* the session, for everything except the name.

## Stage 2: the name

**Built**, in [session.rs](../../crates/exos/src/session.rs), and it is less
than this stage originally asked for. An opaque random id in a cookie, and
nothing else:

```text
Set-Cookie: exos=<128 random bits, as hex>;
            HttpOnly; Max-Age=34560000; Path=/; SameSite=Lax; Secure
```

```rust
let session = exos::session();

session.id()     // Option<Id>: the name it arrived with, if any
session.start()  // Id: the name, minting one if there is none
session.rotate() // Id: a new name, whatever it was called before
session.end()    // no name, and the cookie goes back
```

This stage used to specify typed contents keyed by type, `session.set(&value)?`
and `session.get()?`, with a store behind them. That was built and then removed
before it shipped; [stage 3](#stage-3-and-no-store-at-all) records why. What
remains is the half with no choices in it.

Four things came out differently from the sketch this stage started as.

**Hex, not base64url.** A live token, a connection id and a session id are the
same kind of thing, and there were already two hand-rolled hex encoders in the
tree when this needed a third. One [hex](../../crates/exos/src/hex.rs) module
now serves all three. The extra characters in the cookie buy not having a second
encoding, and not writing a decoder for it.

**`Max-Age`, which the sketch left out, and a long one.** It is four hundred
days, the longest browsers accept. That is not a session lifetime and is set
that way so it cannot be mistaken for one: exos does not know when an
application's record expires, and a cookie running out first would sign somebody
out while their record was still good. The other direction is harmless, because
a name nothing is stored under is simply anonymous.

**Not signed data in the cookie**, which the original argument still holds for.
The application's store is the authority, so there is nothing in the cookie to
forge and no key to rotate, and revocation is a delete. Stateless cookie
sessions trade that away for not having a store, and sign-out has to be able to
reach a stream that is already open.

**`start` and `rotate` are different calls.** `start` is idempotent and is what
an anonymous cart wants; `rotate` always answers with a name nothing has seen
and is what a sign-in wants. `id` never mints, so a page that only looks costs
no cookie, which is the whole of the laziness this stage asked for.

## Stage 3: and no store at all

**Decided, against what this stage used to say, and recorded here so the
question is not reopened without new information.**

It used to say to take `tower-sessions`: the cookie, the lazy materialisation,
the store trait and the layer are not where exos is interesting, and are where
session bugs live. Then it said exos should own an equivalent, on the grounds
that almost nothing of that crate survived the conditions attached to taking it.
Both of those were answers to the wrong question. The right one is whether exos
should hold a session's *contents* at all, and it should not.

**The deciding argument is about awaiting.** Resolving a name to a user is a
database call. In an application it happens in a handler, where awaiting is
legal, and the result goes in the [request scope](#stage-1-a-request-scope),
which every view underneath reads synchronously. That is the scope doing the
job it already exists for.

A framework holding the contents cannot do that. `session()` has to answer
synchronously, because a [`view!`](../site/content/templates.md) fragment is
a plain function and cannot await, so the layer has to load before the handler
runs, and every request carrying a cookie pays for a session whether or not it
uses one. That concession was written into this document for exactly one
release of it. Handing back a name costs nothing and puts the round trip where
the application decides it goes.

Four things follow, and each of them was a known defect in the store version.

- **No key derived from `type_name`.** Keying values by the compiler's name for
  their type meant moving a type between modules silently emptied it for
  everybody holding a session. That existed only because exos was storing typed
  values.
- **No blind writes.** `SessionStore::save` wrote a record whole with no
  version, so two actions in flight from one tab lost a key. An application's
  table has whatever concurrency answer it already uses.
- **No expiry policy.** exos had picked fourteen days and a refresh-at-halfway
  rule, both invented. An application expires its own rows.
- **No error type.** `id`, `start`, `rotate` and `end` cannot fail, so
  `SessionError` went with the store. A boundary that makes a whole error enum
  unnecessary is usually the right boundary.

What it costs is the store ecosystem and a little repetition. Nobody will write
a Redis store for exos, because there is nothing to write one against; instead
every application writes the same handful of lines resolving a name and putting
the result in the scope. The obvious answer is a single hook rather than a trait
(see [stage 5](#stage-5-identity-on-the-stream), which needs the same thing),
and it is worth building when something wants it rather than now.

The general shape of the decision is worth keeping: **the narrow version is the
reversible one.** Shipping the store would have made `Record`, the expiry policy
and the `type_name` keying public API with two known defects in them. Shipping
the name keeps every one of those questions open, and a typed store can still
land later as an additive layer on top. The other order does not work.

## Stage 4: signing in and out

**Built.**

```rust
#[exos::post("/session")]
async fn sign_in(Json(form): Json<Credentials>) -> Result<Effect, Error> {
    let user = data::<Users>().authenticate(&form).await?;
    let session = exos::session();
    let sessions = data::<Sessions>();

    // Read before rotating, because rotating replaces it: an anonymous visit
    // may have left something under the old name worth moving or dropping.
    let previous = session.id();
    let id = session.rotate();

    sessions.bind(&id, user.id).await?;

    if let Some(previous) = previous {
        sessions.forget(&previous).await?;
    }

    Ok(Effect::reload())
}
```

`rotate` stays exos's, which is the one part of a session's contents leaving
that was worth arguing about. Two reasons it does: it is a cookie operation, and
forgetting it is silent. It is still an explicit call, because only the
application knows which request is the privilege change, and this stage used to
wonder whether a write could imply it. It cannot. exos no longer sees any writes
at all, and even when it did it could not tell a principal from a shopping cart,
so implying it would have rotated on a cart and invalidated the live tokens of
anonymous visitors for nothing.

`rotate` lost its `?` and gained a return value. It replaces a name and hands
back the new one, which is what the very next line needs, and nothing about that
can fail. `end` is the other end of it and is exos's half of signing out;
deleting whatever the application stored is the application's half, and it has
to happen, because a name the browser stopped sending is not a name nobody else
has.

Two consequences reach further than this handler, and both are why sign-in
answers with a `reload` rather than a patch.

- **Rotation will invalidate every live token**, once stage 6 binds them to the
  session id. A fragment still on screen would silently stop updating. It does
  not yet, because that binding does not exist.
- **A stream's identity is captured when it opens.** The runtime morphs the
  body on navigation without reopening the `EventSource`, so a tab that signs
  in as somebody else keeps the previous audience until the stream drops. That
  is a leak, and it is the same one [directed effects](directed-effects.md)
  names in its stage 1.

A `reload` closes both by dropping the document, for the tab that asked.

**What it does not close is the browser's other tabs**, and this section used
to say a `reconnect` step would. Building
[`examples/auction`](../../examples/auction) showed that it would not, which is
worth writing down because the reasoning looks sound until it is tried. What
does close them is **built**, in
[stream.rs](../../crates/exos/src/live/stream.rs) and
[session.rs](../../crates/exos/src/session.rs), and the rest of this stage is
the argument that got there.

A browser is one cookie and several tabs. Signing in changes the cookie for all
of them, and only one of them made a request that can be answered. Reaching the
rest by pushing them something down the stream, a `reload` or a `reconnect`
alike, loses a race it cannot win: the push goes out while the response
carrying the new cookie is still being written, so a tab that acts on it at
once re-requests with the *old* cookie and comes back as whoever it used to be.
Nothing prompts it a second time. The example reproduced this by hand: a reload
carrying the previous name renders the guest again, and stays that way.

The race is not about streams, so no step pushed down one can settle it. It is
about which of two concurrent things reaches the browser first, and a mechanism
that does not carry the cookie cannot order itself against it.

**The answer is server-side and involves no client at all.** A connection now
records the session name it opened under, and a rotation ends every connection
carrying the name it replaced. The session layer is where it happens, because
it already compares the arriving name against the current one to decide the
`Set-Cookie` and is therefore already the thing that knows.

**Ending them, rather than re-identifying them.** The first design was to
re-resolve those connections and swap their audiences in place, which is
strictly worse and was caught by asking what it does to a stolen cookie.
Rotation is the defence against exactly that, and re-resolving would carry a
compromised connection *across* the boundary: an attacker with a live stream on
the old name would be upgraded to the victim's new identity and stay there,
reading every directed effect, where today they merely go stale. Ending the
stream is what makes rotation mean what it says.

It also turned out to be the smaller change, and to fix the tabs rather than
only their identity. There is nothing to await, so no resolver call and no
`await_holding_lock` question. And the client needs nothing new: `EventSource`
reconnects on its own, long past the cookie race, and a greeting that is not
the first already makes the runtime re-fetch the page. So the other tabs come
back correctly identified *and* showing the right markup, with no reload and no
flash. The [reconnect repair](loose-ends.md) built for a dropped connection
turns out to be exactly what a renamed one needs.

**The wait is the server's to set, which is what makes it quick enough.** A
browser left to itself waits about three seconds after a stream it thinks
broke, and three seconds of a tab showing the wrong name is the whole
complaint. Server-sent events let the stream name its own reconnection time, so
one ended on purpose says `retry: 150` on the way out and the greeting on the
next one puts three seconds back. Measured against a spec `EventSource`: 3041ms
without it, ~175ms with. The impatience therefore lasts exactly one reconnect,
and the only window where it outlives that is a server going down in the moment
between the two.

That is worth contrasting with the client-side answer, which was the obvious
one and is not needed. A `BroadcastChannel` would let the tab that signed in
tell its siblings directly, correctly ordered because that tab demonstrably has
the cookie, and it would be instant rather than nearly instant. It is also a
new step, a new client concept, cross-tab messaging to scope and a stub for the
jsdom harness, to save a sixth of a second. Worth reaching for if something
ever needs true immediacy; not worth reaching for first.

Two things to know about it.

- **A connection that opened under no name is never matched.** Every browser
  arriving without a cookie looks identical, so treating them as a group would
  let one visitor's sign-in end every anonymous stream in the process. That is
  the one rule a plausible implementation gets wrong, and it has a test of its
  own.
- **Subscriptions survive the drop**, because the client re-sends them on the
  new connection. That is right today, since a live token proves a topic and
  nothing else. Once [stage 6](#stage-6-keys) binds tokens to the session id,
  `/_exos/subscribe` will reject the stale ones by itself, which is the right
  place for it rather than here.

A `reconnect` step is still worth building, and this was never the argument for
it. It is worth building because sign-in drops a whole document in order to
drop one stream, and that flash is the only reason the tab that signed in has
to be rebuilt at all.

Signing out everywhere, rather than here, needs an index from user to session
names. That is now plainly the application's, and it gets it for free if its
sessions table is keyed the obvious way. Reaching the tabs afterwards is a
directed effect to that audience, which is the first thing the two documents
share.

## Stage 5: identity on the stream

**Built**, in [identity.rs](../../crates/exos/src/identity.rs). Both of its
preconditions were already in place. The connection it hangs identity on is the
server's to name, since the id is minted server-side and sent as the stream's
first event rather than invented by the client and trusted. And the stream's
`GET` runs inside the session layer, which was the reason to mount that layer
around `/_exos/live` rather than only around the handlers.

The resolver [directed effects](directed-effects.md) asked for takes the name
and answers with the audiences it stands for:

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

**It is async, where the plan originally said it would be sync**, and stage 3 is
why: the framework no longer holds anything to make it a pure function of. It
runs once per connection, on the stream's `GET`, which is what makes that
affordable. It is fallible for the same reason, and a resolver that fails
refuses the stream, exactly as this stage guessed it would have to: opening one
with no audiences is the silent version of the same failure, and `EventSource`
retries on its own, so refusing costs a delay rather than a tab.

Four things came out differently from the sketch.

**The name arrives as an `Option`, and anonymous is not a case exos decides.**
The sketch took an `Id`, which quietly assumed exos would short-circuit a
connection with no name. It should not. A stream cannot start a session, since
its response headers go out when it opens and there is no second chance at the
cookie, so a nameless connection is a real state rather than one to design
away. Handing it to the resolver is also what makes the open question below
about anonymous visitors fall out rather than become a feature: a name with
nobody behind it can be addressed as itself, which is what a queue position or
a checkout timer wants.

**Audiences are taken by reference**, `Audiences::of(&Viewer(id))`, which the
sketch wrote without the `&`. That matched `publish(&fragment)` and the `send`
the other document sketches, and it keeps `needless_pass_by_value` quiet
without an exception. `publish` has since taken the render rather than the
fragment, so it is `send` that this still lines up with; nothing about the
argument here changed with it.

**An audience reduces to the same key a topic does**, through `Topic::new`, so
there is one rule for how a name and its arguments become a key rather than two
that could drift. That an audience and a topic could in principle spell the
same string is harmless, because they are matched against separate sets, and
keeping those apart is the whole security property. A test claims an audience
key as a topic, with a valid token for it, and checks that the connection ends
up watching a topic and being nobody.

**`connected` came with it**, from [directed effects](directed-effects.md)
stage 3, because a field nothing can read is not a built feature. It is the
minimum that makes an audience observable, and `send` is still that document's.

Worth noticing what this hook is: an async function from a session name to
something the request needs. So is the six lines every application will write
to put a viewer in the scope. If exos ever grows the one, it should be the same
one, serving the handler and the stream from a single place rather than asking
for the same lookup twice in two shapes. That is the shape to reach for when
something wants it, and it is not a store trait.

## Stage 6: keys

**The key material is built**, in [keys.rs](../../crates/exos/src/keys.rs), and
was taken out of order because the live token needed it before sessions
existed. There is one configured key, everything derives a subkey from it by
label, and `live-token` and `csrf` are already independent of one another
without `csrf` existing yet:

```rust
exos::keys(Keys::from_secret(std::env::var("EXOS_SECRET")?));
```

Unconfigured, it mints a per-process random and says so on stderr, which is
right for `cargo run` and wrong for everything else. The dependency cost landed
as predicted: `hmac`, `subtle` and `getrandom`, with `sha2` already in the
workspace for content hashing.

**The binding is built**, and it closes the README's first gap. A token used to
be the MAC of the topic id alone, so it proved this server rendered the fragment
and not that this viewer was served it. The message gained the session id:

```text
token = HMAC-SHA256(subkey("live-token"), topic_id || session_id)[..16]
```

Verification at `/_exos/subscribe` reads the session the request carries, so a
pair lifted out of somebody else's page verifies against a different name and
the topic is dropped. Three things follow, as this section always said they
would. Rendering a live fragment calls `session().start()`, so an anonymous
visitor served one gets a name, which is a cookie and nothing else and therefore
genuinely free. Rotation invalidates outstanding subscriptions, which is what
stage 4's `reload` is for. And caching does not suffer, because
[`Page`](../../crates/exos/src/response.rs) already answers `no-cache, private`.

**What made it more than an afternoon is that `publish` renders outside any
request.** It calls [`Fragment::to_markup`](../../crates/exos/src/live.rs),
which wrote the `data-token` attribute into the patch it pushes, and it is
called from background jobs and from handlers acting on somebody else's behalf.
There is no session there, and `session()` panics outside a request by design.

Three ways out were drawn: the morph preserving the attribute, `publish` sending
the contents without the wrapper, and a token bound to the connection instead.
The second was called the likely answer and the first was called the cheap one
that makes the client responsible for a security property.

**What was built is the first, and the objection to it was wrong.** The morph
never removes `data-token`, and markup that carries one overwrites it. The
client is not being trusted with anything: a token is checked by HMAC on the
server, and a browser that edits its own DOM can write whatever it likes there
either way. What the rule actually says is that a patch is not a grant, which
is the same sentence the second option was reaching for, without a wire format
change, a new step, or `applyPatch` learning to address the inside of an
element.

The one thing the second option had over it is that a token cannot be replaced
by a patch at all. That is not wanted: a rotated session has to be able to hand
its tabs new grants, and it does so through the page the runtime fetches back.

### What it found

**A rule about removal needs the other half spelled out.** "The client keeps the
token" and "the client owns the token" are one word apart and the second is a
tab that can never be re-granted. A rotation ends the streams, the runtime
repairs the page, and the fresh tokens in it have to win, so the morph writes
what arrives and only declines to take away what does not.

**The observer was watching the wrong thing for this.** Subscriptions are synced
on a mutation, and the observer took `childList` only. A page that comes back
after a rotation with identical markup and fresh tokens produces no node
mutation at all, so the tab would go on presenting grants the server had stopped
honouring, silently, until something else on the page happened to change. It now
watches `data-token` as well, which is the one attribute a subscription is made
of. A client test fails without it.

**The mask that keeps a fragment viewer-independent is the same rule.** A
fragment inside a fragment renders through
[`detached`](../../crates/exos/src/scope.rs), so it cannot read the session and
carries no token either. That looked like a regression and is the correct
answer: a nested fragment's markup is published to everybody watching the outer
topic, so a token in it would be one viewer's grant handed to all of them.

**A fragment has two halves and only now needed telling apart.** Two viewers are
served the same content under different grants, so the examples' "it says the
same thing to everybody" tests stopped holding on `to_markup` and now hold on
[`Fragment::markup`](../../crates/exos/src/live.rs), with the wrapper asserted
to differ. The invariant did not change; what changed is that the wrapper is no
longer part of what the topic determines.

**A token can only be had by being served one**, which is what a test has to do
too. [`tests/directed.rs`](../../crates/exos/tests/directed.rs) fetches a route
that renders the wrapper and reads the token out of the markup, carrying its
cookie the whole way, because there is no way in from outside the crate and
there should not be.

## Stage 7: CSRF, which is mostly already handled

**Not built, and mostly should not be.** The README lists this as a gap, and
the analysis is better than the entry suggests. Three things already stand
between an attacker's page and a state change.

- **`SameSite=Lax`** on the session cookie means a cross-site `POST` carries
  no session at all.
- **A custom header.** The runtime sends `X-Exos` on every request. A
  cross-origin request cannot set one without a preflight, and a preflight
  needs CORS the application never turned on.
- **JSON bodies.** A cross-origin HTML form can only send the three
  form-safe content types, and `Json<T>` refuses all of them.

Together that is a policy rather than the start of one, and the first two legs
of it now hold: the session cookie is `SameSite=Lax`, and the guide says all
three under [cross-site
requests](../site/content/sessions.md#cross-site-requests) as the reason exos
ships no token. The gap is real but narrow: a handler that accepts a
form-encoded body steps outside all three at once. The answer is a token derived
from the session id, rendered by a helper into the form, and it should be built
when the first form handler exists rather than before.

## What it cost

Almost nothing, which is the point of the shape it ended up as.

- **No new dependencies**, where this section used to forecast `tower-sessions`
  and the tower stack behind it. `tower` is still a dev-dependency only, and the
  crypto was already paid for by stage 6.
- **No store round trip anywhere in exos**, and no store. Reading a cookie
  header is the whole of what a request that carries a name pays for.
- **Every application writes the same handful of lines**, resolving a name and
  putting the result in the scope. That is the price of the boundary and the
  thing to watch: if it turns into more than a handful, or if it and stage 5's
  resolver want the same lookup in two shapes, that is the signal to build one
  hook for both. Stage 5 shipped the stream's half of that as `identify`,
  which is what makes the question answerable now rather than guessable.

## Testing

The task-local is the better half of this for tests, and it already works.
`provide` is process-global and its module docs warn that parallel tests
interfere; a request scope is per task, so two tests can hold different sessions
at once without arranging anything.

The open question here was whether the session needs a seeding helper of its
own. It does not, and now it obviously does not: what a test wants to seed is
the *viewer*, which is application data in the scope, and `with_scope` already
does that. `session()` under `with_scope` mints and rotates exactly as it would
in a request, for the tests that care about the name itself.

```rust
exos::with_scope(|| {
    exos::scope().set(Viewer { id: 7, team: 3 });
    assert_eq!(badge().into_string(), "<span class=\"badge\">2</span>");
});
```

## Open questions

Three of these used to be here and are gone, because they were questions about
a store: concurrent writes clobbering, whether the key should be `type_name`,
and what the expiry policy should be. An application answers all three with
whatever its database already does.

- **Cached viewer or a lookup per request.** Resolving the name on every request
  is a query per request; caching what it resolved to means a revoked role stays
  live. This is now plainly the application's call, which is the right place for
  it, and the guide should probably say so rather than leaving it unmentioned.
- **One resolver hook, or two.** [Stage 5](#stage-5-identity-on-the-stream)
  built half of it: `identify` is an async function from a name to what the
  *stream* needs. The handler half is still the six lines every application
  writes into the scope. Whether they should become one hook is now a question with a
  concrete shape in front of it rather than a guess, which is the right time to
  leave it open a little longer: the two want different answers out of one
  lookup, audiences on one side and a viewer in the scope on the other, and
  collapsing them before something has written both is how the store happened.
- **Persisting across a browser restart is the browser's business now.** The
  cookie asks for four hundred days, so exos imposes no ceiling. Whether a
  session survives is decided by the application's record, which is the answer
  "remember me" wanted anyway.
- **Multi-instance.** Sessions are the application's, so they cross instances if
  its database does. That leaves the connection registry, which is a
  process-local `HashMap`, so a directed effect only reaches the tabs connected
  to the instance that sent it. That belongs to [more than one
  instance](more-than-one-instance.md), which also names what a rotation has to
  do about a stream held by another node.
