// Tests for the client runtime.
//
// Everything here is the half of the framework Rust cannot reach. The server
// side is checked by the crate's own tests, which assert what is rendered; the
// bugs that got through were all on this side of the boundary, in scoping,
// morphing and the two of them meeting.

import assert from "node:assert/strict";
import { test } from "node:test";

import { after, boot, bootDev, bootUnder, settled } from "./harness.js";

/** One row, declaring a signal and binding a class to it. */
const row = (id = "row") =>
    `<li id="${id}" class="row" data-signals='{"open":false}' ` +
    `data-class='{"open": $.open}'><span class="hit">x</span></li>`;

test("a signal belongs to the element that declared it", async () => {
    const window = boot(`<ul>${row("one")}${row("two")}</ul>`);

    window.exos.setIn(window.document.querySelector("#one .hit"), "open", true);
    await settled();

    assert.ok(window.document.getElementById("one").classList.contains("open"));
    assert.ok(!window.document.getElementById("two").classList.contains("open"));
});

test("an element leaving the page takes its signals with it", async () => {
    const window = boot(`<ul id="list">${row()}</ul>`);

    window.exos.setIn(window.document.querySelector(".hit"), "open", true);
    await settled();

    window.exos.applyPatch(`<ul id="list"></ul>`);
    await settled();

    assert.equal(Object.keys(window.exos.signals).length, 0);
});

// The bug: switching filters replaces the wrapper around a list, because a live
// fragment's id carries the filter. The new row is built before the old one is
// torn down, both carry the same id, and the departing row's cleanup deleted
// the arriving row's signals.
test("two elements with one id do not share a scope", async () => {
    const window = boot(`<div id="host"><div id="wrap-a"><ul>${row()}</ul></div></div>`);

    const host = window.document.getElementById("host");
    const next = window.document.createElement("div");
    next.id = "host";
    next.innerHTML = `<div id="wrap-b"><ul>${row()}</ul></div>`;

    window.exos.morph(host, next);

    window.exos.setIn(window.document.querySelector("#wrap-b .hit"), "open", true);
    await settled();

    assert.ok(window.document.querySelector("#wrap-b li").classList.contains("open"));
});

// The bug: the server renders class="row", knowing nothing about a class the
// browser owns, and the morph took the incoming markup's word for it. The
// signal still said true, so nothing ever put the class back.
test("a binding survives the markup that morphs over it", async () => {
    const window = boot(`<ul id="list">${row()}</ul>`);
    const before = window.document.getElementById("row");

    window.exos.setIn(window.document.querySelector(".hit"), "open", true);
    await settled();
    assert.ok(before.classList.contains("open"));

    window.exos.applyPatch(`<ul id="list">${row()}</ul>`);
    await settled();

    assert.ok(before.classList.contains("open"), "the class came back");
    assert.equal(window.document.getElementById("row"), before, "and the row is the same one");
});

// The bug: a model's fields are what a handler writes with Effect::set, which
// the client applies against the document, so a field declared into an
// element's scope was a different signal of the same name.
test("a document signal is reachable however deep it was declared", async () => {
    const window = boot(
        `<section><form data-signals-root='{"draft":""}'>` +
            `<input data-bind="draft" data-bind-kind="string"></form></section>`,
    );

    // What an effect's signals step does.
    Object.assign(window.exos.signals, { draft: "written by the server" });
    await settled();

    assert.equal(window.document.querySelector("input").value, "written by the server");
});

// The other half of the same rule, and the reason a model's fields are not
// declared the way a signal() handle is.
test("an element's signal is not reachable from the document", async () => {
    const window = boot(
        `<section data-signals='{"draft":""}'>` +
            `<input data-bind="draft" data-bind-kind="string"></section>`,
    );

    Object.assign(window.exos.signals, { draft: "written by the server" });
    await settled();

    assert.equal(window.document.querySelector("input").value, "");
});

test("a declaration never overwrites a value the page is already holding", async () => {
    const window = boot(`<ul id="list">${row()}</ul>`);

    window.exos.setIn(window.document.querySelector(".hit"), "open", true);
    await settled();

    window.exos.applyPatch(`<ul id="list">${row()}</ul>`);
    await settled();

    assert.equal(window.exos.readIn(window.document.querySelector(".hit"), "open"), true);
});

test("a navigation starts the page as its own markup declares it", async () => {
    const page = (draft) =>
        `<!DOCTYPE html><html><head><title>next</title></head><body>` +
        `<main data-signals-root='{"draft":"${draft}"}'>` +
        `<input data-bind="draft" data-bind-kind="string"></main></body></html>`;

    const window = boot(
        `<main data-signals-root='{"draft":""}'>` +
            `<input data-bind="draft" data-bind-kind="string"></main>`,
    );

    const field = window.document.querySelector("input");
    field.value = "half typed";
    field.dispatchEvent(new window.Event("input", { bubbles: true }));
    await settled();
    assert.equal(window.exos.signals.draft, "half typed");

    window.fetch = async () => ({ text: async () => page(""), url: "http://localhost/next" });
    await window.exos.navigate("http://localhost/next", true);
    await settled();

    assert.equal(window.exos.signals.draft, "", "the arriving page said it starts empty");
});

// A page in another language is served as one and has to be read as one. `lang`
// and `dir` sit on <html>, which a morph of the body cannot reach, so a
// navigation used to leave the previous language in place: a screen reader in
// the wrong voice, hyphenation by the wrong rules, and the text starting on the
// wrong side of the box.
test("a navigation brings the language of the page it lands on", async () => {
    const window = boot(`<p>hello</p>`);
    window.document.documentElement.lang = "en";

    const page = (attributes) => async () => ({
        text: async () =>
            `<!DOCTYPE html><html ${attributes}><head><title>next</title></head>` +
            `<body><p>next</p></body></html>`,
        url: "http://localhost/next",
    });

    window.fetch = page(`lang="ar" dir="rtl"`);
    await window.exos.navigate("http://localhost/next", true);
    await settled();

    assert.equal(window.document.documentElement.lang, "ar");
    assert.equal(window.document.documentElement.dir, "rtl");

    // And a page that says nothing about its language does not inherit the
    // one before it, which would be the same lie the other way round.
    window.fetch = page("");
    await window.exos.navigate("http://localhost/plain", true);
    await settled();

    assert.ok(!window.document.documentElement.hasAttribute("lang"));
    assert.ok(!window.document.documentElement.hasAttribute("dir"));
});

// Click through two links quickly and the answers come back in whatever order
// the network decides. Only the newest may land: the older one's page, and the
// URL that goes with it, are somewhere the reader has already left.
test("an older navigation does not land on the page a newer one put up", async () => {
    const window = boot(`<main id="page">first</main>`);

    const answers = [];
    window.fetch = (url) =>
        new Promise((resolve) =>
            answers.push(() =>
                resolve({
                    text: async () =>
                        `<!DOCTYPE html><html><head><title>${url}</title></head>` +
                        `<body><main id="page">${url}</main></body></html>`,
                    url,
                }),
            ),
        );

    const slow = window.exos.navigate("http://localhost/a", true);
    const quick = window.exos.navigate("http://localhost/b", true);

    // The second click answers first and the first click answers last, which
    // is the whole of what the network is allowed to do here.
    answers[1]();
    await quick;
    answers[0]();
    await slow;
    await settled();

    assert.equal(window.document.getElementById("page").textContent, "http://localhost/b");
    assert.equal(window.document.title, "http://localhost/b");
    assert.equal(window.location.href, "http://localhost/b");
});

