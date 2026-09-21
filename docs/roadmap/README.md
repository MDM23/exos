# The roadmap

One line per document: where it stands and what is left. The documents
themselves carry the reasoning, including for the parts that are built, because
what a stage decided is worth as much as what it shipped. Finished designs move
to [docs/spec](../spec).

| Document | Built | Left |
| --- | --- | --- |
| [async fragments](async-fragments.md) | nothing | all five stages, and it changes code that exists rather than adding beside it |
| [dimensions](dimensions.md) | nothing | all four stages; [localization](localization.md) is the first thing that wants it |
| [directed effects](directed-effects.md) | stages 1 to 3: a connection knows who it is, `send` reaches every tab an audience has open | stage 4, what a toast *is*, which is two open questions rather than a chore |
| [forms](forms.md) | stages 0 to 3, 5, 6, and the gate half of 4 | patterns (1a) and `required_when` |
| [localization](localization.md) | stages 1, 2 and the count half of 3: locales, messages, a count the browser picks its own sentence for | the rest of stage 3, stage 4 (waits on [dimensions](dimensions.md)), stage 5, dates and times |
| [loose ends](loose-ends.md) | ten of the thirteen entries | three: disabling a busy control, the publish index, and precompiled expressions |
| [more than one instance](more-than-one-instance.md) | stages 1 to 4: a cluster with no sticky sessions, where signing out means the same thing everywhere | stage 5, the broker doing the filtering, which waits for a volume nothing has reached |
| [observability](observability.md) | nothing; the crate emits no span, metric or log line | all five stages, and one field that has to be decided before the bus grows a second version |

Two documents have moved out because nothing in them is open: [sessions and
identity](../spec/sessions-and-identity.md) and [what an outside review
found](../spec/outside-review.md).
