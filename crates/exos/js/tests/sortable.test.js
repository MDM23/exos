// Tests for drag to reorder.
//
// The plugin reads boxes and jsdom lays nothing out, so the rows are given a
// layout below. Everything else is real: the pointer sequence, the DOM moves
// it makes, and the one expression it runs when the user lets go.

import assert from "node:assert/strict";
import { test } from "node:test";

import { boot } from "./harness.js";

const ROW = 20;

/** Rows twenty pixels tall, measured from wherever they currently sit. */
function layout(list) {
    for (const row of list.children) {
        row.getBoundingClientRect = () => {
            const top = [...list.children].indexOf(row) * ROW;
            return { bottom: top + ROW, height: ROW, left: 0, right: 0, top, width: 0, x: 0, y: top };
        };
    }
}

/** A pointer event, which jsdom does not implement. */
function pointer(window, type, target, y) {
    const event = new window.MouseEvent(type, {
        bubbles: true,
        button: 0,
        cancelable: true,
        clientX: 0,
        clientY: y,
    });

    Object.defineProperty(event, "pointerId", { value: 1 });
    target.dispatchEvent(event);
}

/**
 * A list that reports its new order into a signal, so a test can see both that
 * the expression ran and which `_order` it read.
 */
function sortable({ handles = false } = {}) {
    const handle = handles ? "<span data-drag-handle>::</span>" : "";
    const rows = [1, 2, 3]
        .map((id) => `<li id="row-${id}" data-sort-item="${id}">${handle}</li>`)
        .join("");

    return (
        `<ul id="list" data-signals='{"_order":null}' ` +
        `data-sortable="$.reordered = $._order.join(',')">${rows}</ul>`
    );
}

/** The ids in the order the DOM has them. */
const ids = (list) => [...list.children].map((row) => row.getAttribute("data-sort-item"));

/** A page holding one sortable list, laid out and ready to drag. */
function open(markup) {
    const window = boot(markup, "sortable");
    const list = window.document.getElementById("list");

    layout(list);
    return { list, window };
}

test("a press that goes nowhere is a click rather than a drag", () => {
    const { list, window } = open(sortable());
    const first = window.document.getElementById("row-1");

    pointer(window, "pointerdown", first, 10);
    pointer(window, "pointermove", first, 12);
    pointer(window, "pointerup", first, 12);

    assert.ok(!first.classList.contains("is-dragging"));
    assert.deepEqual(ids(list), ["1", "2", "3"]);
    assert.equal(window.exos.signals.reordered, undefined);
});

test("dragging past a neighbour moves the row under the pointer", () => {
    const { list, window } = open(sortable());
    const first = window.document.getElementById("row-1");

    pointer(window, "pointerdown", first, 10);
    pointer(window, "pointermove", first, 35);

    assert.ok(first.classList.contains("is-dragging"));
    assert.deepEqual(ids(list), ["2", "1", "3"]);
});

test("letting go tells the server once, in the container's scope", () => {
    const { list, window } = open(sortable());
    const first = window.document.getElementById("row-1");

    // Copied out of the page's realm, where an array is a different Array from
    // this one and deepEqual says so.
    const announced = [];
    list.addEventListener("exos:reordered", (event) => announced.push([...event.detail.order]));

    pointer(window, "pointerdown", first, 10);
    pointer(window, "pointermove", first, 35);
    pointer(window, "pointerup", first, 35);

    assert.deepEqual(announced, [["2", "1", "3"]]);
    assert.deepEqual([...window.exos.readIn(list, "_order")], ["2", "1", "3"]);

    // The expression ran, and read the same signal the plugin wrote: the one
    // the list declares, not the global that merely shares its name.
    assert.equal(window.exos.signals.reordered, "2,1,3");
    assert.equal(window.exos.signals._order, undefined);
});

test("a drag that ends where it started says nothing", () => {
    const { list, window } = open(sortable());
    const first = window.document.getElementById("row-1");

    pointer(window, "pointerdown", first, 10);
    pointer(window, "pointermove", first, 18);
    pointer(window, "pointerup", first, 18);

    assert.deepEqual(ids(list), ["1", "2", "3"]);
    assert.equal(window.exos.signals.reordered, undefined);
});

test("a handle, where there is one, is the only way to start a drag", () => {
    const { list, window } = open(sortable({ handles: true }));
    const first = window.document.getElementById("row-1");

    pointer(window, "pointerdown", first, 10);
    pointer(window, "pointermove", first, 35);

    assert.ok(!first.classList.contains("is-dragging"));
    assert.deepEqual(ids(list), ["1", "2", "3"]);
});