// A textarea's value is what is written between its tags rather than an
// attribute, so reading `value` off the incoming element found nothing and the
// control kept what had been typed into it. The input beside it took the
// server's word, which is two controls disagreeing about who owns a value.
test("an unbound textarea takes the value the server sent, like the input beside it", async () => {
    const window = boot(
        `<form id="form"><textarea>first</textarea><input value="first"></form>`,
    );

    const note = window.document.querySelector("textarea");
    const title = window.document.querySelector("input");

    note.value = "half typed";
    title.value = "half typed";

    window.exos.applyPatch(
        `<form id="form"><textarea>second</textarea><input value="second"></form>`,
    );
    await settled();

    assert.equal(title.value, "second");
    assert.equal(note.value, "second");
    assert.ok(note.isConnected, "and it is the same control, not a rebuilt one");
});


test("a navigation places the caret the arriving page asks for", async () => {
    const window = boot(`<a href="/next">next</a>`);

    // Where a click leaves the focus, and what ends the browser's own
    // autofocus for the life of the document.
    window.document.querySelector("a").focus();

    window.fetch = async () => ({
        text: async () =>
            `<!DOCTYPE html><html><head><title>next</title></head>` +
            `<body><input id="field" autofocus></body></html>`,
        url: "http://localhost/next",
    });

    await window.exos.navigate("http://localhost/next", true);
    await settled();

    assert.equal(window.document.activeElement, window.document.getElementById("field"));
});

test("a control keeps what the viewer typed when a patch lands on it", async () => {
    const window = boot(
        `<form id="form"><input id="field" data-bind="draft" data-bind-kind="string"></form>`,
    );

    const field = window.document.getElementById("field");
    field.value = "half typed";
    field.dispatchEvent(new window.Event("input", { bubbles: true }));
    await settled();

    window.exos.applyPatch(
        `<form id="form"><input id="field" value="" data-bind="draft" data-bind-kind="string"></form>`,
    );
    await settled();

    assert.equal(window.document.getElementById("field").value, "half typed");
});

/**
 * A control carrying rules and the record they are written into.
 *
 * The record is declared the way a model handle declares it, so that reading a
 * field out of it never lands on an undefined.
 */
const validated = (rules = `$.draft.length === 0 ? "needed" : ""`) =>
    `<input id="field" data-signals-root='{"errors":{}}' data-bind="draft" ` +
    `data-bind-kind="string" data-bind-state="errors" data-bind-rules='${rules}'>`;

/** Types into the control and lets the effects settle. */
async function type(window, value) {
    const field = window.document.getElementById("field");
    field.value = value;
    field.dispatchEvent(new window.Event("input", { bubbles: true }));
    await settled();
}

// A form that is red before it is read is worse than no validation, so nothing
// a field says about itself is written until it has been edited.
test("a field says nothing about itself until it is edited", async () => {
    const window = boot(validated());
    await settled();

    assert.deepEqual({ ...window.exos.signals.errors }, {});
    assert.equal(window.document.getElementById("field").hasAttribute("aria-invalid"), false);

    await type(window, "");

    assert.equal(window.exos.signals.errors.draft, "needed");
});

// The mark is what a stylesheet and a screen reader both read, and it has to
// say `true`: an empty aria-invalid means false, so a merely present attribute
// says the opposite of what it is there to say.
test("an invalid control marks itself in the attribute the language has", async () => {
    const window = boot(validated());

    await type(window, "");
    assert.equal(window.document.getElementById("field").getAttribute("aria-invalid"), "true");

    await type(window, "filled in");
    assert.equal(window.document.getElementById("field").hasAttribute("aria-invalid"), false);
});

// One slot per field, whichever side decided what is in it. Editing recomputes
// it, which is what takes a stale verdict away: the client cannot answer a rule
// it does not own, and a message about a value that is no longer there is
// worse than none.
test("editing a control replaces what the server said about it", async () => {
    const window = boot(validated());

    window.exos.signals.errors = { draft: "taken", other: "kept" };
    await settled();

    assert.equal(window.document.getElementById("field").getAttribute("aria-invalid"), "true");

    await type(window, "something else");

    assert.equal(window.exos.signals.errors.draft, undefined, "the stale verdict is gone");
    assert.equal(window.exos.signals.errors.other, "kept", "and nobody else's is touched");
});

// The same rule for a field the server alone can judge. Nothing recomputes over
// it, so without this its message outlives every value it was ever about, and a
// form gated on the record could never be submitted again.
test("editing a field with no rules of its own still retires what was said", async () => {
    const window = boot(
        `<input id="field" data-signals-root='{"errors":{}}' data-bind="code" ` +
            `data-bind-kind="string" data-bind-state="errors">`,
    );

    window.exos.signals.errors = { code: "not one of ours", other: "kept" };
    await settled();

    assert.equal(window.document.getElementById("field").getAttribute("aria-invalid"), "true");

    await type(window, "EARLYBIRD");

    assert.equal(window.exos.signals.errors.code, undefined);
    assert.equal(window.exos.signals.errors.other, "kept");
});

// A gated rule applies only while the field that arms it is filled in, so
// editing that field changes which rules there are. Without this, unticking the
// box that reveals a section leaves the complaints about it in the record,
// where nothing on screen can reach them.
test("editing a gate retires what was said about the fields it arms", async () => {
    const window = boot(
        `<input id="field" type="checkbox" data-signals-root='{"errors":{}}' ` +
            `data-bind="invoice" data-bind-kind="bool" data-bind-state="errors" ` +
            `data-bind-arms="company vat">`,
    );

    window.exos.signals.errors = { company: "needed", vat: "needed", name: "kept" };
    await settled();

    const box = window.document.getElementById("field");
    box.checked = false;
    box.dispatchEvent(new window.Event("change", { bubbles: true }));
    await settled();

    assert.deepEqual({ ...window.exos.signals.errors }, { name: "kept" });
});

/**
 * A field with a rule the server alone can answer.
 *
 * `data-bind-check` is the model that answers for it, and `data-bind` is the
 * field, so the two names the binding already carries are the whole address.
 */
const checkable = (rules = "") =>
    `<input id="field" data-signals-root='{"errors":{}}' data-bind="code" ` +
    `data-bind-kind="string" data-bind-state="errors" data-bind-check="m1"` +
    (rules ? ` data-bind-rules='${rules}'` : "") +
    ">";

/** Longer than the fixed delay a checked field waits out. */
const CHECKED = 340;

// The wish stage 3 could not deliver: a rule that needs the server, answered
// while the field is still being edited rather than at submit.
test("a field the server alone can judge asks it once the typing stops", async () => {
    const window = boot(checkable());
    window.transport.responses.body = "not one of ours";

    await type(window, "nope");
    assert.equal(window.transport.requests.length, 0, "and not once per keystroke");

    await after(CHECKED);
    await settled();

    assert.equal(window.transport.requests.length, 1);
    assert.deepEqual(
        window.transport.requests.map(({ url, body }) => ({ url, body })),
        [{ url: "/_exos/check/m1/code", body: "nope" }],
    );
    assert.equal(window.exos.signals.errors.code, "not one of ours");
});

