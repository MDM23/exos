# Sessions and identity

Somewhere to put who this is, and a way to reach them. Built, and narrower than
it was drawn: exos names the browser and carries the name in a cookie, and what
a name *means* belongs to the application.

This is the design as built. What is still open is at the bottom; the rest is
kept because it is what stops each decision being re-taken.

## What exos owns

- An opaque name for the browser, in a cookie.
- A [request scope](#the-request-scope) to put whatever that name resolves to.
- The mapping from a name to who a live stream belongs to.
- One rule about which requests it will accept at all.

It owns no user table, no login pages, no permission model, no `can()`, no
device management, and [no session store](#no-store-at-all). Authorization
happens at the call site, in ordinary Rust, where it can be read.

## The request scope

[scope.rs](../../crates/exos/src/scope.rs). `exos::scope()` answers with a
per-request store keyed by type, `with_scope` gives a test one without a server
in front of it, and a layer in `app()` wraps everything, so the stream and the
asset routes are inside it too.

Two rules, both enforced rather than documented, and the failures say different
things. Outside a request, asking panics: a background job reading the session
is a mistake made once rather than a condition every caller handles, and `None`
would quietly render the logged-out view of something and then publish it.
Inside a live fragment, asking also panics: `#[exos::live]` renders through
`detached`, so a fragment's arguments stay its whole input and its topic keeps
determining its content.

`Scope::get` answers with an `Option`, which is where a viewer goes and what
anonymous looks like.

## The name

[session.rs](../../crates/exos/src/session.rs). An opaque random id in a
cookie, and nothing else:

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

- **Hex, not base64url.** A live token, a connection id and a session id are the
  same kind of thing, and one [hex](../../crates/exos/src/hex.rs) module serves
  all three rather than a third hand-rolled encoder.
- **`Max-Age` is four hundred days**, the longest browsers accept. That is not a
  session lifetime and is set so it cannot be mistaken for one: exos does not
  know when an application's record expires, and a cookie running out first
  would sign somebody out while their record was still good. The other
  direction is harmless, because a name nothing is stored under is anonymous.
- **Nothing is signed into the cookie.** The application's store is the
  authority, so there is nothing to forge, no key to rotate, and revocation is a
  delete.
- **`start` is idempotent and `rotate` never is.** `start` is what an anonymous
  cart wants, `rotate` is what a sign-in wants, and `id` never mints, so a page
  that only looks costs no cookie.

## No store at all

**Decided, and recorded so the question is not reopened without new
information.** A store was specified, built, and taken out again before it
shipped.

The deciding argument is about awaiting. Resolving a name to a user is a
database call. In an application it happens in a handler, where awaiting is
legal, and the result goes in the request scope, which every view underneath
reads synchronously. A framework holding the contents cannot do that:
`session()` has to answer synchronously, because a `view!` fragment is a plain
function and cannot await, so the layer has to load before the handler runs and
every request carrying a cookie pays for a session whether or not it uses one.

Four known defects of the store version went with it: a key derived from
`type_name`, so moving a type between modules silently emptied it for everybody;
blind whole-record writes, so two actions in flight from one tab lost a key; an
invented expiry policy; and an error type that nothing needs, since `id`,
`start`, `rotate` and `end` cannot fail.

What it costs is the store ecosystem and a little repetition: nobody will write
a Redis store for exos because there is nothing to write one against, and every
application writes the same handful of lines resolving a name into the scope.

The general shape is worth keeping: **the narrow version is the reversible
one.** Shipping the store would have made `Record`, the expiry policy and the
keying public API with two known defects in them. A typed store can still land
on top of a name; the other order does not work.

## Signing in and out

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

`rotate` stays an explicit call. Only the application knows which request is the
privilege change, and exos sees no writes at all and could not tell a principal
from a shopping cart, so implying it would rotate on a cart and invalidate the
live tokens of anonymous visitors for nothing. `end` is exos's half of signing
out; deleting whatever the application stored is the application's half, and it
has to happen, because a name the browser stopped sending is not a name nobody
else has.

Sign-in answers with a `reload` because rotation invalidates every live token
and because a stream's identity is captured when it opens: the runtime morphs
the body on navigation without reopening the `EventSource`, so the tab would
keep the previous audience. Dropping the document closes both, for the tab that
asked.

### The other tabs, which no pushed step can reach

A browser is one cookie and several tabs, and only one of them made a request
that can be answered. Pushing the rest something down the stream, a `reload` or
a `reconnect` alike, loses a race it cannot win: the push goes out while the
response carrying the new cookie is still being written, so a tab that acts on
it at once re-requests with the *old* cookie and comes back as whoever it used
to be, with nothing to prompt it again. Reproduced by hand in
[`examples/auction`](../../examples/auction). The race is not about streams, so
no step pushed down one settles it.

**The answer is server-side and involves no client at all.** A connection
records the session it opened under, and a rotation ends every connection
carrying the name it replaced, in the session layer, which already compares the
arriving name against the current one to decide the `Set-Cookie`.

- **Ending them, rather than re-identifying them.** Re-resolving in place would
  carry a compromised connection across the boundary: an attacker with a live
  stream on a stolen cookie would be upgraded to the victim's new identity and
  stay there. Ending the stream is what makes rotation mean what it says. It is
  also the smaller change, with nothing to await.
- **The client needs nothing new.** `EventSource` reconnects, a greeting that is
  not the first makes the runtime re-fetch the page, and the [reconnect
  repair](../roadmap/loose-ends.md) built for a dropped connection is exactly
  what a renamed one needs.
- **The wait is the server's to set.** A browser left alone waits about three
  seconds; a stream ended on purpose says `retry: 150` on the way out and the
  next greeting puts three seconds back. Measured: 3041ms without, ~175ms with.
  A `BroadcastChannel` would be instant rather than nearly instant, and costs a
  new client concept and a jsdom stub to save a sixth of a second.
- **A connection that opened under no name is never matched.** Every browser
  arriving without a cookie looks identical, so treating them as a group would
  let one visitor's sign-in end every anonymous stream in the process. It has a
  test of its own.

Signing out everywhere needs an index from user to session names, which is the
application's and comes free if its sessions table is keyed the obvious way.
Reaching the tabs afterwards is a [directed
effect](../roadmap/directed-effects.md) to that audience.

## Identity on the stream

[identity.rs](../../crates/exos/src/identity.rs). The connection is the
server's to name, since the id is minted server-side and sent as the stream's
first event rather than invented by the client, and the stream's `GET` runs
inside the session layer, which is why that layer is mounted around
`/_exos/live` rather than only around the handlers.

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

- **Async and fallible**, because the framework holds nothing to make it a pure
  function of. It runs once per connection, which is what makes that
  affordable. A resolver that fails refuses the stream: opening one with no
  audiences is the silent version of the same failure, and `EventSource`
  retries, so refusing costs a delay rather than a tab.
- **The name arrives as an `Option`, and anonymous is not a case exos decides.**
  A stream cannot start a session, since its response headers go out when it
  opens and there is no second chance at the cookie, so a nameless connection is
  a real state. Handing it to the resolver is also what lets a name with nobody
  behind it be addressed as itself, which is what a queue position or a checkout
  timer wants.
- **An audience reduces to the same key a topic does**, through `Topic::new`, so
  there is one rule rather than two that could drift. That the two could spell
  the same string is harmless, because they are matched against separate sets,
  and keeping those apart is the security property. A test claims an audience
  key as a topic with a valid token and checks the connection ends up watching a
  topic and being nobody.
- **A connection registers before the resolver is awaited**, and `identify`
  answers with whether there was still a connection to identify. Resolving first
  left a window, containing the application's database call, in which a rotation
  closed the streams it could see and this one registered afterwards holding the
  audiences of the session that had just gone.

## Keys, and the token

[keys.rs](../../crates/exos/src/keys.rs). One configured key; everything
derives a subkey by label, so `live-token` and `csrf` are independent.

```rust
exos::keys(Keys::from_secret(std::env::var("EXOS_SECRET")?));
```

Unconfigured, it mints a per-process random and says so on stderr, which is
right for `cargo run` and wrong for everything else.

A token used to be the MAC of the topic id alone, so it proved this server
rendered the fragment and not that this viewer was served it. The message
carries the session id:

```text
token = HMAC-SHA256(subkey("live-token"), topic_id || session_id)[..16]
```

`/_exos/subscribe` reads the session the request carries, so a pair lifted out
of somebody else's page verifies against a different name and the topic is
dropped. Rendering a live fragment calls `session().start()`, so an anonymous
visitor served one gets a name, which is a cookie and nothing else. Caching does
not suffer, because [`Page`](../../crates/exos/src/response.rs) already answers
`no-cache, private`.

What made it more than an afternoon is that `publish` renders outside any
request, where `session()` panics by design, and `Fragment::to_markup` wrote
`data-token` into the patch it pushes.

- **The morph never removes `data-token`, and markup that carries one overwrites
  it.** The objection that this makes the client responsible for a security
  property is wrong: a token is checked by HMAC on the server, and a browser
  that edits its own DOM can write whatever it likes there either way. The rule
  says a patch is not a grant. Removal is the half that needs spelling out: "the
  client keeps the token" and "the client owns the token" are one word apart,
  and the second is a tab that can never be re-granted.
- **The observer watches `data-token`, not only `childList`.** A page that comes
  back after a rotation with identical markup and fresh tokens produces no node
  mutation at all, so the tab would go on presenting grants the server had
  stopped honouring. A client test fails without it.
- **A nested fragment carries no token**, because it renders through `detached`
  and its markup is published to everybody watching the outer topic. A token
  there would be one viewer's grant handed to all of them.
- **A fragment has two halves.** Two viewers are served the same content under
  different grants, so "it says the same thing to everybody" holds on
  `Fragment::markup` rather than on `to_markup`, with the wrapper asserted to
  differ.
- **A token can only be had by being served one**, including in tests:
  [`tests/directed.rs`](../../crates/exos/tests/directed.rs) fetches a route
  that renders the wrapper and reads the token out of the markup, carrying its
  cookie the whole way.

`Topic::verify` consults nobody about whether that session is still one, and
what that is worth is smaller than it looks: `subscribe` writes topics onto a
connection that was identified when it opened, so a revocation that ends the
stream ends the grant with it. What is left is a browser sending a cookie the
application has retired, watching a fragment whose content is
viewer-independent by [the crate's own
invariant](../../crates/exos/src/live.rs). `disconnect` is the lever for that:
the half that knows says so, every tab under that name ends, and each comes back
asking who it is now. Closing it in general would need a fourth answer from
every `identify` and a database call on every change of a tab's visible fragment
set, against a design of one resolve per connection.

## CSRF is one rule

[csrf.rs](../../crates/exos/src/csrf.rs). An unsafe method has to carry the
`X-Exos` header, checked in one layer, outermost, for every route exos serves.
No token to derive, no form helper to remember, no secret to rotate, nothing
per route to get wrong.

The first draft of this argued a token was unnecessary because three
circumstances already stood in the way: `SameSite=Lax`, the header the runtime
sends, and `Json<T>` refusing the three content types a cross-origin form can
post. The gap was wider than the form-encoded handler it named. A handler taking
no body at all is outside the JSON leg too, and sign-out is exactly such a
handler; `SameSite=Lax` is same-*site*, so a sibling origin under the same
registrable domain sends the cookie with everything; and the header, the one leg
that distinguishes this application's runtime from anybody's page, was sent and
never required, so it defended nothing. The other two legs stay true and stay
unmentioned in the code: they narrow what an attacker can send, and the header
decides.

What it costs is that an unsafe request which did not come from the runtime is
refused: `curl -X POST` at an application's own routes, and a
`<form method="post">` submitted without JavaScript. Neither is a shape exos
serves. A webhook or an API for somebody else's program is mounted beside the
application, on the `Router` an `App` converts into, where it is outside the
session and the scope as well. The tax it does levy is on tests, one header per
helper, and the refusal says which header is missing.

## What it cost

- **No new dependencies.** `tower-sessions` and the tower stack behind it were
  forecast and not needed; `tower` is still a dev-dependency, and the crypto was
  already paid for by the live token.
- **No store round trip anywhere in exos**, and no store. Reading a cookie
  header is the whole of what a request carrying a name pays for.
- **Every application writes the same handful of lines** resolving a name into
  the scope. That is the price of the boundary and the thing to watch.

## Testing

The task-local is the better half of this for tests. `provide` is process-global
and its module docs warn that parallel tests interfere; a request scope is per
task, so two tests can hold different sessions at once without arranging
anything. What a test wants to seed is the viewer, which is application data,
and `with_scope` already does that. `session()` under `with_scope` mints and
rotates exactly as it would in a request.

```rust
exos::with_scope(|| {
    exos::scope().set(Viewer { id: 7, team: 3 });
    assert_eq!(badge().into_string(), "<span class=\"badge\">2</span>");
});
```

## Open questions

- **Cached viewer or a lookup per request.** Resolving on every request is a
  query per request; caching means a revoked role stays live. Plainly the
  application's call, and the guide should say so rather than leaving it
  unmentioned.
- **One resolver hook, or two.** `identify` is an async function from a name to
  what the *stream* needs. The handler half is still the six lines every
  application writes into the scope. The two want different answers out of one
  lookup, audiences on one side and a viewer in the scope on the other, and
  collapsing them before something has written both is how the store happened.
- **A `reconnect` step** is still worth building, for a reason that has nothing
  to do with the cookie race: sign-in drops a whole document in order to drop
  one stream, and that flash is the only reason the tab that signed in has to be
  rebuilt at all.
- **Multi-instance.** Sessions cross instances if the application's database
  does. What does not is the connection registry, which belongs to [more than
  one instance](../roadmap/more-than-one-instance.md).
