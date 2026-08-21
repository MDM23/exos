# Reading the examples

[`examples/playlist`](https://github.com/MDM23/exos/tree/main/examples/playlist) is a listening room several
browsers share, and exercises most of the surface in one page: a live fragment
republished by a clock, so the track changes with nobody having asked;
optimistic hearts and removals; selection with a batch action; and drag to
reorder, deciding what plays next.

What it does not have is a switch labelled "simulate a server error". The room
will not remove what it is playing, and that one rule is enough: ask it to,
watch the row go at once and come back, and an optimistic update that turns out
to be wrong has shown you what it does. Run it with `cargo run -p playlist` and
open two tabs.

[`examples/todos`](https://github.com/MDM23/exos/tree/main/examples/todos) is TodoMVC, and covers what the first one
does not: a live fragment per filter, because a topic has to determine its
content and a filtered list is not the list; filters as routes rather than as
client state; editing a row as viewer state from the double click through
escape and blur to the save; and a list that renders nothing at all when it is
empty, decided by an ordinary `if` on the server. Run it with
`cargo run -p todos`, also twice.

[`examples/auction`](https://github.com/MDM23/exos/tree/main/examples/auction) is the one about *who*. A sale room
where the price of a lot is state, published to every tab watching it, and
being outbid is an event, sent to one person on every tab they have open and
on whatever page they happen to be reading. It covers the whole identity
surface: a resolver turning a session name into audiences, a guest who has
claimed no account and is addressable as the name in their cookie anyway,
claiming one as a rotation that carries the lots you were winning across, a
role as an audience covering several people at once, `connected` choosing
between a push and an email, and the auctioneer closing a lot, which tells a
winner who asked for nothing. Run it with `cargo run -p auction`, in two
ordinary tabs and one private window, and then reload: the price is still
there and the message is not.