// The guard the extractor applies too: nothing asks the application whether an
// empty string is taken, and a value the control has already refused is not
// worth a request either.
test("nothing is asked about a value the control already complains about", async () => {
    const window = boot(checkable(`$.code.length < 3 ? "too short" : ""`));

    await type(window, "no");
    await after(CHECKED);

    assert.equal(window.transport.requests.length, 0);
    assert.equal(window.exos.signals.errors.code, "too short");
});

// Two checks can overlap on a slow connection and answer in the other order.
// What decides it is the value rather than the order: the edit that overtook
// this one has already retired whatever was said about what it was about.
test("an answer about a value nobody is holding any more is dropped", async () => {
    const window = boot(checkable());
    const answers = [];
    window.fetch = () => new Promise((resolve) => answers.push(resolve));

    await type(window, "nope");
    await after(CHECKED);

    const field = window.document.getElementById("field");
    assert.equal(field.getAttribute("aria-busy"), "true", "a check in flight says so");

    await type(window, "nopealike");

    answers.shift()({
        ok: true,
        status: 200,
        headers: { get: () => null },
        text: () => Promise.resolve("not one of ours"),
    });
    await settled();

    assert.equal(window.exos.signals.errors.code, undefined);
    assert.ok(!field.hasAttribute("aria-busy"), "and the marker goes with the answer");
});

// Whether anything in a form has been edited, which is one flag beside the
// per-field ones rather than a fold over however many fields it has.
test("a model knows whether any of its controls has been edited", async () => {
    const window = boot(`${validated()}<p id="say" data-text="dirty('errors') ? 'yes' : 'no'"></p>`);
    await settled();

    assert.equal(window.document.getElementById("say").textContent, "no");

    await type(window, "typed");

    assert.equal(window.document.getElementById("say").textContent, "yes");
});

// A control bound to a plain signal has no rules and no record, and must not go
// looking for either.
test("a binding with no rules survives being edited", async () => {
    const window = boot(`<input id="field" data-bind="draft" data-bind-kind="string">`);

    await type(window, "typed");

    assert.equal(window.exos.signals.draft, "typed");
    assert.equal(window.document.getElementById("field").hasAttribute("aria-invalid"), false);
});

// A patch re-delivering a control must not wipe the message that arrived with
// it, which is the whole reason the rules wait for an edit.
test("a message survives a patch over the control it is about", async () => {
    const window = boot(`<div id="host">${validated()}</div>`);

    window.exos.signals.errors = { draft: "taken" };
    await settled();

    window.exos.applyPatch(`<div id="host">${validated()}</div>`);
    await settled();

    assert.equal(window.exos.signals.errors.draft, "taken");
});

// Every field of a form somebody just submitted has been edited, so the guard
// above is not what protects a refusal: the rules run again over the record
// they are written into, and a field a handler refused is one they find nothing
// wrong with. What they clear is what they said themselves.
test("a refusal outlives the rules of the field it is about", async () => {
    const window = boot(validated());

    await type(window, "filled in");

    window.exos.signals.errors = { draft: "taken" };
    await settled();

    assert.equal(window.exos.signals.errors.draft, "taken");
    assert.equal(window.document.getElementById("field").getAttribute("aria-invalid"), "true");
});

// And the same refusal on a form the answer re-rendered, which is a fresh
// element running its rules for the first time over a record that already has
// something in it.
test("a refusal survives a patch over the edited control it is about", async () => {
    const window = boot(`<div id="host">${validated()}</div>`);

    await type(window, "filled in");

    window.exos.signals.errors = { draft: "taken" };
    await settled();

    window.exos.applyPatch(`<div id="host">${validated()}</div>`);
    await settled();

    assert.equal(window.exos.signals.errors.draft, "taken");
});

// What a field's rules do own, they still retire, or a form gated on the record
// could never be submitted out of a verdict its own value has moved past.
test("a field still withdraws what its own rules said", async () => {
    const window = boot(validated());

    await type(window, "");
    assert.equal(window.exos.signals.errors.draft, "needed");

    window.exos.signals.draft = "filled in";
    await settled();

    assert.equal(window.exos.signals.errors.draft, undefined);
});

// A refusal about the submission rather than about one field of it sits under
// the empty key, which no field name and no row key can be. Nothing recomputes
// it, so any edit into the model is what retires it: it was about the values
// that were sent.
test("a refusal about the model itself is retired by editing anything", async () => {
    const window = boot(validated());

    window.exos.signals.errors = { "": "Wrong email or password.", other: "kept" };
    await settled();

    assert.equal(window.document.getElementById("field").hasAttribute("aria-invalid"), false);

    await type(window, "typed");

    assert.deepEqual({ ...window.exos.signals.errors }, { other: "kept" });
});

test("a handler on markup that arrived later still fires", () => {
    const window = boot(`<div id="host"></div>`);

    window.exos.applyPatch(`<div id="host"><button id="go" data-on-click="$.hit = true"></button></div>`);
    window.document
        .getElementById("go")
        .dispatchEvent(new window.MouseEvent("click", { bubbles: true }));

    assert.equal(window.exos.signals.hit, true);
});

test("a keyed reorder moves rows rather than rebuilding them", () => {
    const window = boot(`<ul id="list">${row("one")}${row("two")}</ul>`);
    const one = window.document.getElementById("one");

    window.exos.applyPatch(`<ul id="list">${row("two")}${row("one")}</ul>`);

    assert.equal(window.document.getElementById("one"), one);
    assert.equal(window.document.getElementById("list").firstElementChild.id, "two");
});

// The morph is hand-rolled and staying that way, so the invariants it holds up
// are written down here rather than borrowed from somebody else's test suite.
// Everything below is a property the runtime promises: identity survives,
// bindings outrank the markup that lands on them, and the server's word is
// final for anything no binding owns.

test("an element that opted out is left alone", () => {
    const window = boot(
        `<div id="host"><div id="keep" data-preserve><span>original</span></div></div>`,
    );

    const keep = window.document.getElementById("keep");

    window.exos.applyPatch(
        `<div id="host"><div id="keep" data-preserve><span>replaced</span></div></div>`,
    );

    assert.equal(keep.textContent, "original", "a media player or an open <details> is not touched");
});

test("an attribute the server dropped is removed", () => {
    const window = boot(`<div id="host"><p id="note" class="a" title="old">x</p></div>`);

    window.exos.applyPatch(`<div id="host"><p id="note" class="b">x</p></div>`);

    const note = window.document.getElementById("note");

    assert.equal(note.getAttribute("class"), "b", "a changed attribute follows the server");
    assert.ok(!note.hasAttribute("title"), "and one the server stopped sending is gone");
});

// The other half of the rule the bound control above proves. Preservation is
// earned by a binding owning the value, not by the element being a field, or a
// patch could never correct one the server has the last word on.
test("a control with no binding takes the value the server sent", async () => {
    const window = boot(`<form id="form"><input id="field" value="server"></form>`);

    const field = window.document.getElementById("field");
    field.value = "typed by hand";

    window.exos.applyPatch(`<form id="form"><input id="field" value="server"></form>`);
    await settled();

    assert.equal(field.value, "server");
});

// Properties drift from their attributes the moment somebody clicks, and the
// attribute sync alone would never notice: both sides still read `checked`.
test("a checkbox follows the markup rather than the property it drifted to", async () => {
    const window = boot(`<form id="form"><input id="box" type="checkbox" checked></form>`);

    const box = window.document.getElementById("box");
    box.checked = false;

    window.exos.applyPatch(`<form id="form"><input id="box" type="checkbox" checked></form>`);
    await settled();

    assert.equal(box.checked, true);
});

