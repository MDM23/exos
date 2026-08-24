# Status

Early. The shape is settled and the pieces work together, but this has not
carried a real application yet.

Known gaps, roughly in priority order.

**A message cannot reach the browser.** [`locales!`](languages) declares an
application's languages and `messages!` declares its text, as ordinary Rust
functions whose arms the compiler holds to every language, so a locale nobody
translated a message into is a build failure rather than an English string in a
German page. What is missing is the crossing: a message whose count comes from
client state should cross as its variants and let `Intl.PluralRules` pick,
which is what keeps catalogs on the server.

**Expressions are compiled with `new Function`**, which a strict CSP without
`unsafe-eval` blocks. A precompiled mode is the answer.

**Running more than one instance.** The connection registry is a process-local
map, so a [publish](live-fragments) reaches only the tabs connected to the
instance that sent it. It needs a bus, and it is the one gap here that cannot
be added quietly later.

**No CSRF token.** `SameSite=Lax` on the session cookie, the `X-Exos` header
and JSON-only bodies are three defences rather than one, which is a policy and
is written down under [cross-site requests](sessions#cross-site-requests). It
leaks for a handler that accepts a form-encoded body, and that is when a token
should be built.

## The roadmap

Where each of those is going is written down in
[docs/roadmap](https://github.com/MDM23/exos/tree/main/docs/roadmap).
[Sessions and identity](https://github.com/MDM23/exos/blob/main/docs/roadmap/sessions-and-identity.md)
is the one most of the others wait on and names both the browser and, on a live
stream, who it belongs to.
[Directed effects](https://github.com/MDM23/exos/blob/main/docs/roadmap/directed-effects.md)
is built as far as pushing an effect to a person.
[Loose ends](https://github.com/MDM23/exos/blob/main/docs/roadmap/loose-ends.md)
collects the smaller work that waits for nothing.

[What exos does not do](limits) is the other half of this page, and is about
decisions rather than gaps.
