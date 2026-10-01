# Revising forms

A form whose fields depend on each other, answered by rendering the form again
on the server rather than by teaching the browser the dependencies.

Status: built, all four stages. Stages 1 and 2 landed inside the revision route
rather than as API of their own, since stage 4 leaves an application nothing to
call them from.

The case is common enough to be the default rather than the exception. Pick a
tenant, and the sports offered are that tenant's. Pick a sport, and the leagues
offered are the ones playing it. Pick the country, and the postcode rule
changes. Change the first choice, and the second has to be cleared if it no
longer belongs, or kept if it still does, or chosen for the reader if only one
is left.

## What the alternatives cost

Three shapes answer this, and only one of them answers all of it.

- **Every option rendered and gated with `show`.** What
  [forms](forms.md#stage-3-one-debounce-three-features) found for widgets, and
  it is right for what it is: instant, no request, nothing new in exos. It is
  also the whole list in the document, it cannot decide anything (picking the
  one sport left for a tenant is a rule, and the browser has no rules but the
  ones a model declares), and clearing a dependent choice is a `set("")` per
  field written at every call site. Some browsers ignore `hidden` on an
  `<option>`, so the gate is not even reliable there.
- **A request per dependent field.** A route per dependency, each answering the
  options of one control. It grows with the form, and a change that affects two
  fields is two routes or one route that knows about both, which is the form
  again, split badly.
- **A live fragment per control.** The wrong tool twice over. A fragment is
  addressed by a topic and fanned out to every viewer, and it renders masked,
  so it [cannot read the request scope](async-fragments.md#what-does-not-change)
  and therefore cannot ask what this reader is allowed to see. The options of a
  form are exactly that question.

What is left is the form itself: its markup is a function of its model, the
page already calls that function once, and a change to a field that others
depend on calls it again with what the reader has picked so far.

```rs
impl exos::Form for TeamForm {
    /// The whole form, for whatever the reader has picked so far.
    async fn render(&mut self, key: FormKey) -> Markup {
        let sports = Sport::of(&self.tenant).await;
        self.sport = keep_or_preselect(&self.sport, &sports);

        let form = self.signals(key);

        view! { <form {&form} …> … </form> }
    }
}
```

The first render and every revision are one method, so the rules deciding what
is offered, what is kept and what is chosen for the reader exist once, on the
server, next to the queries that already know who is asking.

## What already works

- **A model can start at any value.** `to_signals(&self)` beside `signals()` in
  [model.rs](../../crates/exos-macro/src/model.rs), which is what
  `self.signals(key)` becomes for a form.
- **A patch morphs by id.** A form carrying an `id` is replaced in place by an
  [`Effect::patch`](../../crates/exos/src/effect.rs) of the same markup, with
  no target selector and no swap strategy.
- **What is typed survives the morph.** `declare` in
  [runtime.js](../../crates/exos/js/runtime.js) declares a signal and never
  overwrites one, so a patch re-delivering a field does not reset what the
  reader typed into it.
- **Out-of-order answers are already dropped**, for a request made under a
  debounce key: `request` stamps each call with its turn and cancels an answer
  a newer call has overtaken.
- **A per-model route exists.** `/_exos/check/{model}/{field}` in
  [valid.rs](../../crates/exos/src/valid.rs) is resolved through `inventory`
  from entries `#[model]` submits, which is the shape a revision route wants.

## Stage 1: a model that is not judged

`Model<T>` validates and refuses, which is right for a submission and wrong for
a revision. A form half filled in is the normal state of a form being revised,
and a refusal would mark every field the reader has not reached yet.

**Done**, as `read` in [model.rs](../../crates/exos/src/model.rs): the body
through the same `inward` mapping `Model<T>` uses, judged by nothing. `Model<T>`
reads through it before it validates, and the revision route reads through it
and stops there.

Not an extractor of its own. Nothing outside the route needs one yet; an
autosave would be the first, and `Draft<T>` is the name waiting for it.

## Stage 2: an effect that writes the model back

The patched markup cannot carry the new values, because `declare` never
overwrites, and that rule is the one keeping typed text alive. So the values the
server decided travel as a step of their own. `Step::Signals` exists and merges
into the store; what is missing is a way to fill it from a model rather than one
`and_set` per field.

**Done**, inside the route. **Only what the server changed is written.** The
revision serializes the model before and after `render` and answers with the
fields that differ, keyed as they travel, beside the markup.

Writing every field would put back the value of a field the reader edited while
the request was out, which is the same race
[stage 6](forms.md#the-control-still-writes-the-slot) closes for checks by
dropping an answer about a value nobody is holding any more. The client applies
the same guard here: a field whose value changed since the request went out
keeps it.

## Stage 3: the newest revision wins

A reader picking tenant A and then tenant B can receive B's form first and A's
last. The turn stamp in `request` already solves this, but only under a debounce
key, which a `select` has no reason to carry.

**Done.** A revision is keyed by its form's id instead: `revise` in the runtime
stamps every revision of one form with a turn and drops the answer a later one
has overtaken. The same idea as the stamp in `request`, in a map of its own,
because a revision goes through its own function the way a check does.

## Stage 4: a form is a model that renders itself

**Done.** Stages 1 to 3 are the round trip; this is where it lives. No route and
no `on_change` is written at a call site, the way
[stage 6](forms.md#stage-6-the-rule-only-the-server-can-answer-while-it-is-typed)
left none for checked fields, and the render the page and the route share lives
on the model it renders.

```rs
#[exos::form]
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct TeamForm {
    #[revises]
    pub tenant: String,

    #[revises]
    pub sport: String,

    pub league: String,
}
```

The view is the `impl Form` at the top of this document. The page asks for it
with `TeamForm::default().markup().await`, and the revision route asks for it
with whatever the reader has picked.

- **A form is a model and its view, 1:1.** `#[exos::form]` is `#[exos::model]`
  plus an `impl Form`, and the `inventory` entry it submits names
  `<TeamForm as Form>::render`, so a form without a view does not compile. A
  model is a shape on the wire; a form is a model that is also markup.
- **Every form, not only a revising one.** A form with no `#[revises]` field is
  one whose render is never asked for twice. One way to write a form is the
  point; [forms](forms.md#open-questions) holds what that takes out of signup.
- **One template, by construction.** A form's handle comes from
  `self.signals(key)`, and `FormKey` has no public constructor: exos makes one
  for `render` and nowhere else. A form gets no `signals()` or `to_signals()`,
  so no page can bind one outside its view.
- **The key orders the render.** A handle captures the model's values when it
  is built, so decisions go above `self.signals(key)` and markup below it. That
  is what declares a value chosen for the reader on first render, where no step
  exists to correct it. A handle built too early shows as a missing
  preselection on first load.
- **exos owns the id.** The handle's spread writes it from the type's name,
  along with the stage 3 key, so no template types either.
- **One route, one call.** `/_exos/revise/{form}` is resolved through
  `inventory` the way checks are, reads the body unjudged, calls `render`, and
  answers with the patch and the fields `render` changed.
- **`#[revises]` only on a form.** On a plain `#[model]` it is a compile error,
  since nothing could answer it. A binding on such a field posts the form, a
  `select` or a checkbox on `change` and a text field on the stage 3 debounce,
  which is what the binding already knows about its control.
- **Everything `render` needs is in the model or the session.** The route
  receives the body and nothing else, so the page's path is gone: a form editing
  team 42 carries `team: u64` as a field nothing binds. That is what keeps a
  revision stateless.

## What it costs

- **A request per change** of a field others depend on. For a `select` that is
  one request per decision the reader makes, which is cheap; for a text field
  it is the debounce, which exists.
- **The queries run again** on every revision. The render is the page's own, so
  a revision costs what rendering the form costs.
- **A second route answering questions about data.** Like the check route it is
  a same-origin `POST` carrying `X-Exos` and the session, and what it may reveal
  is what `render` decides to render. Nothing new is exposed that the page
  itself does not render, provided `render` treats `self` as untrusted: on a
  revision it is an unjudged draft, `team` included, so it authorizes what it
  reads exactly as the page would.
- **The application's guard answers it.** Both routes run application code, so
  they are merged in with the application's routes and sit under whatever it
  layers on, the way an action does. A form on a page served without a session
  therefore needs its guard to let `/_exos/revise/{form}` through as well.
- **One instance per page.** The id comes from the type, as every signal name
  already comes from its field, so two of one form on a page collide. That is
  the limit models have today, not a new one; per-row edit forms are where it
  will be felt first.
- **A render is async**, even for a form that awaits nothing, because the trait
  has one shape.

## What exos will not do

- **No dependency graph.** Which field affects which is the application's
  function, not a declaration exos evaluates. `#[revises]` says when to ask,
  never what the answer depends on.
- **No partial render.** The whole form comes back. Diffing is the morph's job,
  and a form small enough to fill in is small enough to send.
- **No client-side options cache.** A revision asks again, because what a reader
  may choose can change between two changes.
- **No second view of one form.** A model shown two ways is two forms, or a
  field on the one.
- **No submission in the trait.** A submit route carries path params and
  answers with any effect, which the typed caller already serves. Folding it in
  would make `Form` a controller.

## Testing

- **A form renders without a request**: `markup()` on a model built in the test
  shows the preselection, with no route involved.
- **A form without a view, and `#[revises]` on a plain model, do not compile.**
- **A revision refuses nothing**: an empty model that breaks a rule reaches
  `render` and is answered.
- **Typed text survives a revision** of another field, including a field the
  server did not touch and one it did but the reader edited while the request
  was out.
- **The older of two overlapping revisions is dropped**, on a slow connection
  simulated by holding the first response.
- **Focus stays where it was** across the morph, since a reader tabbing through
  a revising `select` is the common path and the morph has never been asserted
  against a focused control inside the replaced element.

## Open questions

- **What a failed revision shows.** A revision that errors has no submission to
  refuse, so an alert is too loud and silence leaves stale options on screen.
  `render` answers with `Markup` for now, so a query that fails has nowhere to
  go but the markup; a `Result` there waits for this answer.
- **Whether gated options become a helper anyway**, for the lists small and
  static enough that a request is waste. That is the first alternative above,
  and it may deserve a name even though it is not this document's answer.