// Deliberately not awaited. The observer would rebind this a microtask later
// anyway, so settling first would pass whether or not the morph did its own
// binding. Binding is synchronous because a caller that patches and then reads
// has no microtask to wait for, and that is the promise being pinned here.
test("an element whose tag changed is replaced and its bindings rebuilt", () => {
    const declare = `data-signals='{"on":true}' data-class='{"lit": $.on}'`;
    const window = boot(`<div id="host"><span id="slot" ${declare}>x</span></div>`);

    window.exos.applyPatch(`<div id="host"><button id="slot" ${declare}>x</button></div>`);

    const slot = window.document.getElementById("slot");

    assert.equal(slot.tagName, "BUTTON");
    assert.ok(slot.classList.contains("lit"), "bound by the morph, not by the observer later");
});

// The bug: a sign-in form and a registration form are the same shape, so
// navigating between them reused the inputs. Their effects still read the
// fields they were bound to, and the browser's autofilled credentials came
// back up in the name fields of the form that arrived.
test("a control the server rebound is not the control that was there", async () => {
    const field = (name) => `<input data-bind="${name}" data-bind-kind="string">`;
    const window = boot(`<form id="form" data-signals-root='{"email":""}'>${field("email")}</form>`);

    const before = window.document.querySelector("input");
    before.value = "filled in by the browser";
    before.dispatchEvent(new window.Event("input", { bubbles: true }));
    await settled();

    window.exos.applyPatch(
        `<form id="form" data-signals-root='{"firstname":""}'>${field("firstname")}</form>`,
    );
    await settled();

    const after = window.document.querySelector("input");

    assert.notEqual(after, before, "the node bound to the old field is gone");
    assert.equal(after.value, "", "and nothing of it came through");
});

// Replacing the node instead would drop a selection or an IME composition
// sitting in it, which is the same class of loss as rebuilding an element.
test("changed text updates the node rather than replacing it", () => {
    const window = boot(`<p id="note">before</p>`);

    const note = window.document.getElementById("note");
    const text = note.firstChild;

    window.exos.applyPatch(`<p id="note">after</p>`);

    assert.equal(note.firstChild, text, "the same text node");
    assert.equal(note.textContent, "after");
});

test("an unkeyed element in the same slot is updated rather than rebuilt", () => {
    const window = boot(`<div id="host"><p class="a">one</p></div>`);

    const paragraph = window.document.querySelector("#host p");

    window.exos.applyPatch(`<div id="host"><p class="b">two</p></div>`);

    assert.equal(window.document.querySelector("#host p"), paragraph, "the same element");
    assert.equal(paragraph.className, "b");
    assert.equal(paragraph.textContent, "two");
});

// One fragment can legitimately be on the page twice, and those copies share an
// id because they are the same fragment. Updating only the first leaves the
// rest stale, which is why the patch looks them all up.
test("a patch updates every copy of a fragment that appears twice", () => {
    const window = boot(`<div><p id="twin">before</p><p id="twin">before</p></div>`);

    window.exos.applyPatch(`<p id="twin">after</p>`);

    const copies = [...window.document.querySelectorAll('[id="twin"]')];

    assert.equal(copies.length, 2);
    assert.ok(copies.every((copy) => copy.textContent === "after"));
});

// A move arrives as a removal followed by an insertion, so cleaning up on every
// removal would dispose effects that never stopped being valid.
test("a row that moved keeps the bindings it had", async () => {
    const window = boot(`<ul id="list">${row("one")}${row("two")}</ul>`);

    window.exos.setIn(window.document.querySelector("#one .hit"), "open", true);
    await settled();

    window.exos.applyPatch(`<ul id="list">${row("two")}${row("one")}</ul>`);
    await settled();

    const one = window.document.getElementById("one");
    assert.ok(one.classList.contains("open"), "a move is not a removal");

    window.exos.setIn(one.querySelector(".hit"), "open", false);
    await settled();

    assert.ok(!one.classList.contains("open"), "and the binding still answers afterwards");
});

test("a row dropped from a list takes only its own signals with it", async () => {
    const window = boot(`<ul id="list">${row("one")}${row("two")}</ul>`);

    window.exos.setIn(window.document.querySelector("#one .hit"), "open", true);
    window.exos.setIn(window.document.querySelector("#two .hit"), "open", true);
    await settled();

    assert.equal(Object.keys(window.exos.signals).length, 2, "one scope per row");

    window.exos.applyPatch(`<ul id="list">${row("two")}</ul>`);
    await settled();

    assert.equal(window.document.getElementById("one"), null, "the dropped row is gone");
    assert.equal(Object.keys(window.exos.signals).length, 1, "and so is its scope, but no other");
    assert.ok(
        window.document.getElementById("two").classList.contains("open"),
        "the survivor kept what it was holding",
    );
});

// A refusal is a thing the server has something to say about, and an Effect is
// how it says it. Reading the body only on 2xx meant a handler could either be
// honest about the status or be heard, never both, so every rejected action
// came back as a console line and a page that did not change.

/** Clicks a button that posts, and waits for whatever came back to land. */
async function posted(window) {
    const button = window.document.createElement("button");
    button.setAttribute("data-on-click", "post('/drafts')");
    window.document.body.append(button);

    button.dispatchEvent(new window.MouseEvent("click", { bubbles: true }));

    // One for the reply to be read off the wire, one for the effects it
    // scheduled to run.
    await settled();
    await settled();
}

test("an effect is applied whatever the status the server sent it with", async () => {
    const window = boot(`<p id="slot">before</p>`);

    window.transport.responses.status = 422;
    window.transport.responses.type = "text/event-stream";
    window.transport.responses.body =
        `event: signals\ndata: {"draft":"too short"}\n\n` +
        `event: patch\ndata: <p id="slot">after</p>\n\n`;

    await posted(window);

    assert.equal(window.exos.signals.draft, "too short", "the validation message landed");
    assert.equal(window.document.getElementById("slot").textContent, "after");
});

test("a failure with nothing to apply is announced rather than swallowed", async () => {
    const window = boot(`<p>x</p>`);
    const seen = [];

    window.document.addEventListener("exos:error", (ev) => seen.push(ev.detail.status));

    window.transport.responses.status = 500;
    window.transport.responses.type = "text/plain";
    window.transport.responses.body = "it broke";

    await posted(window);

    assert.deepEqual(seen, [500]);
});

// The other half of the rule. HTML on a failure is a document about the
// failure, so morphing it in would let a 500 eat the page.
test("html arriving with a failure is left where it is", async () => {
    const window = boot(`<p id="slot">before</p>`);

    window.transport.responses.status = 500;
    window.transport.responses.type = "text/html";
    window.transport.responses.body = `<p id="slot">the server is on fire</p>`;

    await posted(window);

    assert.equal(window.document.getElementById("slot").textContent, "before");
});

// aria-busy is written by the request path rather than by a binding, so it was
// in nothing `reapply` puts back and the attribute sync took it off an element
// whose own request was still in flight: the spinner went, the button looked
// ready again, and the `finally` afterwards removed an attribute that was
// already gone. It happens most readily where it is worst, on a page whose live
// fragments publish while somebody is clicking.

/** A button that posts, and the same markup a patch lands with. */
const acting = `<div id="host"><button id="go" data-on-click="post('/drafts')">x</button></div>`;

