# What exos does not do

Knowing the edges is more useful than a feature list.

- **No client-side loop.** Server-rendered lists plus morphing cover it. If you
  need a list bound to reactive client data, exos is the wrong tool.
- **A model is sent whole or not at all.** The generated caller has no way to
  send one field, so a rule only the server can answer cannot be checked while
  the rest of the form is still empty: it waits for the submit.
- **No client-side routing beyond fetch-and-morph.**
- **No arbitrary Rust in the browser.** Handlers record expressions, and
  anything the combinators cannot say needs `Js::raw`.
- **Expressions are compiled with `new Function`**, so a strict CSP without
  `unsafe-eval` blocks them. A precompiled mode is the answer and does not
  exist yet.
