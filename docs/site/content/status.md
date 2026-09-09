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
[docs/roadmap](https://github.com/MDM23/exos/tree/main/docs/roadmap).
[Sessions and identity](https://github.com/MDM23/exos/blob/main/docs/roadmap/sessions-and-identity.md)
is the one most of the others wait on and names both the browser and, on a live
stream, who it belongs to.
[Directed effects](https://github.com/MDM23/exos/blob/main/docs/roadmap/directed-effects.md)
is built as far as pushing an effect to a person.
[Localization](https://github.com/MDM23/exos/blob/main/docs/roadmap/localization.md)
is built as far as a message whose count the browser holds, and says what a
live fragment in eight languages costs.
[More than one instance](https://github.com/MDM23/exos/blob/main/docs/roadmap/more-than-one-instance.md)
is built as far as a cluster that needs no sticky sessions and where signing
out means the same thing on every node, and says which of the ordering
guarantees survive.
[Loose ends](https://github.com/MDM23/exos/blob/main/docs/roadmap/loose-ends.md)
collects the smaller work that waits for nothing.

[What exos does not do](limits) is the other half of this page, and is about
decisions rather than gaps.