test("a patch landing mid-request leaves the busy marker on", async () => {
    const window = boot(acting);
    const go = window.document.getElementById("go");

    go.dispatchEvent(new window.MouseEvent("click", { bubbles: true }));
    assert.equal(go.getAttribute("aria-busy"), "true", "marked before anything awaited");

    window.exos.applyPatch(acting);
    assert.equal(go.getAttribute("aria-busy"), "true", "and the patch did not take it");

    await settled();
    await settled();

    assert.ok(!go.hasAttribute("aria-busy"), "taken off once the reply landed");
});

// The same ownership one step on: a marker belongs to the requests that are
// waiting, not to whichever of them answers first. A debounced field and the
// form around it are both in flight on one element often enough.
test("the first of two requests to answer does not unmark the element", async () => {
    const window = boot(acting);
    const go = window.document.getElementById("go");
    const answers = [];

    window.fetch = () => new Promise((resolve) => answers.push(resolve));

    const click = () => go.dispatchEvent(new window.MouseEvent("click", { bubbles: true }));
    const answer = () => answers.shift()({ ok: true, status: 204, headers: { get: () => null } });

    click();
    click();

    answer();
    await settled();

    assert.equal(go.getAttribute("aria-busy"), "true", "the other one is still waiting");

    answer();
    await settled();

    assert.ok(!go.hasAttribute("aria-busy"), "and gone once nothing is");
});

// A repeating group is a <template> and however many rows are beside it. The
// whole point is that a row needs no name: a clone is its own signal scope, so
// adding one is a DOM copy and the submission reads them back out at the end.

/** One row of the group below, template and rendered row alike. */
const rowOf = (state) =>
    `<div data-signals='{"name":""}' data-row>` +
    `<input data-bind="name" data-bind-state="${state}" data-bind-rows="g">` +
    `<button type="button" data-on-click="dropRow(el, 'g', '${state}')">x</button>` +
    `</div>`;

/** A group as the server renders it: the template, then the rows it opened with. */
const group = (rows = 1, state = "errors") =>
    `<form data-signals-root='{"${state}":{}}' ` +
    `data-on-submit="post('/save', {'g': rows(el, 'g', ['name'])})">` +
    `<div data-rows="g">` +
    `<template>${rowOf(state)}</template>` +
    Array.from({ length: rows })
        .map(() => rowOf(state))
        .join("") +
    `</div>` +
    `<button id="add" type="button" data-on-click="addRow(el, 'g', '${state}')">add</button>` +
    `</form>`;

/** Types `text` into the nth row's field, the way a person would. */
function fill(window, nth, text) {
    const field = window.document.querySelectorAll("[data-row] input")[nth];
    field.value = text;
    field.dispatchEvent(new window.Event("input", { bubbles: true }));
}

const click = (window, selector) =>
    window.document
        .querySelector(selector)
        ?.dispatchEvent(new window.MouseEvent("click", { bubbles: true }));

test("a row added in the browser holds its own value", async () => {
    const window = boot(group(1));

    click(window, "#add");
    await settled();

    assert.equal(window.document.querySelectorAll("[data-row]").length, 2);

    fill(window, 0, "Ada");
    fill(window, 1, "Grace");
    await settled();

    // The two rows declare one name and hold two values, which is what the
    // scope-per-element rule has always given a row's own signal.
    const values = [...window.document.querySelectorAll("[data-row] input")].map((el) => el.value);
    assert.deepEqual(values, ["Ada", "Grace"]);
});

test("the submission collects the rows in the order they are shown", async () => {
    const window = boot(group(1));

    click(window, "#add");
    await settled();

    fill(window, 0, "Ada");
    fill(window, 1, "Grace");
    await settled();

    window.document
        .querySelector("form")
        .dispatchEvent(new window.Event("submit", { bubbles: true, cancelable: true }));
    await settled();

    assert.deepEqual(window.transport.requests[0].body, {
        g: [{ name: "Ada" }, { name: "Grace" }],
    });
});

test("a row removed in the browser is gone from the submission", async () => {
    const window = boot(group(1));

    click(window, "#add");
    await settled();

    fill(window, 0, "Ada");
    fill(window, 1, "Grace");
    await settled();

    // The first row's own button, so this also checks that `dropRow` walks up
    // to the row it was clicked inside rather than to some other one.
    click(window, "[data-row] button");
    await settled();

    window.document
        .querySelector("form")
        .dispatchEvent(new window.Event("submit", { bubbles: true, cancelable: true }));
    await settled();

    assert.deepEqual(window.transport.requests[0].body, { g: [{ name: "Grace" }] });
});

// A message names a row by where it sits, so the group changing shape decides
// which of them are still about the row they were written for.
test("adding a row retires what was said about how many there are", async () => {
    const window = boot(group(1));

    window.exos.signals.errors = { g: "add at least one", "g.0.name": "needed" };
    await settled();

    click(window, "#add");
    await settled();

    assert.equal(window.exos.signals.errors.g, undefined, "the count it was about has changed");
    assert.equal(window.exos.signals.errors["g.0.name"], "needed", "and nobody moved");
});

test("removing a row retires the messages that would have moved", async () => {
    const window = boot(group(2));

    window.exos.signals.errors = { "g.0.name": "first", "g.1.name": "second" };
    await settled();

    window.document
        .querySelectorAll("[data-row] button")[0]
        .dispatchEvent(new window.MouseEvent("click", { bubbles: true }));
    await settled();

    // Renumbering would be guessing. The next submission says what is wrong
    // with the rows as they then are.
    assert.deepEqual({ ...window.exos.signals.errors }, {});
});

test("the template is not a row and never rides along", async () => {
    const window = boot(group(0));

    window.document
        .querySelector("form")
        .dispatchEvent(new window.Event("submit", { bubbles: true, cancelable: true }));
    await settled();

    assert.deepEqual(window.transport.requests[0].body, { g: [] });
});

// A handler on `input` runs per keystroke, so anything that leaves the machine
// has to be held back. The key is generated per call site on the server and
// resolved against the DOM here, which is the half only a document can check.

/** A field whose input debounces a call, under the key one call site produces. */
const box = (id, key = "k1", delay = 10) =>
    `<input id="${id}" data-on-input="debounce('${key}', ${delay}, () => post('/search'))">`;

/** Types into a field, however many times, without waiting between. */
function types(window, id, times = 1) {
    const field = window.document.getElementById(id);
    for (let i = 0; i < times; i += 1) {
        field.dispatchEvent(new window.Event("input", { bubbles: true }));
    }
}

test("typing sends one request rather than one per keystroke", async () => {
    const window = boot(box("query"));

    types(window, "query", 4);
    assert.equal(window.transport.requests.length, 0, "and nothing before the wait is over");

    await after(30);
    assert.equal(window.transport.requests.length, 1);
});

// The bug this is here to prevent: a key that is only the call site collapses
// every row onto one timer, because a helper called once per row is one call
// site. Typing in the second row would then cancel the first row's save.
test("two rows debounce apart, because each row is its own scope", async () => {
    const window = boot(
        `<li id="one" data-signals='{"draft":""}'>${box("a")}</li>` +
            `<li id="two" data-signals='{"draft":""}'>${box("b")}</li>`,
    );

    types(window, "a");
    types(window, "b");

    await after(30);
    assert.equal(window.transport.requests.length, 2);
});

test("one row typed into twice is still one request", async () => {
    const window = boot(`<li id="one" data-signals='{"draft":""}'>${box("a")}</li>`);

    types(window, "a", 3);

    await after(30);
    assert.equal(window.transport.requests.length, 1);
});

