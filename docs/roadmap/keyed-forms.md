# Keyed forms

More than one form of a type on one page, each addressed on its own.

Status: nothing built. A page holds one form of each type, and a second one
silently shares the first one's id, signals and revisions.

## The case

A report builder shows a chip per filter of a list, and each chip opens the
same small form over its own filter: how it compares, the value. The natural
markup is one form per chip, rendered with the page, which is what a popover
needs to open without a request. Today the page holds several `FilterEdit`
forms and they collide in three places:

- **The id.** The form's element id is its type's state name, so a revision's
  patch finds every one of them by the same id and morphs the last answer into
  all of them.
- **The signals.** A form's fields are document signals named by its type and
  field, so two forms of one type read and write one set of values. Typing into
  one writes the other.
- **The answers.** A revision is posted to `/_exos/revise/{form}` and a refusal
  writes its errors to the record named by the type, so neither says which of
  the forms it is about.

An application can route around it by sending the one form it needs in a dialog
when asked, which a handler with `modal_now` already does well. A form a patch
brings where the page has none starts as its own markup declares it, so the
dialog sent for a second filter no longer opens on the first one's values.
That costs a request before the form shows, and it rules out a form that is
simply there.

## The shape

A form says which of its fields tell two of it apart, and everything exos names
after the type is named after the type and those values.

```rs
#[exos::form]
pub struct FilterEdit {
    #[identifies]
    pub list: Uuid,
    #[identifies]
    pub index: usize,

    #[revises]
    pub operator: Operator,
    pub value: String,
}
```

Those fields are the form's key. They have nothing to do with `FormKey`, the
token only `Form::render` is handed so a form has one template, which is why
the attribute is not called `#[key]`.

The key is a field rather than an argument to `markup`, because every place
that has to rebuild a name already has the model and nothing else: the revision
route reads it from the body, and a refusal reads it from the body it refused.
An argument would have to travel beside the body to each of them.

A key never changes for the life of a form, since every name the page holds
for it is derived from it. So the macro refuses `#[identifies]` beside
`#[revises]`, and a key field is never bound to a control.

A form without `#[identifies]` is unchanged, so no application has to touch a
form it shows once.

## Stages

### Stage 1: keyed names

The macro derives one suffix from the key fields' values, short and safe in an
id and a signal name, hashed the way signal names already are. The form's
element id, its record signal and every field signal take it.
`signals(&self, FormKey)` builds the keyed handle from `self`, so a template
binding `&form.value` binds this instance's field with nothing new to write.

Today every one of those names is a `&'static str` the macro computes at
compile time from `"Model.field"`. A keyed name is only known at runtime, so the
handle's names, the element id and the record become owned strings.

### Stage 2: keyed revisions

The client already posts to the id of the form it revises, so the route looks
the type up by the id's unkeyed part and renders the model the body carries,
which names its own key. The patch it answers with carries the keyed id and
lands on that form alone, and the signals it writes back are that form's.

### Stage 3: keyed refusals

`ModelRejection::Refused` writes the errors to the record of the instance the
body names, so a refused submit marks the chip's form that sent it and leaves
the others quiet. This needs the key read before validation, which the body has.

`Refused { state: &'static str }` becomes an owned name. Its fields are public,
so this is a breaking change to the enum, and it lands in a release that may
break.

### Stage 4: rows in a keyed form

`Rows` names its group after the record. Keyed, a row group's names take the
suffix too. Nothing asks for it yet, so this waits for a form that does.

## What exos will not do

- **No key invented for the application.** A position on the page is not an
  identity: a row removed above renumbers everything below, and a revision in
  flight would land on the wrong form. The key is data the model owns.
- **No check that a key is genuine.** A body can name any key, and a forged
  one only aims a revision at another form's id. The handler authorises the
  record the key points at, as it does for every other field of a body.
- **No element scope for form fields.** Scoping fields to the form element
  would separate instances for free, but the server writes a revision's
  signals and a refusal's record to the document, and it cannot reach an
  element's scope. Keyed document names keep both.
- **No handler writing a form's field.** A form's handle is only built inside
  its render, keyed or not, so `Effect::set` cannot reach one today. That is a
  question for forms in general, not for this document.

## Open

Document signals are never released. A tab that stays open across many lists
keeps a slot for every chip it ever showed, each a few bytes under a hashed
name. Probably fine; worth a look if a page ever holds thousands.

## Testing

- **Two forms of one type on a page** revise independently: changing one's
  `#[revises]` field answers into that one, and typing into the other is kept.
- **A refused submit** marks the form that sent it and no other.
- **A key beside `#[revises]`** fails to compile.
- **A form without a key** renders and revises exactly as before, its id and
  signal names unchanged, so no stored page or test breaks.
