# Testing

Three rungs, and most of what an application does is testable on the first two.

## A view is a string

`view!` renders without a server, a request or a runtime:

```rust
#[test]
fn a_track_shows_its_length() {
    let markup = track(&Track::new("Nights in White Satin", 448));
    assert!(markup.as_str().contains("7:28"));
}
```

## A handler is a function

A route attribute leaves the function alone, so a handler can be called with
its arguments and its answer read back. An `Effect` is a list of steps, and the
list is public:

```rust
#[tokio::test]
async fn removing_clears_the_selection() {
    exos::provide(Room::seeded());

    let effect = remove(Model(Selection { picked: vec![3] })).await;

    assert_eq!(effect.steps().len(), 2);
    assert!(matches!(effect.steps()[0], Step::Signals(_)));
}
```

No HTTP, no router, no runtime. Reach for this whenever what is being checked
is the decision the handler made rather than how the request reached it.

## A browser is the rest

Routing, extraction, sessions, refusals and the live stream need a request, and
a request needs the things a browser does: cookies, the header the runtime
sends, an open stream, and a subscription for every fragment on the page. The
[`exos-test`](https://docs.rs/exos-test) crate is that browser.

```toml
[dev-dependencies]
exos-test = "0.1"
```

```rust
use exos_test::Browser;

#[tokio::test]
async fn a_removal_reaches_the_other_tab() {
    let mut tab = Browser::new(app());
    let page = tab.get("/tracks").await;

    assert!(page.body().contains("Nights in White Satin"));

    let answer = tab.post("/tracks/remove", &Selection { picked: vec![3] }).await;

    assert_eq!(answer.status(), StatusCode::OK);
    assert_eq!(answer.steps(), [Step::Scroll(String::from("#queue"))]);

    // What the publish in that handler pushed down the live stream.
    let Step::Patch(markup) = tab.next().await else { panic!() };
    assert!(!markup.as_str().contains("Nights in White Satin"));
}
```

| on a browser | does |
| --- | --- |
| `get(url)` | loads a page, or makes a call that answers with an effect |
| `post(url, &model)` | an action carrying a model |
| `call(method, url)` | an action carrying nothing |
| `send(request)` | anything else, still as this browser |
| `next()` | the next step off the live stream |
| `listen()` | opens the stream where no fragment asked for one |
| `watching()`, `cookie(name)` | what the tab is watching, and what it holds |
| `with_cookie(name, value)` | a browser that turns up already holding one |

Two tabs are two browsers, which is how a test checks that what one of them did
reached the other.

## What it is watching

Nothing has to be said about subscriptions, because the runtime does not keep
them either: it reads the document after every change it applies, and the tab
watches whatever `<exos-live>` elements are in it. A browser follows the same
rule from the other side.

| what arrived | what it watches after |
| --- | --- |
| a document | the fragments on it, and nothing from the page before |
| a `page` step | the same, since that is a navigation that saved a fetch |
| a `navigate` or `reload` step | nothing, until the next document |
| a `patch` step | what it watched, plus any fragment the patch brought |

A `navigate` is not followed. The runtime fetches the URL it names; a test says
where it went by opening that URL, rather than having a request made for it.

## What a browser is not

It is not a DOM. Nothing morphs markup, evaluates a binding or runs an event
handler, because a second implementation of the runtime would be a second thing
to keep true. What it models is the half of a tab the server can see: what was
asked for, what came back, and which fragments the tab then said it was
watching.

That leaves one thing untested from an application's side, and it is the same
thing for every application: whether the runtime does the right thing with a
step it is sent. That is not yours to test. It is checked in `npm test`, in the
runtime's own suite, against the same wire format your handler answered with.

It also leaves one thing a browser cannot follow. A fragment goes when its
element does, and only a document says what is left, so a patch over the region
holding a fragment, or a `remove` of it, stops a real tab watching it and does
not stop this one. Such a tab receives a publish the real one would not. A test
about a fragment leaving the page should load the page that no longer holds it.

Write browser tests against what the server decided. Reach for a real browser
only if you ship JavaScript of your own.

## One process, one application

`provide` stores by type for the whole process, and cargo runs a crate's tests
in parallel, so two tests that provide different values of the same type will
interfere. Give each test its own types, or seed once and have the tests touch
disjoint data. The same goes for live fragments: two tests sharing a topic
watch each other's publishes.