// Debouncing alone is not enough, and this is the part that gets forgotten.
// Two requests can be in flight together on a slow connection, and the older
// one answering last paints the results for a prefix of what is in the box.
test("a reply older than the newest request under its key is dropped", async () => {
    const window = boot(`${box("query")}<p id="slot">before</p>`);

    const answers = [];
    const said = (text) =>
        `event: patch\ndata: <p id="slot">${text}</p>\n\n`;

    window.fetch = () =>
        new Promise((resolve) => {
            answers.push((text) =>
                resolve({
                    ok: true,
                    status: 200,
                    headers: { get: () => "text/event-stream" },
                    body: {
                        getReader: () => oneChunk(window, said(text)),
                        cancel: () => Promise.resolve(),
                    },
                }),
            );
        });

    types(window, "query");
    await after(30);

    types(window, "query");
    await after(30);

    assert.equal(answers.length, 2, "both went out, so they can answer out of order");

    // The older one answers last, which is the whole failure mode.
    answers[1]("second");
    await settled();
    answers[0]("first");
    await settled();
    await settled();

    assert.equal(window.document.getElementById("slot").textContent, "second");
});

/** A body handed over whole, for a test whose subject is not the chunking. */
function oneChunk(window, text) {
    let sent = false;

    return {
        read() {
            if (sent) return Promise.resolve({ done: true });
            sent = true;
            return Promise.resolve({ done: false, value: new window.TextEncoder().encode(text) });
        },
    };
}

/** One live fragment, as the server renders it: a name and the proof of it. */
const live = (topic = "presence-1") =>
    `<exos-live data-topic="${topic}" data-token="token-for-${topic}"></exos-live>`;

/** The element a topic was rendered on, of which a page may hold several. */
const at = (window, topic = "presence-1") =>
    window.document.querySelector(`[data-topic="${topic}"]`);

/** A whole document, the way a fetch of a URL answers with one. */
const served = (body) =>
    `<!DOCTYPE html><html><head><title>exos</title></head><body>${body}</body></html>`;

/** The connections a tab claimed, in order, ignoring whatever else it fetched. */
const claimed = (window) =>
    window.transport.requests
        .filter((request) => request.url === "/_exos/subscribe")
        .map((request) => request.body.connection);

test("a page with no live fragments opens no stream", () => {
    const window = boot(`<p>nothing live here</p>`);

    assert.equal(window.transport.streams.length, 0);
});

test("a tab subscribes with the id the server gave it", async () => {
    const window = boot(live());
    const [stream] = window.transport.streams;

    assert.equal(stream.url, "/_exos/live", "the client does not name the connection");
    assert.equal(window.transport.requests.length, 0, "and cannot subscribe before it is named");

    stream.emit("connection", "5f4dcc3b5aa765d61d8327deb882cf99");
    await settled();

    const [request] = window.transport.requests;

    assert.equal(request.url, "/_exos/subscribe");
    assert.equal(request.body.connection, "5f4dcc3b5aa765d61d8327deb882cf99");
    assert.deepEqual(request.body.topics, [["presence-1", "token-for-presence-1"]]);
});

// A publish renders outside every request, so it has no viewer to bind a token
// to and its patch says nothing about one. The grant was made when the page was
// served and a patch has never been able to make one, so the element keeps what
// it has: without this a fragment unsubscribes itself the first time it
// updates, and the second publish reaches nobody.
test("a patch that does not restate the grant leaves the subscription alone", async () => {
    const window = boot(live());
    const [stream] = window.transport.streams;

    stream.emit("connection", "named");
    await settled();

    stream.emit("patch", `<exos-live data-topic="presence-1"><span>online</span></exos-live>`);
    await settled();

    const element = at(window);

    assert.equal(element.dataset.token, "token-for-presence-1");
    assert.equal(element.textContent, "online", "and the content is the patch's");
    assert.equal(window.transport.requests.length, 1, "so it has nothing new to say");
});

// The other half, and the reason this is not simply an attribute the client
// owns. A rotated session invalidates every outstanding token, the runtime
// fetches the page back, and the grants it comes back with are the ones that
// verify: markup that carries a token has to win.
test("markup that carries a grant replaces the one on the element", async () => {
    const window = boot(live());
    const [stream] = window.transport.streams;

    stream.emit("connection", "named");
    await settled();

    stream.emit("patch", `<exos-live data-topic="presence-1" data-token="rotated"></exos-live>`);
    await settled();

    // Twice: the attribute change is what schedules the sync, and the request
    // it sends is a turn behind it.
    await settled();

    assert.equal(at(window).dataset.token, "rotated");
    assert.deepEqual(window.transport.requests.at(-1).body.topics, [["presence-1", "rotated"]]);
});

test("a reconnect subscribes again under the new id", async () => {
    const window = boot(live());
    const [stream] = window.transport.streams;

    stream.emit("connection", "first");
    await settled();

    // EventSource reconnected on its own, and the server named what is a new
    // connection with the same tab behind it. The topics have to be re-sent,
    // because the server forgot them along with the old connection.
    stream.emit("connection", "second");
    await settled();

    assert.deepEqual(claimed(window), ["first", "second"]);
});

test("a connection the server has forgotten is dropped and reopened", async () => {
    const window = boot(live());
    const [stale] = window.transport.streams;

    window.transport.responses.status = 410;
    stale.emit("connection", "forgotten");
    await settled();

    assert.ok(stale.closed, "the stream that cannot subscribe is closed");
    assert.equal(window.transport.streams.length, 2, "and a fresh one takes its place");

    window.transport.responses.status = 204;
    window.transport.streams[1].emit("connection", "fresh");
    await settled();

    assert.equal(claimed(window).at(-1), "fresh");
});

// A claim the server refused is not one it is holding. The comparison that
// keeps this quiet is against what the tab believes it has already said, so a
// tab that remembered a rejected claim as sent would wait for patches nobody
// is sending it, and go on waiting for as long as the page holds still.
test("a subscription the server refused is claimed again on the next change", async () => {
    const window = boot(live());
    const [stream] = window.transport.streams;

    window.transport.responses.status = 503;
    stream.emit("connection", "named");
    await settled();

    assert.equal(claimed(window).length, 1, "it tried once");

    window.transport.responses.status = 204;
    window.document.body.appendChild(window.document.createElement("p"));
    await settled();

    assert.equal(claimed(window).length, 2, "and again, though the set is the same");
    assert.deepEqual(watching(window), [["presence-1", "token-for-presence-1"]]);
});

/** What the tab last told the server it is watching. */
const watching = (window) =>
    window.transport.requests.filter((request) => request.url === "/_exos/subscribe").at(-1)
        ?.body.topics;

// The topic model from the end the client owns, and the reason a publish is
// cheap: a tab is sent a fragment's patch because it asked for that fragment by
// name, so what it asks for is the whole of what decides which patches reach
// it. The server's half of this is checked in the crate's own tests; that the
// set tracks the page was checked nowhere.
test("a fragment that leaves the page is one the tab stops watching", async () => {
    const window = boot(`${live("presence-1")}${live("presence-2")}`);
    const [stream] = window.transport.streams;

    stream.emit("connection", "named");
    await settled();

    assert.deepEqual(watching(window), [
        ["presence-1", "token-for-presence-1"],
        ["presence-2", "token-for-presence-2"],
    ]);

    at(window, "presence-2").remove();
    await settled();

    assert.deepEqual(watching(window), [["presence-1", "token-for-presence-1"]]);
});

