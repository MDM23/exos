# Glossary

Every public item, grouped by what it is for. The guide explains why each of
these exists; this page is for when you know what you want and need its name.

Everything down to [the browser runtime](#the-browser-runtime) is written
against directly. What comes after it is public because it stands in one of
these signatures or because a macro expansion has to name it, and is listed
separately for that reason. Items hidden from rustdoc appear in neither.

## Macros

Everything a macro generates is checked by the compiler on both sides, so a
rename is an error rather than a page that quietly stops working.

| item | what it does |
| --- | --- |
| `view!` | Compiles [real HTML](templates) into string pushes; a braced block is Rust |
| `#[get]`, `#[post]`, `#[put]`, `#[patch]`, `#[delete]` | Registers a handler at its path and generates the [typed caller](calling-the-server) beside it |
| `#[model]` | Makes a struct both [client state and a request body](models) |
| `#[live]` | Makes a function a [fragment that keeps itself up to date](live-fragments) |
| `asset!` | Builds and embeds a [file](assets) while the crate compiles |
| `locales!` | Declares the [languages](languages) the application is built in |
| `messages!` | Declares text in every one of them |
| `#[derive(Enumerable)]` | Lets a message branch on an enum of your own |
| `#[valid(...)]` | Declares a field's [rules](models#rules-on-a-model), answered on both sides |

## The application

One call builds the router. There is no second list of routes, and no
configuration for where the application is mounted.

| item | what it does |
| --- | --- |
| `app()` | Every discovered route, the assets, the stream and the endpoints, as an `axum::Router` |
| `base(path)` | Says the mount prefix explicitly, for a proxy that strips one the server never sees |
| `base_path()` | The prefix, discovered or told |
| `url(path)` | That prefix in front of a path |
| `name::url(..)` | The URL of one route, from its own path and parameter types |
| `name::get(..)`, `name::post(..)`, … | Records a call to it, with the payload type checked |
| `Page` | A whole document, answering `no-cache, private` |
| `Markup` | A fragment, as a response and as the type every template produces |

## Rendering

The rule is one line long: everything is escaped except `Markup`, which only
`view!` produces.

| item | what it does |
| --- | --- |
| `Markup` | Markup that writes through unescaped, with `as_str`, `into_string`, `is_empty` |
| `Render` | What a value implements to be interpolable: `render_to`, `render` |
| `AttributeValue` | The same for attribute position, where `None` omits the attribute rather than emptying it |
| `Flag(bool)` | A present-or-absent attribute |
| `escape_into` | Escapes text into a buffer, for a hand-written `Render` |
| `escape_display_into` | The same for anything that is `Display` |

## Attributes

Each of these is written as a block on an element, and blocks are merged, so
repeating one is the style rather than a mistake. See
[binding to the DOM](bindings).

| item | what it does |
| --- | --- |
| `text(expression)` | Keeps text content in sync |
| `show(condition)` | Toggles `hidden` |
| `class(name, condition)` | Toggles one class |
| `attr(name, value)` | Sets one attribute |
| `prop(name, value)` | Sets one property, for `value`, `checked` and friends |
| `bind(&signal)` | Two-way binding for a form control, over a signal or a model field |
| `preserve()` | Keeps an element out of every morph |
| `Attr::new(name, value)` | The escape hatch for an attribute nothing above writes |

## Events

Handlers are delegated, so an element rendered later is already wired. The
closure runs on the server at render time and records what it does; see
[handlers](handlers).

| item | what it does |
| --- | --- |
| `on_change`, `on_click`, `on_dblclick`, `on_focusout`, `on_input`, `on_keydown`, `on_submit` | One handler for the event it names |
| `on(EventType, handler)` | The same for the rest, custom types included |
| `EventType` | `Change`, `Click`, `DblClick`, `FocusIn`, `FocusOut`, `Input`, `KeyDown`, `KeyUp`, `PointerDown`, `PointerUp`, `Submit`, `Custom(&str)`, and `name()` |
| `Event::target()` | The element the event came from |
| `Event::key()` | Which key it was |
| `Event::prevent_default()` | Suppresses what the browser would do |
| `Event::stop_propagation()` | Stops it bubbling |
| `Target::value()` | The control's value |
| `Target::checked()` | Whether it is ticked |

## Expressions

A `Js<T>` is a JavaScript expression that will produce a `T`. You never write
one by hand: they come out of handles, callers and the combinators below. See
[expressions](expressions).

| item | what it does |
| --- | --- |
| `Js::raw(source)` | An expression written out, for what the combinators cannot say |
| `Js::source`, `Js::into_source` | The source back out |
| `Js::cast::<U>()` | The same expression, at another type |
| `concat` | String concatenation, at any type |
| `and`, `or`, `not`, `!` | On `Js<bool>` |
| `eq`, `ne` | On `Js<String>`, `Js<bool>`, `Js<f64>`, `Js<i32>`, `Js<u32>` |
| `gt`, `lt`, `ge`, `le`, `plus`, `minus` | On `Js<f64>`, `Js<i32>`, `Js<u32>` |
| `is_empty`, `len`, `contains`, `to_lowercase`, `trim` | On `Js<String>` |
| `len`, `is_empty`, `any`, `contains` | On `Js<Vec<T>>` |
| `when(condition, body)` | The branch that survives to the browser, since `Js<bool>` is not `bool` |
| `debounce(millis, body)` | Holds a body back until the typing stops, keyed by the call site, with last-response-wins |
| `record(body)` | Collects what a body emitted, as source |
| `emit(statement)` | Appends one statement to what is being recorded |
| `attr_now(name, value)` | A speculative DOM write, for [optimistic updates](calling-the-server#optimistic-updates) |
| `focus_now(selector)` | Moves the caret during a handler |
| `append(template, into)` | Clones a `<template>` into a container |
| `quote_js(value)` | Quotes a string into source, for a hand-written `Js::raw` |

## Client state

A signal is state the server does not own. It is declared once, in Rust, and
the handle is the only way to reach it; see [signals](signals).

| item | what it does |
| --- | --- |
| `signal(initial)` | Declares one, named after where it was declared |
| `Signal::get()` | Its value, as an expression |
| `Signal::set(value)` | Writes it |
| `Signal::name`, `Signal::initial`, `Signal::placement` | What it was declared as |
| `Signal<bool>::toggle()` | Flips it |
| `Signal<Vec<T>>::clear`, `push`, `toggle_member` | Collection writes |
| `Bound<T>` | What a model field's handle holds: a signal that knows its rules, and derefs to `Signal<T>` |
| `Bound::error()` | What is wrong with this field, as an expression |
| `Bound::invalid()` | Whether anything is |

A signal from `signal` belongs to the element that declares it, so a hundred
rows repeating one name hold a hundred values. A `#[model]` field belongs to
the document, because a handler writes it with `Effect::set` and the client
applies that against the document root.

## Models and forms

One declaration is the client state, the request body and the rules. See
[models](models).

| item | what it does |
| --- | --- |
| `Model::signals()` | The handle, with one field per field |
| `Model::FIELD` | A typed token per field, for naming one in a refusal |
| `handle.valid()` | Whether nothing in the model is complaining |
| `handle.dirty()` | Whether any of its controls has been edited |
| `handle.refusal()` | What a handler said about the submission rather than a field |
| `handle.refused()` | Whether there is one |
| `Model<T>` | The body extractor, which renames, deserializes and checks before a handler runs |
| `to_wire(&value)` | A model as the body the extractor reads, for a server that already knows what to send |
| `Rows<T>` | The field type for [repeating groups](models#repeating-groups): `iter`, `len`, `is_empty` |
| `RowsOf<T>` | Its handle: `key`, `add`, `remove`, `error`, `invalid`, `each(render)` |

A row's handle derefs to the row model's own, and declares the row when put on
its root element. Adding and removing a row costs no round trip.

## Validation

A rule is written once, on the field it is about, and generates both halves:
the browser answers it as the field is typed into, the extractor answers it
again before the handler body runs.

| item | what it does |
| --- | --- |
| `required` | Filled in at all |
| `required_with = sibling` | The same, whenever a sibling is |
| `length = 2..=40` | Counted in UTF-16 code units, so both sides agree |
| `email` | Shaped like an address |
| `checked_by = function` | The one rule with no browser half, asked over a round trip and again at submit |
| `Violation` | `Required`, `TooShort { least }`, `TooLong { most }`, `Malformed` |
| `complaints(say)` | How this application words a violation; the default is English |
| `Refusal<M>` | A handler's own refusal, in the shape a declared rule produces |
| `Refusal::add(field, message)` | Says what is wrong with one field |
| `Refusal::say(message)` | Says what is wrong with the submission |
| `Refusal::is_empty()` | Whether anything is |
| `Presence` | Implement it and `required` can be asked about your type: `is_present`, `present` |
| `Length` | The same for `length`: `measure`, `length` |

## Effects

One type for everything a handler asks the client to do, applied in the order
given whatever status carries it. See [effects](effects).

| item | what it does |
| --- | --- |
| `Effect::none()` | Nothing to do |
| `Effect::patch(markup)` | Morph this HTML into place, keyed by its ids |
| `Effect::set(&signal, value)` | Merge into the client's signal store |
| `Effect::remove(selector)` | Delete the elements matching it |
| `Effect::navigate(url)` | Client-side navigation |
| `Effect::page(markup)` | Replace the active page without a second fetch |
| `Effect::title(text)` | Retitle the document |
| `Effect::reload()` | Reload it. The last resort |
| `and_patch`, `and_set`, `and_remove`, `and_navigate`, `and_page`, `and_title` | The same again, on an effect that has already started |
| `focus(selector)`, `scroll(selector)` | Move the caret, or the viewport |
| `steps`, `into_steps`, `to_stream` | What it holds, and how it is framed |
| `Step` | One instruction: `Focus`, `Navigate`, `Page`, `Patch`, `Reload`, `Remove`, `Scroll`, `Signals`, `Title` |
| `EffectStream::new(stream)` | Answers with effects as they are computed, for a handler too slow to answer at once |

## Live fragments

Markup with a name the server chose, so subscribing is authorized by
construction. See [live fragments](live-fragments).

| item | what it does |
| --- | --- |
| `Fragment` | What a `#[live]` function answers with: render it in a template, or publish it |
| `Fragment::topic`, `markup`, `to_markup` | Its parts |
| `publish(…)` | Takes a closure rendering a fragment, and pushes it to everyone watching |
| `connection_count()` | How many streams this node holds |

## Identity

A subscription says what a tab is showing. An audience says who it belongs to,
and is written by the server; see
[who a stream belongs to](live-fragments#who-a-stream-belongs-to).

| item | what it does |
| --- | --- |
| `Audience` | What your type implements to be addressable, through `NAME` and `Hash` |
| `Audiences::of(&audience)` | One of them |
| `Audiences::and(&audience)` | And another, so a connection can be a viewer and a team at once |
| `Audiences::none()` | Nobody, which is what an unrecognised name resolves to |
| `identify(resolver)` | Says what a session name stands for. Once per process, before serving |
| `send(&audience, &effect)` | Pushes an effect to a person wherever they are |
| `connected(&audience)` | Whether anybody by that name is streaming. A hint, never a guarantee |

## More than one instance

exos ships no broker adapter. What it ships is the two ends; see
[more than one instance](live-fragments#more-than-one-instance).

| item | what it does |
| --- | --- |
| `bus(cross)` | Registers the outbound half, a closure answering with a future |
| `deliver(frame)` | The inbound half, called from your own subscriber loop |
| `Frame::to_bytes`, `Frame::from_bytes` | The codec, which is exos's rather than yours |
| `Frame::kind`, `key`, `trace` | What one is addressed at, and the trace it belongs to |
| `Kind` | `Topic`, `Audience`, `Connection` |

A frame carries no session name, no connection id, no fragment arguments, no
application state and no token. With no bus registered, none of it is built.

## Sessions and keys

exos owns the part with no choices in it: an opaque name, and the cookie that
carries it. See [sessions](sessions).

| item | what it does |
| --- | --- |
| `session()` | The session of the request being served |
| `Session::id()` | The name the browser presented, if any. Asking does not start one |
| `Session::start()` | Mints one where there is none. Idempotent |
| `Session::rotate()` | A new name, which is what a sign-in does |
| `Session::end()` | Drops it |
| `Id` | The name itself: `random`, `parse`, `as_str`, and a `Debug` that does not print it |
| `Keys::from_secret(secret)` | The one key everything signed derives from |
| `Keys::random()` | A key for a process that has no secret to be given |
| `keys(keys)` | Installs it, once at startup. Unconfigured means a random key and a warning |

## Application state

Two lifetimes, both keyed by type, so a view three levels deep reaches what it
needs without every caller above it forwarding one. See
[application state](application-state).

| item | what it does |
| --- | --- |
| `provide(value)` | Stores one for the process |
| `data::<T>()` | Reads it, panicking where nothing provided it |
| `try_data::<T>()` | The same question asked politely |
| `scope()` | The current request's scope, which panics outside a request |
| `Scope::get::<T>()`, `Scope::set(value)` | What is in it |
| `with_scope(body)` | Opens one, which `app()` does for every request |
| `detached(body)` | Renders with no scope at all, which is what a live fragment does |

## Localization

The set of languages is generated, and every message is held to all of them by
rustc. See [languages](languages).

| item | what it does |
| --- | --- |
| `Locale::ALL`, `FALLBACK` | Every declared locale, and the one a request answers with when nothing better is known |
| `Locale::tag()`, `from_tag()` | The tag it was declared with, and back |
| `Locale::direction()` | Which way its script runs |
| `Locale::number(count)` | A whole number as that language writes one |
| `de::Plural`, `de::category(count)` | Exactly the categories CLDR gives that language, and which one a count is |
| `locale::<Locale>()` | The request's language: the scope, then `Accept-Language`, then the fallback |
| `lang(locale)` | What `<html>` carries: `lang`, and `dir` where the script runs right to left |
| `Direction` | `LeftToRight`, `RightToLeft`, `as_str` |

A `messages!` parameter declared `Plural` is a count, branched through that
language's own categories. One declared `Slot` is an `FnOnce(Markup) -> Markup`
wrapper, so a sentence with a link in it stays one message; `{b}` and `{i}` are
that with the wrapper already written. Anything else interpolates by `{name}`,
and branches where its type derives `Enumerable`. Given an expression rather
than a number, a message projects and `Intl.PluralRules` picks in the browser.

## Assets

Everything is content-hashed at build time, so a URL changes exactly when its
bytes do and caching is unconditional. See [assets](assets).

| item | what it does |
| --- | --- |
| `asset!("css/app.css")` | Builds, embeds, and answers with a handle |
| `asset!("data/blob.xyz", "application/octet-stream")` | The same, naming a type the web has none for |
| `Asset::url()` | Where it is served from, under the application's base |
| `Asset::bytes()` | The same file again, to compute whatever its content decides |
| `Asset::file()` | The hashed name |
| `runtime()` | The client runtime's URL, with `?dev` in a debug build |

A `.css` file is bundled through its `@import`s and its `url()`s are embedded
and rewritten; a `.js` or `.mjs` file is bundled through its `import`s;
anything else is embedded byte for byte. Release builds minify.

## The browser runtime

Events are delegated and bindings are observed, which is why markup that
arrives later needs no initialization. What follows is the surface a plugin of
your own uses.

| item | what it does |
| --- | --- |
| `window.exos.base` | Where this application's URLs start |
| `window.exos.signals` | The global signal namespace |
| `window.exos.setIn(el, name, value)` | Writes a signal in the scope that element resolves it to |
| `window.exos.readIn(el, name)` | Reads it as that element would see it |
| `window.exos.listen(type)` | Registers another delegated event type, which is what `EventType::Custom` promises |
| `window.exos.binding(name, make)` | Registers a binding attribute, applied to existing and future nodes |
| `window.exos.navigate(url)` | Client-side navigation |
| `window.exos.morph`, `applyPatch` | Morphing and patching by hand |
| `window.exos.effect`, `dispose`, `bindTree`, `unbindTree`, `evaluate` | Reactivity and the binding lifecycle |
| `window.exos.progress.start()`, `.done()` | Drives the bar for work the runtime does not make |
| `exos:busy`, `exos:idle` | Announced on `document` for every round trip, with `detail.kind` |
| `exos:mutated` | Announced when the observed tree changed |
| `data-sortable`, `data-sort-item`, `data-drag-handle` | The [sortable plugin](bindings), which talks to the server once, on drop |
| `data-exos-progress`, `data-exos-progress-delay` | Turns the bar off, or waits longer before it appears |
| `--exos-progress-color`, `-height`, `-shadow`, `-z-index`, `-duration`, `-fade` | What it looks like |
| `data-reload` | Opts a link out of client-side navigation |

The element a call came from carries `aria-busy` for the duration, which is
what says where the work is happening.

## Endpoints exos mounts

All of them sit under whatever prefix the application is mounted at.

| endpoint | what it is |
| --- | --- |
| `/_exos/live` | The stream, one per browser tab |
| `/_exos/subscribe` | Replaces what a connection is watching |
| `/_exos/check/{model}/{field}` | Answers one `checked_by` rule |
| `/_exos/<name>-<hash>.<ext>` | Every embedded asset |

## Exported, but not written by hand

None of this is something an application constructs or names on an ordinary
day. It is public because it stands in one of the signatures above, or because
a macro expansion has to reach it. Reading it is how the rest is understood;
writing against it usually means something above was meant to do the job.

| item | what it is |
| --- | --- |
| `Attributes` | The merged set an element ends up with. Named when writing an attribute helper of your own |
| `IntoAttributes` | What every handle and helper implements, with `write`. The other half of writing one |
| `Attr`, `Class`, `Bind` | What the attribute helpers answer with, put on an element rather than read |
| `BindKind` | Carries a bound field's Rust type across so the browser coerces before it sends |
| `Bindable` | The bound `bind` takes, satisfied by `Signal<T>` and `Bound<T>`, and not nameable from outside |
| `IntoJs<T>` | The bound every helper takes, so a call site can pass an expression or a plain value |
| `IntoPayload<T>` | The bound a generated caller's body takes |
| `call(method, url, payload)` | What a typed caller records. By hand it gives up the checking the caller exists for |
| `Placement` | `Element` or `Document`, which is where a signal's name lives |
| `Field<M>` | The type of the per-field tokens `#[model]` generates |
| `ModelRejection` | What the `Model<T>` extractor refuses with. Only `Refused` is meant for a viewer |
| `ModelFields` | The wire-name table behind the renaming |
| `RowModel` | What a model implements so it can be somebody's row |
| `Errors` | The record every message lands in, keyed by wire name |
| `Validate` | What `#[model]` implements, and the bound `Model<T>` and `Refusal<M>` take |
| `CheckEntry` | One registration per `checked_by` field, for the check route to resolve |
| `Topic` | What `Fragment::topic` answers. A name is derived rather than written |
| `Resolution` | The result an `identify` resolver answers with |
| `Sent` | The result a `bus` closure answers with |
| `LocaleSet` | What `locales!` implements and what `locale::<L>()` resolves through |
| `Lang` | What `lang(locale)` answers with, put on `<html>` |
| `PluralCategory` | The shared spelling of a category, for what crosses locales |
| `Symbols` | The symbols a language writes a whole number with |
| `Count`, `Counted` | The bounds a message's count parameter takes, and what lets one call site serve both sides |
| `Embedded`, `AssetSet` | One embedded file, and a crate's worth of them |
| `asset_routes(sets)` | Mounts them, which `app()` already does |
| `RouteEntry`, `AssetSetEntry` | The entries the macros submit for `app()` to collect |
| `exos-build` | The asset pipeline `asset!` runs: `build`, `Built`, `Mode`, `content_type`, `Error` |
| `exos-cldr` | The table `locales!` reads: `Entry`, `Symbols`, `Rule`, `Test`, `Category`, `Direction`, `LOCALES`, `VERSION` |

The runtime reads a vocabulary of attributes the helpers above emit:
`data-signals` and `data-signals-root` declare, `data-messages` carries a
projected sentence, `data-text`, `data-show`, `data-class`, `data-attr`,
`data-prop` and `data-bind` with its `-kind`, `-state`, `-rules`, `-arms`,
`-check` and `-rows` companions bind, `data-on-*` dispatches, and `data-row`,
`data-rows` and `<exos-live id data-token>` are structure. They are the wire
between the two halves rather than something to write.
