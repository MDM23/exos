# Languages

Declare the ones the application is built in, once, at the crate root:

```rust
exos::locales! {
    De = "de",
    #[fallback]
    En = "en",
}
```

That generates `Locale`: the tags, the fallback, each language's writing
direction, and, per language, exactly the plural categories CLDR gives it, so
`de::Plural` has `One` and `Other` while `ar::Plural` has six. The table is
vendored as source, nothing is fetched while your crate builds, and only the
languages you declared are compiled into it.

`exos::locale()` says which one a request is in:

```rust
let locale: Locale = exos::locale();
```

In order, first hit wins:

1. A `Locale` in the request scope, put there by you.
2. `Accept-Language`, matched against your tags by RFC 4647 lookup, so `de-CH`
   finds `de`. A range is only ever shortened, so `pt` does not find a `pt-BR`
   you declared; declare `pt` too if you want to answer it.
3. The fallback, which is why this answers with a `Locale` rather than an
   `Option`. There is no such thing as a request in no language.

The rules the scope has are the rules this has: outside a request it panics,
and inside a live fragment it panics, because a fragment renders again from
whatever publishes it and its arguments are its whole input.

Step 1 is the override, and it belongs where you already resolve who is
reading:

```rust
if let Some(viewer) = data::<Sessions>().viewer(&id).await? {
    exos::scope().set(viewer.locale);
    exos::scope().set(viewer);
}
```

That is [the handler that resolves a session](sessions#why-it-stops-there) with one
more line in it. `Locale` is `Copy`, so the language goes in before the viewer
moves.

exos writes no language cookie and has nowhere to keep one. The preference
lives in the profile that owns it, a second copy disagrees with it the moment
the reader changes their language on their phone, and whether such a cookie
needs consent is a question about your jurisdiction rather than about a
framework. An anonymous language switcher is one cookie you write and read in
step 1.

## What the document carries

```rust
view! {
    <html { exos::lang(locale) }>
}
```

`lang`, and `dir` where the script runs right to left. Not decoration: the
browser hands `document.documentElement.lang` to every `Intl` call, so this is
how the two halves of a page agree on the language.

A response that reached step 2 carries `Vary: Accept-Language`, including one
whose request sent no header at all, because a request that had sent one would
have been answered differently and a cache has to be told. A response you
decided yourself carries no such claim, since it did not vary by the header,
and a page rendered for one reader is yours to set a caching policy for.

## Messages

Text is Rust, declared wherever it is used:

```rust
exos::messages! {
    clear_selection {
        De = "Auswahl aufheben",
        En = "Clear selection",
    }

    items_selected(count: Plural) {
        De { One } = "{count} Element ausgewählt",
        De { _ }   = "{count} Elemente ausgewählt",
        En { One } = "{count} item selected",
        En { _ }   = "{count} items selected",
    }
}
```

That generates a function per message, where the block stands:

```rust
view! {
    <button>{ clear_selection() }</button>
    <p>{ items_selected(picked.len()) }</p>
}
```

Each function reads `exos::locale()` and answers with a `String`, so a message
interpolated into a template is escaped like every other string. No part of one
is ever parsed as HTML, which means a translation cannot introduce an element by
being edited.

Arms read like a `match`: in order, first wins, `_` and `..` as wildcards, and a
locale written with no braces answers whatever the parameters are. One thing
does not read like a `match`, on purpose: a bare name is a value rather than a
binding, so `De { One }` is that category and never "call whatever this is
`One`".

A `Plural` parameter is a count. It arrives as any whole number, so `len()` and
a literal both go in without a cast, and the categories you may name are the
ones that language has: `de::Plural` has `One` and `Other` while `ar::Plural`
has six. It is also written the way that language writes a number, so German
says 1.234 where English says 1,234 and Hindi says 12,34,567. Every other
parameter is written into the sentence with `{name}`, wherever the translation
puts it and as often as it likes:

```rust
exos::messages! {
    assigned(to: Assignee, count: Plural) {
        De { .. }          = "{count} Dateien zugewiesen",
        En { Me, One }     = "{count} file assigned to you",
        En { Me, _ }       = "{count} files assigned to you",
        En { Somebody, _ } = "{count} files assigned to {to}",
    }
}
```

An arm names one pattern per parameter, in the order they were declared. Naming
a value rather than `_` branches on that parameter, which asks that its type can
list its values:

```rust
#[derive(Clone, Copy, exos::Enumerable)]
enum Assignee {
    Me,
    Somebody,
}
```

`bool` is such a type already, and a parameter no arm ever names is only
interpolated, so it can be anything that implements `Display`. The two are
independent: `{to}` in the arm above writes the assignee into the sentence, so
that message asks `Assignee` for a `Display` as well as for this.

### Slots: a sentence with a link in it

Do not build one out of two messages. The link lands somewhere else in the next
language, and a translator handed `"Please accept the "` and `" before
continuing"` has been handed two things that are not sentences and cannot be
checked. Declare a slot instead:

```rust
exos::messages! {
    accept_terms(terms: Slot) {
        De = "Bitte die {terms}Nutzungsbedingungen{/terms} annehmen.",
        En = "Please accept the {terms}terms of service{/terms}.",
    }
}
```

```rust
accept_terms(|inner| view! { <a href="/terms">{ inner }</a> })
```

A slot is a wrapper, `FnOnce(Markup) -> Markup`, so the href, the classes and
the routing stay in Rust while the words, including the ones inside the link,
stay in the sentence. `{b}` and `{i}` are the same mechanism with the wrapper
already written, `<strong>` and `<em>`, because emphasis falls on different
words in different languages and there is nothing there for you to decide:

```rust
exos::messages! {
    unread(count: Plural) {
        En { One } = "You have {b}{count} unread{/b} message",
        En { _ }   = "You have {b}{count} unread{/b} messages",
    }
}
```

A message with a slot answers with `Markup` rather than a `String`, so
interpolating it into a template writes the elements you wrapped its words in.
Its own words are still escaped, while your crate compiles: no part of a
message string is ever parsed as HTML, and the only structure a translation can
carry is a slot you declared. Every declared slot wraps something in every
language, so a translation that drops the link fails the build rather than
shipping a sentence nobody can click.

### What the compiler holds you to

Nothing is looked up while your application runs, so all of this is a build
failure instead:

| what you did | what you get |
| --- | --- |
| left a locale out of a message | non-exhaustive match on `Locale` |
| left a category out of a locale | non-exhaustive match on `de::Plural` |
| named a category that language has not | no variant `de::Plural::Few` |
| dropped a slot from one translation | the arm that dropped it, named |
| added a language to `locales!` | both of the first two, at every message |

The last line is the guarantee and the cost in one sentence: adding a language
breaks the build until every message is translated, and there is no fallback
that quietly renders English into a German page.

Write the macro as often as you like, so that messages live next to the feature
that says them. It puts nothing around the functions, so where you want a
prefix, put the block in a module of your own naming and call it whatever
reads best from the outside. Your languages are found at `crate::Locale` by
convention; write `exos::messages!(in path::to::Locale { ... })` where they are
somewhere else.

A count is the one number a message knows is a number. Anything else you
interpolate is written the way it displays, so format it yourself, with
`Locale::number` where a plain whole number is what you want:

```rust
let locale: Locale = exos::locale();

locale.number(1_234_567) // "1.234.567" in German
```

What is not built is the crossing: a message whose count comes from client
state should decide in the browser rather than on the server. [The
roadmap](https://github.com/MDM23/exos/blob/main/docs/roadmap/localization.md) says what that looks like.