test("a fragment a patch brought with it is one the tab starts watching", async () => {
    const window = boot(live("presence-1"));
    const [stream] = window.transport.streams;

    stream.emit("connection", "named");
    await settled();

    stream.emit("patch", live("presence-2"));
    await settled();

    assert.deepEqual(watching(window), [
        ["presence-1", "token-for-presence-1"],
        ["presence-2", "token-for-presence-2"],
    ]);
});

// The other end of the same rule. Two fragments are two topics, so the one that
// was published lands and the one that was not is untouched, down to the node:
// a patch that rebuilt its neighbour would take the caret and the bindings of
// whatever the viewer was doing there.
test("a patch lands on the fragment it names and leaves its neighbour alone", async () => {
    const window = boot(
        `<exos-live data-topic="presence-1" data-token="token-for-presence-1"><span>one</span></exos-live>` +
            `<exos-live data-topic="presence-2" data-token="token-for-presence-2"><span>two</span></exos-live>`,
    );
    const [stream] = window.transport.streams;

    stream.emit("connection", "named");
    await settled();

    const untouched = at(window, "presence-2").querySelector("span");

    stream.emit("patch", `<exos-live data-topic="presence-1"><span>changed</span></exos-live>`);
    await settled();

    assert.equal(at(window).querySelector("span").textContent, "changed");
    assert.equal(untouched.textContent, "two");
    assert.ok(
        untouched.isConnected,
        "and it is the same node, not markup that happened to match",
    );
});

// Why the topic is not the wrapper's id. A page may show one fragment twice,
// two elements may not share an id, and a morph that keyed on one would index
// the second over the first and rebuild both copies on every patch. Everything
// node identity buys, focus, selection, an open <details>, playing media, would
// be lost on both of them every time either published.
test("both copies of a fragment keep their identity across a patch", async () => {
    const window = boot(`<div id="list">${live()}${live()}</div>`);
    const [stream] = window.transport.streams;

    stream.emit("connection", "named");
    await settled();

    const before = [...window.document.querySelectorAll("exos-live")];
    assert.equal(before.length, 2);

    // The container around them, which is where a shared name does the damage:
    // the copies are morphed as its children rather than found one by one.
    stream.emit("patch", `<div id="list">${live()}${live()}</div>`);
    await settled();

    assert.deepEqual([...window.document.querySelectorAll("exos-live")], before);
});

// The server refuses a request that changes something and does not carry this,
// because a page on another origin cannot set a header without a preflight
// nobody granted. So it is not decoration on one call site: every unsafe
// request the runtime makes has to have it, and a new one that forgets is a
// feature that works nowhere but in a test.
test("every request that changes something says it came from the runtime", async () => {
    const window = boot(live());
    const [stream] = window.transport.streams;

    // A subscription, an action, and a field the server judges: the three
    // places the runtime posts from.
    stream.emit("connection", "named");
    await settled();

    await posted(window);

    const unsafe = window.transport.requests.filter((request) => request.method !== "GET");

    assert.ok(unsafe.length >= 2, "both kinds of post were made");
    assert.deepEqual(
        unsafe.filter((request) => request.headers["X-Exos"] !== "true"),
        [],
        "and each of them said so",
    );
});

// A set, not a list, because it is a set on the server. One fragment on the
// page twice is the same fragment, so it is one thing to watch and the tab must
// not name it twice.
test("a fragment the page shows twice is watched once", async () => {
    const window = boot(`${live()}${live()}`);
    const [stream] = window.transport.streams;

    stream.emit("connection", "named");
    await settled();

    assert.deepEqual(watching(window), [["presence-1", "token-for-presence-1"]]);
});

// What makes a drag silent, and the reason the pairs are sorted rather than
// read in document order. Reordering a row is a real DOM move, so every pointer
// move hands `syncSubscriptions` the same fragments in a new order; a tab that
// compared them as they sit would post an identical subscription on each one,
// for the length of the drag.
test("reordering the fragments on a page says nothing to the server", async () => {
    const window = boot(`<div id="list">${live("presence-1")}${live("presence-2")}</div>`);
    const [stream] = window.transport.streams;

    stream.emit("connection", "named");
    await settled();

    const list = window.document.getElementById("list");
    const [first, second] = [...list.children];

    for (let move = 0; move < 5; move++) {
        list.insertBefore(second, first);
        await settled();
        list.insertBefore(first, second);
        await settled();
    }

    assert.equal(
        window.transport.requests.length,
        1,
        "the set never changed, however often the page did",
    );
});

// And the coalescing proper, which is the half sorting cannot do. A page that
// changes several times in one turn is one page by the end of it, so the sync
// waits out the turn and reads the DOM once rather than replaying what each
// change would have sent.
test("a turn that changes the page repeatedly subscribes once", async () => {
    const window = boot(live("presence-0"));
    const [stream] = window.transport.streams;

    stream.emit("connection", "named");
    await settled();

    for (let n = 1; n <= 20; n++) {
        window.document.body.insertAdjacentHTML("beforeend", live(`presence-${n}`));
        window.document.dispatchEvent(new window.CustomEvent("exos:mutated"));
    }

    await settled();

    assert.equal(window.transport.requests.length, 2, "one for the page, one for the turn");
    assert.equal(watching(window).length, 21, "and it names what the turn ended with");
});

test("the stream carries every step, not only the ones a patch uses", async () => {
    const window = boot(`${live()}<input id="field">`);
    const [stream] = window.transport.streams;

    assert.deepEqual(
        [...stream.handlers.keys()].sort(),
        // The nine an Effect can be made of, plus the greeting that is not one.
        [
            "connection", "focus", "navigate", "page", "patch",
            "reload", "remove", "scroll", "signals", "title",
        ],
    );

    stream.emit("focus", "#field");
    await settled();

    assert.equal(window.document.activeElement.id, "field", "and an out of band one lands");
});

// A patch is element over element and the head is never morphed, so the title
// of a page whose contents changed under it is only ever a step.
test("a title step retitles a document a patch cannot reach", async () => {
    const window = boot(`${live()}<p id="count">1</p>`);
    const [stream] = window.transport.streams;

    stream.emit("patch", `<p id="count">2</p>`);
    stream.emit("title", "2 waiting - MyApp");
    await settled();

    assert.equal(window.document.getElementById("count").textContent, "2");
    assert.equal(window.document.title, "2 waiting - MyApp");
});

// The gap: EventSource reconnected on its own, the server had forgotten the
// connection, and whatever was published while it was gone reached nobody. The
// page renders every fragment it is showing, so fetching it back repairs all of
// them at once.
test("a reconnect fetches the page back to repair what the gap lost", async () => {
    const window = boot(`<main>${live()}<p id="count">1</p></main>`);
    const [stream] = window.transport.streams;

    stream.emit("connection", "first");
    await settled();

    // Published while nothing was listening.
    window.transport.responses.body = served(`<main>${live()}<p id="count">2</p></main>`);

    stream.emit("connection", "second");
    await settled();

    assert.equal(window.document.getElementById("count").textContent, "2");
});

// A dev build treats the same reconnect as a rebuild, because that is what it
// is: a watcher kills the process, compiles, and starts a binary whose assets
// are hashed differently. Morphing the body would leave the head pointing at
// the build that is gone.
test("a dev build keeps a stream open with nothing live on the page", () => {
    const window = bootDev(`<p>nothing live here</p>`);

    assert.equal(window.transport.streams.length, 1, "or it would never notice a restart");
    assert.equal(boot(`<p>nothing live here</p>`).transport.streams.length, 0, "a release build does not");
});

