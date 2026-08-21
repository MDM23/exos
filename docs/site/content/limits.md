# What exos does not do

Knowing the edges is more useful than a feature list.

- **No client-side loop.** Server-rendered lists plus morphing cover it. If you
  need a list bound to reactive client data, exos is the wrong tool.
- **No debounce.** A call fires per event, so a rule only the server can answer
  waits for a submit rather than answering while a field is being typed.
- **No client-side routing beyond fetch-and-morph.**
- **No arbitrary Rust in the browser.** Handlers record expressions, and
  anything the combinators cannot say needs `Js::raw`.
- **Expressions are compiled with `new Function`**, so a strict CSP without
  `unsafe-eval` blocks them. A precompiled mode is the answer and does not
  exist yet.
