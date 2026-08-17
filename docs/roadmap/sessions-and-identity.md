# Sessions and identity

Somewhere to put who this is, and a way to reach them.

Status: partly built, and out of order. Stage 1 is done, and so is the key
material stage 6 asks for; the session itself, which is what everything else
waits on, is not. Each stage below says where it stands.
[Directed effects](directed-effects.md) depends on all of it, and so does form
validation, localization, and the fix for the live token that the README lists
first among known gaps.

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

What exos owns is narrow: a cookie, a request scope, a place to keep typed
values across requests, and the mapping from that to who a live stream belongs
to.

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

What is left for the session is only what goes *in* the scope.
`Scope::get` already answers with an `Option` for exactly that reason: nothing
written yet is what anonymous will look like.

## Stage 2: the session

**Not built, and next.** Everything below this line waits on it, and so does
the whole of [directed effects](directed-effects.md).

An opaque random id in a cookie, and the contents in a store.

```text
Set-Cookie: exos=<128 random bits, base64url>;
            HttpOnly; SameSite=Lax; Secure; Path=/
```

**Not signed data in the cookie.** The store is the authority, so there is
nothing to forge and no key to rotate, and revocation is a delete rather than
a deny-list. Stateless cookie sessions trade that away for not having a store,
which is the wrong trade here: sign-out has to be able to reach a stream that
is already open, and a token nobody can revoke cannot do it.

**Lazy.** No cookie until something is written. A crawler, a health check, or
an anonymous read gets no `Set-Cookie` and costs no store round trip. The
first write materialises an id and the layer appends the header on the way
out. One edge worth knowing: a session materialised *during* a stream cannot
set a cookie, because those headers went out when the stream opened. Streams
should read the session, never write it.

**Typed contents, keyed by type**, which is what `provide` and `data` already
do, and this is the same idea with a narrower lifetime:

```rust
#[derive(Deserialize, Serialize)]
struct Principal {
    id: u32,
    team: u32,
}

let session = exos::session();

session.set(&Principal { id: 7, team: 3 })?;
let who: Option<Principal> = session.get()?;
```

There is deliberately no `exos::viewer()`. Who the user is belongs to the
application, so it is ordinary session data under the application's own type,
and exos never grows an opinion about its shape. Anonymous is
`session().get::<Principal>()? == None`, which is a value rather than a case
the framework invented.

## Stage 3: buy the store, own the scope

**Not built.** It is the same piece of work as stage 2 and is separated only
because the decision it records is about a dependency rather than about a
design.

`tower-sessions` already does the cookie, the lazy materialisation, the store
trait, and the layer. [rust.md](../../.claude/rules/rust.md) says to prefer a
small well-maintained crate over writing it, and this is that situation: none
of the machinery above is where exos is interesting, and all of it is where
session bugs live.

Two conditions on taking it.

- **It gets wrapped, not re-exported.** It is still pre-1.0, and a public
  dependency below 1.0 makes every one of its releases a major version of
  exos. `exos::Session` is exos's own type with exos's own methods, and what
  is behind it is an implementation detail that can be replaced.
- **Its `Session` never appears in a handler signature.** The layer puts it in
  the task-local; nothing extracts it. That is the whole point of stage 1, and
  it is also what keeps the wrapping honest, since a type nobody names is a
  type that can change.

Ship a memory store for development and tests, the way the example already
seeds `Files` and `Presence` in memory. Redis, Postgres and the rest are the
application's to bring, and the trait is the seam.

## Stage 4: signing in and out

**Not built.**

```rust
#[exos::post("/session")]
async fn sign_in(Json(form): Json<Credentials>) -> Result<Effect, Error> {
    let user = data::<Users>().authenticate(&form).await?;
    let session = exos::session();

    // A new id, or the one an attacker planted before sign-in still works.
    session.rotate()?;
    session.set(&Principal { id: user.id, team: user.team })?;

    Ok(Effect::reload())
}
```

`rotate` is the fixation defence and cannot be defaulted, because only the
application knows which request is the privilege change. Making it a separate
call means it can be forgotten, so `set` on a session that has no principal
yet is a reasonable place to do it automatically. Worth deciding when it is
written; the safe version is to rotate whenever the principal type is written.

Two consequences reach further than this handler, and both are why sign-in
answers with a `reload` rather than a patch.

- **Rotation invalidates every live token**, once stage 6 binds them to the
  session id. A fragment still on screen would silently stop updating.
