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

    // One jsdom provides only to refuse.
    window.scrollTo = () => {};

    // Installed before the runtime is evaluated, because a page that arrives
    // with a live fragment already in it opens its stream on the last line of
    // the runtime rather than waiting for a mutation.
    window.transport = transports(window);

    window.eval(read("runtime"));
    for (const plugin of plugins) window.eval(read(plugin));

    return window;
}

/**
 * The two transports the live stream needs and jsdom does not implement.
 *
 * This is the one place a client test knows more than a plugin does, and it
 * earns that by being the only way to reach `openStream` and
 * `syncSubscriptions` at all. Both record what the runtime did rather than
 * asserting anything about it, so a test reads the traffic the way a network
 * tab would.
 */
function transports(window) {
    const streams = [];
    const requests = [];

    // What the server answers with, so a test can be the server that has
    // forgotten this connection, or the one whose page changed while nobody
    // was listening.
    //
    // A body of `null` means the page as it stands. A reconnect repairs by
    // fetching the URL it is already on, and the ordinary answer to that is the
    // document the tab already has, so a test only says what came back when the
    // point of the test is that something did.
    const responses = { body: null, status: 204 };

    window.EventSource = class EventSource {
        constructor(url) {
            this.url = url;
            this.closed = false;
            this.handlers = new Map();
            streams.push(this);
        }

        addEventListener(name, handler) {
            this.handlers.set(name, handler);
        }

        close() {
            this.closed = true;
        }

        /** Pushes one named event, the way the wire delivers it. */
        emit(name, data) {
            this.handlers.get(name)?.({ data });
        }
    };

    window.fetch = (url, options = {}) => {
        requests.push({
            url,
            body: options.body === undefined ? null : JSON.parse(options.body),
        });

        return Promise.resolve({
            ok: responses.status < 400,
            status: responses.status,
            text: () =>
                Promise.resolve(
                    responses.body ??
                        `<!DOCTYPE html>${window.document.documentElement.outerHTML}`,
                ),
        });
    };

    return { requests, responses, streams };
}

/** Effects are scheduled on a microtask, so nothing is on the page until they run. */
export const settled = () => new Promise((resolve) => setTimeout(resolve, 0));

/** For the plugin whose whole subject is how long something has been waiting. */
export const after = (milliseconds) =>
    new Promise((resolve) => setTimeout(resolve, milliseconds));
