# Trusted proxies

Who is on the other end of a request, and whether the connection it came over
can hold a session.

Status: not built. exos never sees a peer address today: every example serves
with a plain `axum::serve(listener, app())`, which hands the application no
`ConnectInfo`, and nothing in the crate reads a forwarded header.

## The failure it starts from

The session cookie is always `Secure`, and a browser keeps one of those only
from a secure context. `localhost` counts; plain HTTP on any other address does
not, which is a phone on the same network at `http://192.168.1.20:3000` or a
staging box without a certificate. There the cookie is dropped: nobody stays
signed in, and every live fragment fails verification and stops updating,
without an error anywhere.

A development build dropping `Secure` makes a debug binary behave differently on
a network from the release it becomes. A warning in the log is honest and still
quiet: the page renders, half of it works, and nobody reads the server's output
from a phone. So the answer is an error, served in place of a page that would
pretend to work.

Deciding when to serve it is the whole problem. Behind a proxy that terminates
TLS, every request reaches the application as plain HTTP, usually from a
private address, so judging by the connection alone would refuse every correct
production deployment. The application has to be told which peers are proxies,
and once it knows that, the same knowledge answers a second question it cannot
answer today: which address the visitor is actually at.

## What axum gives

Less than it looks.

- **The peer address** is `ConnectInfo<SocketAddr>`, present only when the
  router is served through `into_make_service_with_connect_info`.
- **Forwarded headers** have no handling in axum 0.8. The `Host` extractor
  moved to axum-extra and trusts `X-Forwarded-Host` from anybody.
- **`axum-client-ip`** picks one header source, such as the rightmost
  `X-Forwarded-For`, and has no notion of which peers are allowed to set it.

So trusting a proxy at a given address is exos's to write.

## Three kinds of peer

- **Loopback** is served as it is. The browser treats it as a secure context,
  so the cookie survives, and it is where `cargo run` lives.
- **A trusted proxy** is served as it is, never refused. The scheme and the
  client's address are read out of the header the proxy sets.
- **Anybody else over plain HTTP** is refused with an error page, unless the
  application opted out.

The opt-out is the escape hatch for a development server reached over the
local network, and for a staging box without a certificate. It has to drop
`Secure` from the cookie as well, or the browser still throws the cookie away
and the opt-out fixes nothing but the error.

## Where the peer address comes from

The decision is a layer inside `seal` in
[app.rs](../../crates/exos/src/app.rs), beside the csrf layer, because both ways
of serving an application go through `seal`: `axum::serve(listener,
exos::app())` and `Router::from(exos::app())` nested into something larger.
Everything exos does per request already lives there, and this must not be the
first thing that only one path gets.

The layer reads axum's own `ConnectInfo<SocketAddr>` rather than a type of
exos's. Two sources fill it:

- **The per-connection hook** on `App`, which `axum::serve` already calls with
  the incoming stream, inserts it. That keeps the ordinary path free of setup.
- **An application serving the router itself** writes
  `into_make_service_with_connect_info::<SocketAddr>()`, which is what axum asks
  of any application that wants a peer address.

With neither, the peer is unknown, and an unknown peer is served and has no
client address. Refusing it would refuse every `oneshot` test, exos's and every
application's, since those arrive with no connection at all.

## Reading the forwarded header

The proxy appends the address it received from, so the list is read from the
right. Each entry that is a trusted proxy is skipped, and the first one that is
not is the client. If every entry is trusted, the leftmost is the client.
Reading from the left instead takes whatever the visitor wrote into the header
before the proxy appended to it.

The setting names the header, and exos reads that one and no other. A proxy
that sets `X-Forwarded-For` passes a `Forwarded` header from the visitor through
untouched, so reading both, or preferring the standard one where it is present,
lets anybody forge their address past a correctly configured proxy.

## Stage 1: the peer address

The hook inserts `ConnectInfo`, and the layer in `seal` reads it. Nothing is
decided yet, and a request looks exactly as it did.

## Stage 2: trusted proxies

A setting on `App` lists the proxies, by address or by range, and names the
header they set. The layer works out the scheme and the client's address, and
puts both in the [scope](../../crates/exos/src/scope.rs), where a handler, a
middleware and a view read them the way they read the session.

## Stage 3: refusing insecure transport

The layer refuses plain HTTP from anybody but loopback and a trusted proxy, and
the escape hatch turns that off and drops `Secure` from the cookie. The
[sessions](../site/content/sessions.md) page currently says `Secure` is not
configurable, and changes with it.

## What it costs

- **A setting in production.** An application behind a proxy that has not
  named it is refused after upgrading, which is the point and still a breaking
  change.
- **A setting for anybody serving the router themselves**, who has to ask axum
  for connect info or get no client address.
- **A second way for the cookie to look**, which the session tests have to
  cover rather than assume.

## Testing

- **A client forging its address is not believed.** An entry written to the
  left of the proxy's own is skipped past, never taken.
- **The header not named is ignored**, even where it disagrees with the one
  that is.
- **IPv4 mapped into IPv6 is still loopback.** A dual-stack listener reports
  `::ffff:127.0.0.1`, and `Ipv6Addr::is_loopback` says no to it.
- **A request with no connection is served**, so a `oneshot` against the
  application keeps working.
- **The escape hatch drops `Secure`**, and nothing else on the cookie.

## Open questions

- **The setting's shape**: one method taking proxies and header together, or
  two.
- **Whether ranges are parsed here** or through a crate, for the sake of a few
  lines.
- **What the error is**: which status, and how much of this document it
  explains to whoever sees it.
- **Whether the escape hatch is allowed in a release build.** A staging box
  needs it there, and a production binary with it on is the mistake it enables.
- **Whether exos reads the forwarded host too.** Nothing in the crate builds an
  absolute URL today, so it has no reader yet.
