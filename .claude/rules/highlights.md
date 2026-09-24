# Highlights

[docs/highlights.md](../../docs/highlights.md) collects the everyday problems
exos solves in a few lines because of its design. It is the source for the
website, so it is kept current while working rather than written up later.

## When to add an entry

Add one when, during development, something works with almost no new code
because of a decision already made. All three have to hold:

- **The problem is common.** Most web applications have it: confirmations,
  panels, live counters, validation, optimistic updates.
- **The answer is short.** A few lines at the call site, with no special case
  added to exos for it.
- **It follows from a decision.** Name the one. An entry that cannot name one is
  a feature, and features belong in the guide.

Mention the addition to the user in one line when it happens.

## Shape

- A `##` heading that states the result, not the mechanism.
- One paragraph on the problem and what exos does about it.
- At most one short code block, real code that compiles against the current
  API.
- A closing `Follows from:` line naming the decision.
- No hype words, no comparisons that name another framework.

Keep entries true. When a change breaks one, fix or remove the entry in the same
change.
