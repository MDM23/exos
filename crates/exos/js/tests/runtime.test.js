// Tests for the client runtime.
//
// Everything here is the half of the framework Rust cannot reach. The server
// side is checked by the crate's own tests, which assert what is rendered; the
// bugs that got through were all on this side of the boundary, in scoping,
// morphing and the two of them meeting.

import assert from "node:assert/strict";
import { test } from "node:test";

import { boot, settled } from "./harness.js";

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

/** One live fragment, as the server renders it: a name and the proof of it. */
const live = (id = "presence-1") => `<exos-live id="${id}" data-token="token-for-${id}"></exos-live>`;

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

    assert.deepEqual(
        window.transport.requests.map((request) => request.body.connection),
        ["first", "second"],
    );
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

    assert.equal(window.transport.requests.at(-1).body.connection, "fresh");
});
