// exos/progress: the loading bar.
//
// The runtime announces every round trip with `exos:busy` and `exos:idle`, and
// a navigation still outstanding by the time a user notices draws the bar
// across the top of the window that they expect for one. A navigation that
// answers inside the threshold draws nothing at all, which is what keeps the
// bar from flashing at every trip.
//
// Actions are deliberately left out. The element a click came from already
// carries `aria-busy` for the duration, and a disabled button or a skeleton
// says where the work is happening better than a bar at the top of the window
// can. An action that wants one anyway calls `start` and `done` below.
//
// What it looks like is CSS. The plugin writes only how far along it is, as a
// custom property on the bar, and the default rule reads it:
//
//   :root {
//     --exos-progress-color: var(--accent);
//     --exos-progress-height: 3px;
//   }
//
// Turning it off, or waiting longer before it appears, is markup on the
// document root:
//
//   <html data-exos-progress="off" data-exos-progress-delay="300">

(() => {
    "use strict";

    // How long a round trip has to be outstanding before it is worth saying
    // anything. Under this, a bar is a flicker that reads as a rendering fault
    // rather than as progress.
    const DELAY = 150;

    // Where the bar starts, so the first paint is a stub rather than nothing.
    const START = 0.1;

    // How far it may creep on its own, and how often. Never to 1: a full bar
    // promises an answer that has not arrived.
    const CEILING = 0.9;
    const TICK = 250;

    // The share of the remaining distance each tick covers, which is what makes
    // the bar slow down the longer it waits instead of stopping dead.
    const STEP = 0.1;

    let pending = 0;
    let showing = false;
    let timer = null;
    let trickle = null;
    let value = 0;
    let bar = null;

    document.addEventListener("exos:busy", (ev) => {
        if (navigating(ev)) start();
    });

    document.addEventListener("exos:idle", (ev) => {
        if (navigating(ev)) done();
    });

    // Every round trip is announced, so which of them the bar is for is decided
    // here rather than by what the runtime bothers to report.
    function navigating(ev) {
        return ev.detail?.kind === "navigate";
    }

    function start() {
        pending += 1;

        // Only the first outstanding trip starts the clock. A second one
        // arriving while the first is still open is the same wait to the user.
        if (pending > 1 || !enabled()) return;

        timer = setTimeout(show, delay());
    }

    function done() {
        pending = Math.max(0, pending - 1);
        if (pending > 0) return;

        clearTimeout(timer);
        timer = null;
        finish();
    }

    function show() {
        timer = null;
        showing = true;

        // Back to nothing without animating there first, or a bar still fading
        // out from the last trip would visibly run backwards before it ran
        // forward. Reading the box is what makes the browser apply this before
        // the paint below rather than collapsing the two.
        value = 0;
        paint("idle");
        element().getBoundingClientRect();

        value = START;
        paint("active");
        trickle = setInterval(creep, TICK);
    }

    function creep() {
        value += (CEILING - value) * STEP;
        paint("active");
    }

    function finish() {
        if (!showing) return;

        showing = false;
        clearInterval(trickle);
        trickle = null;

        value = 1;
        paint("done");

        // How long the bar takes to fade is a transition, so the stylesheet is
        // what says when it is gone. Guessing here would make a theme that
        // changes the duration cut its own fade short.
        setTimeout(reset, fade());
    }

    function reset() {
        // A trip that started during the fade owns the bar now.
        if (showing) return;

        value = 0;
        paint("idle");
    }

    function paint(state) {
        const el = element();

        el.dataset.state = state;
        el.style.setProperty("--exos-progress-value", value.toFixed(3));
    }

    /** What the stylesheet says the fade takes, so nothing here has to guess. */
    function fade() {
        const durations = getComputedStyle(element()).transitionDuration.split(",");
        return Math.max(0, ...durations.map((each) => (parseFloat(each) || 0) * 1000));
    }

    // Defaults, not a theme: every value a page might want to change is a
    // custom property with a fallback, so overriding one takes a declaration
    // rather than a rewrite.
    const CSS = `
.exos-progress {
    position: fixed;
    inset-block-start: 0;
    inset-inline: 0;
    z-index: var(--exos-progress-z-index, 9999);
    block-size: var(--exos-progress-height, 2px);
    background: var(--exos-progress-color, currentColor);
    box-shadow: var(--exos-progress-shadow, none);
    opacity: 0;
    pointer-events: none;
    transform: scaleX(var(--exos-progress-value, 0));
    transform-origin: 0 50%;
    transition:
        transform var(--exos-progress-duration, 200ms) ease-out,
        opacity var(--exos-progress-fade, 200ms) linear;
}

.exos-progress:dir(rtl) { transform-origin: 100% 50%; }
.exos-progress[data-state="active"] { opacity: 1; }
.exos-progress[data-state="idle"] { transition: none; }

@media (prefers-reduced-motion: reduce) {
    .exos-progress { transition-duration: 0s; }
}
`;

    /** The bar, built on the first trip slow enough to need one. */
    function element() {
        if (bar) return bar;

        const style = document.createElement("style");
        style.textContent = CSS;

        // First in <head>, so a page rule of the same specificity wins by
        // coming later. A default that needs `!important` to override is not a
        // default.
        document.head.prepend(style);

        bar = document.createElement("div");
        bar.className = "exos-progress";
        bar.dataset.state = "idle";

        // Purely decorative, and a live region that announced every fetch would
        // be worse than silence.
        bar.setAttribute("aria-hidden", "true");

        // Outside <body>, because a navigation morphs the body against the
        // document the server sent, and would take the bar with it at exactly
        // the moment it is on screen.
        document.documentElement.appendChild(bar);

        return bar;
    }

    function enabled() {
        return document.documentElement.dataset.exosProgress !== "off";
    }

    function delay() {
        const given = document.documentElement.dataset.exosProgressDelay;
        const milliseconds = Number(given);

        // An absent attribute is `undefined` and an empty one is "", and
        // `Number` turns the second into 0, so neither may reach the comparison
        // as a number or a page would get no threshold at all.
        return given && Number.isFinite(milliseconds) && milliseconds >= 0 ? milliseconds : DELAY;
    }

    // For work the runtime does not make: a fetch of your own, an upload, a
    // long client-side computation. Balanced calls, so `done` belongs in a
    // `finally`.
    window.exos.progress = { start, done };
})();