test("a dev build reloads on a reconnect rather than repairing", async () => {
    const window = bootDev(`<main>${live()}<p id="count">1</p></main>`);
    const [stream] = window.transport.streams;

    stream.emit("connection", "first");
    await settled();

    window.transport.responses.body = served(`<main>${live()}<p id="count">2</p></main>`);
    stream.emit("connection", "second");
    await settled();

    assert.equal(window.transport.navigations.length, 1, "the whole document comes back");
    assert.deepEqual(
        window.transport.requests.map((request) => request.url),
        ["/_exos/subscribe", "/_exos/subscribe"],
        "and nothing was fetched to morph in its place",
    );
});

// The failure that would make the whole thing unusable: a page that reloads on
// the greeting it gets for being loaded never finishes loading.
test("a dev build's first connection reloads nothing", async () => {
    const window = bootDev(`<p>nothing live here</p>`);

    window.transport.streams[0].emit("connection", "first");
    await settled();

    assert.deepEqual(window.transport.navigations, []);
});

test("the first connection repairs nothing, because nothing was missed", async () => {
    const window = boot(live());
    const [stream] = window.transport.streams;

    stream.emit("connection", "first");
    await settled();

    assert.deepEqual(
        window.transport.requests.map((request) => request.url),
        ["/_exos/subscribe"],
        "the document arrived a moment ago, so there is no gap behind it",
    );
});

// The detail that separates a repair from a navigation. A navigation is a
// different page saying what its signals start as; a repair is this page
// arriving again, and re-seeding it would empty a field on every hiccup.
test("a repair keeps what the viewer is holding, where a navigation would not", async () => {
    const form =
        `<main data-signals-root='{"draft":""}'>` +
        `<input data-bind="draft" data-bind-kind="string">${live()}</main>`;

    const window = boot(form);
    const [stream] = window.transport.streams;

    stream.emit("connection", "first");
    await settled();

    const field = window.document.querySelector("input");
    field.value = "half typed";
    field.dispatchEvent(new window.Event("input", { bubbles: true }));
    await settled();

    window.transport.responses.body = served(form);
    stream.emit("connection", "second");
    await settled();

    assert.equal(window.exos.signals.draft, "half typed", "a dropped stream is not a new page");
    assert.equal(window.document.querySelector("input").value, "half typed");
});

test("a repair that lands after the tab moved on is dropped", async () => {
    const window = boot(`<main>${live()}<p id="count">1</p></main>`);
    const [stream] = window.transport.streams;

    stream.emit("connection", "first");
    await settled();

    window.transport.responses.body = served(`<main>${live()}<p id="count">2</p></main>`);

    stream.emit("connection", "second");
    // Gone before the repair's fetch came back. Whatever it holds describes the
    // page that was left, and that page's arithmetic is no longer this tab's.
    window.history.pushState(null, "", "/elsewhere");
    await settled();

    assert.equal(window.document.getElementById("count").textContent, "1");
});

// An application served under a prefix has to agree with its server about what
// that prefix is. Nothing here is told: the runtime is itself an asset served
// under the same prefix, so the URL its own script came from carries the
// answer, and a wrong one would not have loaded this code at all.

test("the endpoints hang off the base the runtime was loaded from", async () => {
    const window = bootUnder("/admin", live());
    const [stream] = window.transport.streams;

    assert.equal(window.exos.base, "/admin");
    assert.equal(stream.url, "/admin/_exos/live");

    stream.emit("connection", "named");
    await settled();

    assert.equal(window.transport.requests[0].url, "/admin/_exos/subscribe");
});

test("an application at the root has no base and says nothing extra", async () => {
    const window = boot(live());
    const [stream] = window.transport.streams;

    assert.equal(window.exos.base, "");
    assert.equal(stream.url, "/_exos/live");

    stream.emit("connection", "named");
    await settled();

    assert.equal(window.transport.requests[0].url, "/_exos/subscribe");
});

test("a repair leaves the page alone when the fetch does not answer with one", async () => {
    const window = boot(`<main>${live()}<p id="count">1</p></main>`);
    const [stream] = window.transport.streams;

    stream.emit("connection", "first");
    await settled();

    window.transport.responses.status = 404;
    window.transport.responses.body = served(`<h1>not found</h1>`);

    stream.emit("connection", "second");
    await settled();

    assert.equal(window.document.getElementById("count").textContent, "1");
});

// A message whose count is client state crosses as its variants in the one
// language the page was rendered in, and the browser picks between them.
// `Intl` is the other half of the agreement the CLDR fixture holds both sides
// to, so what is checked here is that this side asks it the same question.

/** A paragraph reading a projected message off the table it arrived with. */
const projected = (say, lang = "de", count = 1) =>
    `<p id="say" data-signals-root='{"n":${count}}' ` +
    `data-messages='{"m1":{"lang":"${lang}","say":${JSON.stringify(say)}}}' ` +
    `data-text="msg('m1', $.n)"></p>`;

const GERMAN = { one: ["", " Element ausgewählt"], other: ["", " Elemente ausgewählt"] };

test("a projected message is picked by the category its count falls in", async () => {
    const window = boot(projected(GERMAN));
    await settled();

    const say = window.document.getElementById("say");
    assert.equal(say.textContent, "1 Element ausgewählt");

    window.exos.signals.n = 4;
    await settled();

    assert.equal(say.textContent, "4 Elemente ausgewählt");
});

// The category is asked about the magnitude, which is what CLDR's `n` is and
// what the server asked its own rules about: a language with a singular puts
// -1 in it just as it puts 1 there.
test("a negative count falls where its magnitude does and keeps its sign", async () => {
    const window = boot(projected(GERMAN, "de", -1));
    await settled();

    assert.equal(window.document.getElementById("say").textContent, "-1 Element ausgewählt");
});

// The number is written the way the language writes one, which is the half of
// the agreement `Intl.NumberFormat` answers for.
test("a projected count is written the way its language writes a number", async () => {
    const window = boot(projected({ other: ["", " Elemente"] }, "de", 12345));
    await settled();

    assert.equal(window.document.getElementById("say").textContent, "12.345 Elemente");
});

// A language with nothing to choose between crossed one variant, under the
// category every language has. The count here falls in `one`, which the entry
// does not carry, so what is checked is the fallback rather than a lookup that
// happens to hit.
test("a message that says one thing is read whatever the count is", async () => {
    const window = boot(projected({ other: ["Dateien: ", ""] }, "de", 1));
    await settled();

    assert.equal(window.document.getElementById("say").textContent, "Dateien: 1");
});

// A patch brings its own entries. The key is what the sentence says, so an
// entry the document already has arrives as itself and one it has never seen
// is read off the markup that carried it.
test("a patch carrying a sentence the page has never seen brings its table", async () => {
    const window = boot(`<div id="host">${projected(GERMAN)}</div>`);
    await settled();

    window.exos.applyPatch(
        `<div id="host"><p id="later" ` +
            `data-messages='{"m2":{"lang":"en","say":{"one":["","  item"],"other":["", " items"]}}}' ` +
            `data-text="msg('m2', $.n)"></p></div>`,
    );
    await settled();

    assert.equal(window.document.getElementById("later").textContent, "1  item");
});
