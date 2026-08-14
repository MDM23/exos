// The page a client test runs against.
//
// A browser is handed a document, the runtime, and then whichever plugins the
// page asked for, in the order js/exos.js imports them. Everything a test does
// afterwards goes through what `window.exos` exposes or through the DOM, so a
// test knows no more about the runtime than a plugin does.

import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { JSDOM } from "jsdom";

const HERE = dirname(fileURLToPath(import.meta.url));
const read = (name) => readFileSync(join(HERE, "..", `${name}.js`), "utf8");

/** A page with the runtime loaded, the markup bound, and `plugins` applied. */
export function boot(body, ...plugins) {
    const dom = new JSDOM(`<!DOCTYPE html><html><body>${body}</body></html>`, {
        runScripts: "outside-only",
        url: "http://localhost/",
    });

    const { window } = dom;

    // One the runtime reads while it is still loading and jsdom does not
    // provide, and one jsdom provides only to refuse.
    window.crypto.randomUUID ??= () => "00000000-0000-4000-8000-000000000000";
    window.scrollTo = () => {};

    window.eval(read("runtime"));
    for (const plugin of plugins) window.eval(read(plugin));

    return window;
}

/** Effects are scheduled on a microtask, so nothing is on the page until they run. */
export const settled = () => new Promise((resolve) => setTimeout(resolve, 0));

/** For the plugin whose whole subject is how long something has been waiting. */
export const after = (milliseconds) =>
    new Promise((resolve) => setTimeout(resolve, milliseconds));
