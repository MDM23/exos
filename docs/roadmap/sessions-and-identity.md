# Sessions and identity

Somewhere to put who this is, and a way to reach them.

Status: design. Nothing here is implemented.
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

`data` in [context.rs](../../crates/exos/src/context.rs) is process-global and
argues for itself on ergonomics: a view three levels deep should reach the
database handle without every caller above it accepting and forwarding one.
The same argument applies to the current request, and the same answer works,
except that the value changes per request rather than per process.

A task-local, set by a layer in `app()`. The alternative, an axum extractor,
does not fit for one specific reason: **a `view!` fragment is a plain
function, not a handler.** It cannot extract anything, and making it able to
would mean threading a parameter through every template in the application,
which is exactly the cost `data` exists to avoid.

Two rules fall out, both of them worth building in rather than documenting.

**Outside a request there is no scope, and asking is a panic.** Not `None`.
This mirrors what `data` already does for an unprovided type, for the same
reason: a background job reading the session is a programming mistake made
once, not a condition every caller should handle. `None` would quietly render
the logged-out view of something and then publish it.

**A live fragment must never see it.** The body of `#[exos::live]` renders
twice, inline during a request and again from whatever publishes it, so a
fragment reading the session would produce different HTML in the two places
and break the topic invariant. The macro already wraps the body in a closure:

```rust
let __markup: ::exos::Markup = (move || #body)();
```

Wrapping that call in a scope mask makes `session()` panic inside a fragment
always, during a request as much as outside one, and turns a rule that is
currently a paragraph in the docs into a compile-and-run failure at the exact
spot. A fragment's arguments are its whole input, and this is what says so.

The cost is one tokio feature: `task_local!` needs `rt`, and the `exos` crate
currently takes `sync` and `time`.

## Stage 2: the session

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

`live.rs` mints a process secret at startup and says why in a comment: a
restart drops every stream anyway, so no token outlives the process. Sessions
break that. A session that dies on deploy is not a session, and two instances
behind a load balancer never agree.

So: one configured key, and everything that needs one derives a subkey from it
by label, so `live-token`, `csrf` and anything later are independent and none
of them is the key itself.

```rust
exos::keys(Keys::from_secret(std::env::var("EXOS_SECRET")?));
```

Unconfigured, keep exactly today's behaviour, a per-process random, and say so
loudly at startup. It is right for `cargo run` and wrong for everything else,
and the failure mode without a warning is a deploy that logs everybody out.

That closes the README's first gap:

```text
token = HMAC-SHA256(subkey("live-token"), topic_id || session_id)[..16]
```

Verification at `/_exos/subscribe` gains the session id, which it has, because
the request carries the cookie. Three things follow. Rendering a live fragment
becomes a session write, so an anonymous visitor served one gets a cookie.
Rotation invalidates outstanding subscriptions, which is stage 4's `reload`.
And caching does not suffer, because [`Page`](../../crates/exos/src/response.rs)
already answers `no-cache, private`.

The dependency cost is real and small: `sha2` is already in the workspace for
content hashing in `exos-build`, so this adds `hmac` and a CSPRNG to `exos`
itself.

## Stage 7: CSRF, which is mostly already handled

The README lists this as a gap, and the analysis is better than the entry
suggests. Three things already stand between an attacker's page and a state
change.

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

- **A store round trip per request that reads the session.** Lazy loading is
  what keeps it off the requests that do not, and the layer must therefore
  load on first read rather than eagerly on the way in.
- **Two dependencies in `exos`**, plus `tower-sessions` and the tower stack
  behind it. `tower` is currently a dev-dependency only.
- **A task-local on every request**, which is nothing, and one tokio feature.
- **The store becomes a hard dependency of running the application.** In
  memory it is not, and the moment it is Redis, a session store outage is a
  sign-in outage. Worth stating, not worth avoiding.

## Testing

The task-local is the better half of this for tests. `provide` is
process-global and the module docs already warn that parallel tests interfere;
a request scope is per task, so two tests can run different sessions at once
without arranging anything. That wants a helper:

```rust
exos::test::with_session(Principal { id: 7, team: 3 }, || {
    assert_eq!(badge().as_str(), "<span class=\"badge\">2</span>");
});
```

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
