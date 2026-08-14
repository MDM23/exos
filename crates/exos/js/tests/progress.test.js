// Tests for the loading bar.
//
// The plugin's whole subject is timing, so these wait rather than pretend: the
// thresholds it reads are milliseconds, and a test that stubbed the clock
// would be asserting against its own stub.

import assert from "node:assert/strict";
import { test } from "node:test";

import { after, boot } from "./harness.js";

/** A round trip, in the two events the runtime announces one with. */
function announce(window, step, kind) {
    window.document.dispatchEvent(new window.CustomEvent(`exos:${step}`, { detail: { kind } }));
}

const bar = (window) => window.document.querySelector(".exos-progress");

/**
 * A page whose bar appears almost at once, so a test is not mostly waiting.
 *
 * Closed when the test ends: a bar that is still creeping holds an interval,
 * and jsdom keeps the timers of a window nobody closed, which leaves the
 * runner waiting on an event loop that never empties.
 */
function impatient(t, ...attributes) {
    const window = boot("", "progress");
    t.after(() => window.close());

    window.document.documentElement.dataset.exosProgressDelay = "10";
    for (const [name, value] of attributes) {
        window.document.documentElement.setAttribute(name, value);
    }

    return window;
}

// The bar exists to say that something is taking a while. Drawing one for a
// trip that answers immediately reads as a rendering fault rather than as
// progress, so nothing is built at all until the threshold passes.
test("a navigation that answers quickly draws nothing", async (t) => {
    const window = boot("", "progress");
    t.after(() => window.close());

    announce(window, "busy", "navigate");
    announce(window, "idle", "navigate");
    await after(220);

    assert.equal(bar(window), null);
});

test("a navigation slow enough to notice draws the bar", async (t) => {
    const window = impatient(t);

    announce(window, "busy", "navigate");
    await after(40);

    assert.equal(bar(window).dataset.state, "active");
    assert.ok(Number(bar(window).style.getPropertyValue("--exos-progress-value")) > 0);
});

// A navigation morphs the body against the document the server sent, and would
// take the bar with it at exactly the moment it is on screen.
test("the bar sits outside the body", async (t) => {
    const window = impatient(t);

    announce(window, "busy", "navigate");
    await after(40);

    assert.equal(bar(window).parentElement, window.document.documentElement);
});

// An action already marks the element it came from with aria-busy, which says
// where the work is happening better than a bar at the top of the window can.
test("an action never draws one", async (t) => {
    const window = impatient(t);

    announce(window, "busy", "request");
    await after(40);

    assert.equal(bar(window), null);
});

test("the document root can turn it off", async (t) => {
    const window = impatient(t, ["data-exos-progress", "off"]);

    announce(window, "busy", "navigate");
    await after(40);

    assert.equal(bar(window), null);
});

// Two trips overlapping is one wait to the person watching.
test("a second trip does not start a second bar", async (t) => {
    const window = impatient(t);

    announce(window, "busy", "navigate");
    announce(window, "busy", "navigate");
    await after(40);
    assert.equal(bar(window).dataset.state, "active");

    announce(window, "idle", "navigate");
    await after(20);
    assert.equal(bar(window).dataset.state, "active", "one trip is still outstanding");

    announce(window, "idle", "navigate");
    await after(20);
    assert.equal(bar(window).dataset.state, "idle");
});

// For a fetch of your own, an upload, a long computation: the same bar, driven
// by hand, which is why `done` belongs in a `finally`.
test("work the runtime does not make drives the same bar", async (t) => {
    const window = impatient(t);

    window.exos.progress.start();
    await after(40);
    assert.equal(bar(window).dataset.state, "active");

    window.exos.progress.done();
    await after(20);
    assert.equal(bar(window).dataset.state, "idle");
});