- **A stream's identity is captured when it opens.** The runtime morphs the
  body on navigation without reopening the `EventSource`, so a tab that signs
  in as somebody else keeps the previous audience until the stream drops. That
  is a leak, and it is the same one [directed effects](directed-effects.md)
  names in its stage 1.

A `reload` closes both by dropping the document. A `reconnect` step that drops
and reopens the stream in place would close both without the flash, and is the
better answer once something needs it.

Signing out everywhere, rather than here, needs an index from principal to
session ids: a store concern, and cheap if the store is asked for it up front
rather than retrofitted. Reaching the tabs afterwards is a directed effect to
that audience, which is the first thing the two documents share.

## Stage 5: identity on the stream

**Not built**, though the connection it would hang identity on is now the
server's to name: the id is minted server-side and sent as the stream's first
event, rather than invented by the client and trusted.

With sessions in place, the resolver [directed
effects](directed-effects.md) asks for stops being a lookup and becomes a pure
function of what the session already holds:

```rust
exos::identify(|session| match session.get::<Principal>() {
    Ok(Some(who)) => Audiences::of(Viewer(who.id)).and(Team(who.team)),
    _ => Audiences::none(),
});
```

The framework does the loading, once, on the stream's `GET`. The application
says only what its own data means. That is a better shape than the sketch in
the other document, which had the resolver resolving a cookie itself, and it
is the payoff for doing sessions first.

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

**What is not built is the binding**, which is the half that closes the
README's first gap. Today a token is the MAC of the topic id alone, so it
proves this server rendered the fragment and not that this viewer was served
it. The message gains the session id:

```text
token = HMAC-SHA256(subkey("live-token"), topic_id || session_id)[..16]
```

Verification at `/_exos/subscribe` gains the session id, which it has, because
the request carries the cookie. Three things follow. Rendering a live fragment
becomes a session write, so an anonymous visitor served one gets a cookie.
Rotation invalidates outstanding subscriptions, which is stage 4's `reload`.
And caching does not suffer, because [`Page`](../../crates/exos/src/response.rs)
already answers `no-cache, private`.

It is a small change to
[`Topic::token`](../../crates/exos/src/live.rs) and its verifier, and it cannot
be made until there is a session id to put in it. That is the whole argument
for doing stage 2 next.

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

Together that is a policy rather than the start of one, and it is worth
writing down in the guide as the reason exos ships no token. The gap is real
but narrow: a handler that accepts a form-encoded body steps outside all three
at once. The answer is a token derived from the session id, rendered by a
helper into the form, and it should be built when the first form handler
exists rather than before.

## What it costs

What is left to pay, now that stages 1 and 6's keys are in and cost what they
said they would:

- **A store round trip per request that reads the session.** Lazy loading is
  what keeps it off the requests that do not, and the layer must therefore
  load on first read rather than eagerly on the way in.
- **`tower-sessions` and the tower stack behind it.** `tower` is currently a
  dev-dependency only. The two crypto dependencies this section used to
  forecast are already paid for.
- **The store becomes a hard dependency of running the application.** In
  memory it is not, and the moment it is Redis, a session store outage is a
  sign-in outage. Worth stating, not worth avoiding.

## Testing

The task-local is the better half of this for tests, and it already works.
`provide` is process-global and its module docs warn that parallel tests
interfere; a request scope is per task, so two tests can hold different
sessions at once without arranging anything.

`with_scope` is the helper, and it shipped with stage 1:

```rust
exos::with_scope(|| {
    exos::scope().set(Principal { id: 7, team: 3 });
    assert_eq!(badge().into_string(), "<span class=\"badge\">2</span>");
});
```

Whether the session wants a second helper that seeds it specifically, rather
than callers reaching for `scope().set`, is worth deciding when there is a
session to seed. The argument for one is that a test should not have to know
which type the session is kept under.

## Open questions

- **Concurrent writes clobber.** Two actions in flight from one tab each load
  the session and each save it whole, and one loses a key. Per-key writes, or
  a compare-and-swap on a version, and this is the sort of thing a store trait
  has to decide before it has implementors.
- **Cached principal or a lookup per request.** Keeping the principal in the
  session means a revoked role stays live until sign-out; looking it up means
  a query on every request. Probably: keep an id in the session, let the
  application decide what to load from it, and do not pretend to know.
- **Idle timeout, absolute lifetime, or both.** Both, with the absolute one
  off by default.
- **Multi-instance.** Swapping the store fixes sessions across instances and
  does nothing for the connection registry, which is a process-local
  `HashMap`. A directed effect only reaches the tabs connected to the instance
  that sent it. That belongs to [directed
  effects](directed-effects.md) and needs a bus, not a session store.
