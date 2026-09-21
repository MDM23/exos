# Status

Early. The shape is settled and the pieces work together, but this has not
carried a real application yet.

Known gaps, roughly in priority order.

**Expressions are compiled with `new Function`**, which a strict CSP without
`unsafe-eval` blocks. A precompiled mode is the answer.

**A sentence with a link in it cannot cross.** A message whose count is client
state [projects](languages#a-count-the-browser-has), and one with a slot does
not: the runtime would have to build elements rather than write text. Neither
does a message with two counts, since they could be on two sides at once.

## The roadmap

Where each of those is going is written down in
[docs/roadmap](https://github.com/MDM23/exos/tree/main/docs/roadmap), whose
index lists every design and what is left of it. The designs that are finished,
sessions and identity and what an outside review found, have moved to
[docs/spec](https://github.com/MDM23/exos/tree/main/docs/spec).

[What exos does not do](limits) is the other half of this page, and is about
decisions rather than gaps.
