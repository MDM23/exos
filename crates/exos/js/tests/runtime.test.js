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
