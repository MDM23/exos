// The page a client test runs against.
//
// A browser is handed a document, the runtime, and then whichever plugins the
// page asked for, in the order js/exos.js imports them. Everything a test does
// afterwards goes through what `window.exos` exposes or through the DOM, so a
// test knows no more about the runtime than a plugin does.

import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { JSDOM, VirtualConsole } from "jsdom";

const HERE = dirname(fileURLToPath(import.meta.url));
const read = (name) => readFileSync(join(HERE, "..", `${name}.js`), "utf8");

/** A page with the runtime loaded, the markup bound, and `plugins` applied. */
export function boot(body, ...plugins) {
    return booted(body, "", plugins);
}

/**
 * The same page as a dev build serves it.
 *
 * `exos::runtime()` puts `?dev` on the runtime's URL in a debug build, and that
 * query is the whole of what tells the client which build it is running under.
 */
export function bootDev(body, ...plugins) {
    return booted(body, "", plugins, "?dev");
}

/**
 * The same page, served under `base` rather than at the root.
 *
 * The runtime works the base out from the URL its own script was loaded from,
 * so what this changes is only the `src` of the script tag in the head, which
 * is exactly what an application changes by calling `exos::base`.
 */
export function bootUnder(base, body, ...plugins) {
    return booted(body, base, plugins);
}

function booted(body, base, plugins, query = "") {
    // The tag the runtime reads its base out of. A real page gets this from
    // `exos::runtime()`, which is the same string with the same prefix on it.
    const script = `<script defer src="${base}/_exos/exos-0123456789ab.js${query}"></script>`;

    // Leaving the page is the one thing a test cannot watch directly: jsdom's
    // `location` is unforgeable, so nothing can stand in for it. What it does
    // instead is refuse, here, and that refusal is the only trace a reload
    // leaves. Genuine exceptions still reach the console; this one would only
    // read as a failure.
    const navigations = [];
    const virtualConsole = new VirtualConsole();
    virtualConsole.forwardTo(console, { jsdomErrors: ["unhandled-exception"] });
    virtualConsole.on("jsdomError", (error) => navigations.push(error.message));

    const dom = new JSDOM(
        `<!DOCTYPE html><html><head>${script}</head><body>${body}</body></html>`,
        { runScripts: "outside-only", url: "http://localhost/", virtualConsole },
    );

    const { window } = dom;

    // One jsdom provides only to refuse.
    window.scrollTo = () => {};

    // Node's rather than jsdom's, so that the bytes the transport below encodes
    // and the runtime decodes come from one realm.
    window.TextDecoder = TextDecoder;
    window.TextEncoder = TextEncoder;

    // Installed before the runtime is evaluated, because a page that arrives
    // with a live fragment already in it opens its stream on the last line of
    // the runtime rather than waiting for a mutation.
    window.transport = transports(window, navigations);

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
function transports(window, navigations) {
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
    //
    // `type` is what decides how the runtime reads the body, and it is the
    // whole subject of any test about a refusal that still has something to
    // say, so it is settable rather than derived from the status.
    const responses = { body: null, status: 204, type: "text/html; charset=utf-8" };

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

        const text =
            responses.body ?? `<!DOCTYPE html>${window.document.documentElement.outerHTML}`;

        return Promise.resolve({
            ok: responses.status < 400,
            status: responses.status,
            statusText: "",
            headers: {
                get: (name) =>
                    name.toLowerCase() === "content-type" ? responses.type : null,
            },
            text: () => Promise.resolve(text),
            // Server-sent events arrive as bytes off a reader rather than as
            // text, because that is what lets the runtime apply a slow
            // handler's steps as they land instead of after the last one.
            body: { getReader: () => reader(window, text) },
        });
    };

    return { navigations, requests, responses, streams };
}

/**
 * A body handed over in small pieces, the way a socket delivers one.
 *
 * Deliberately not one chunk. A server-sent event ends at a blank line and a
 * read ends wherever the network says, so the two boundaries do not line up and
 * the runtime has to buffer across them. Chunking here in sevens means every
 * frame of any length is split at least once, which is the only way this
 * harness can tell a parser that buffers from one that happens to be handed
 * whole frames.
 */
function reader(window, text) {
    const bytes = new window.TextEncoder().encode(text);
    let offset = 0;

    return {
        read() {
            if (offset >= bytes.length) return Promise.resolve({ done: true });

            const chunk = bytes.slice(offset, offset + 7);
            offset += chunk.length;

            return Promise.resolve({ done: false, value: chunk });
        },
    };
}

/** Effects are scheduled on a microtask, so nothing is on the page until they run. */
export const settled = () => new Promise((resolve) => setTimeout(resolve, 0));

/** For the plugin whose whole subject is how long something has been waiting. */
export const after = (milliseconds) =>
    new Promise((resolve) => setTimeout(resolve, milliseconds));
