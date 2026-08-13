// The exos client runtime.
//
// Two mechanisms, chosen deliberately, and between them the reason markup that
// arrives after page load just works:
//
//   * Events are delegated. One listener per type on `document`, dispatching
//     on attributes at event time. Nothing is bound to an element, so an
//     element rendered ten minutes from now is already wired and there is
//     nothing to initialize.
//
//   * Bindings are observed. A binding genuinely needs per-element state, an
//     effect and its cleanup, so a MutationObserver applies them to new nodes
//     and disposes them on removal. The rule is never to bind once and assume
//     the DOM stopped changing.
//
// Expressions are real JavaScript. Signals are reached through `$`, so
// `$.open = !$.open` is an ordinary assignment rather than a dialect this
// runtime invented.

(() => {
    "use strict";

    // -------------------------------------------------------------------------
    //                                REACTIVITY
    // -------------------------------------------------------------------------

    // Push-based: reading a signal inside an effect subscribes that effect,
    // and writing re-runs subscribers on a microtask so a handler that writes
    // several signals causes one DOM pass rather than several.

    const store = new Map(); // name -> { value, subscribers: Set<Effect> }
    const queue = new Set();
    let activeEffect = null;
    let flushing = false;

    function slot(name) {
        let entry = store.get(name);

        if (!entry) {
            entry = { value: undefined, subscribers: new Set() };
            store.set(name, entry);
        }

        return entry;
    }

    function read(name) {
        const entry = slot(name);

        if (activeEffect) {
            entry.subscribers.add(activeEffect);
            activeEffect.sources.add(entry);
        }

        return entry.value;
    }

    function write(name, value) {
        const entry = slot(name);
        if (Object.is(entry.value, value)) return value;

        entry.value = value;
        for (const effect of entry.subscribers) schedule(effect);

        return value;
    }

    function schedule(effect) {
        queue.add(effect);
        if (flushing) return;

        flushing = true;
        queueMicrotask(() => {
            flushing = false;
            const pending = [...queue];
            queue.clear();
            for (const effect of pending) run(effect);
        });
    }

    function run(effect) {
        if (effect.disposed) return;

        // Drop old subscriptions so a branch no longer taken stops updating.
        for (const source of effect.sources) source.subscribers.delete(effect);
        effect.sources.clear();

        const previous = activeEffect;
        activeEffect = effect;

        try {
            effect.fn();
        } catch (error) {
            console.error("[exos] effect failed:", error, effect.el);
        } finally {
            activeEffect = previous;
        }
    }

    function effect(el, fn) {
        const created = { fn, el, sources: new Set(), disposed: false };
        run(created);
        return created;
    }

    function dispose(effect) {
        effect.disposed = true;
        for (const source of effect.sources) source.subscribers.delete(effect);
        effect.sources.clear();
    }

    // -------------------------------------------------------------------------
    //                                  SCOPES
    // -------------------------------------------------------------------------

    // An element that declares signals opens a scope, and its id names it. A
    // row already needs an id for morphing to key on, so a hundred rows each
    // holding a `fav` signal need no unique names invented for them: inside
    // `#file-3`, `$.fav` is `file-3/fav`.
    //
    // Resolution walks up and takes the nearest scope that actually declares
    // the name, so an inner scope shadows an outer one and anything undeclared
    // is global. That is lexical scoping with the DOM as the tree.
    //
    // A model's fields are declared on the document instead, wherever the
    // element that declares them sits, because a handler writes them with
    // Effect::set and that resolves from the root. They are the names that
    // exist to be reachable from off the page; everything else is a row's own.

    const scopes = new WeakMap(); // element -> scope id
    const initialized = new WeakSet();
    let scopeCounter = 0;

    function scopeOf(el) {
        let scope = scopes.get(el);
        if (scope) return scope;

        // An id is preferred because it survives a morph, and because it is
        // already what identifies this element to the server.
        scope = el.id || `exos-${++scopeCounter}`;
        scopes.set(el, scope);
        return scope;
    }

    function resolve(el, name) {
        for (let node = el; node; node = node.parentElement) {
            const scope = scopes.get(node);
            if (scope && store.has(`${scope}/${name}`)) return `${scope}/${name}`;
        }

        return name;
    }

    function namespace(el) {
        return new Proxy(
            {},
            {
                get: (_, name) => read(resolve(el, name)),
                set: (_, name, value) => (write(resolve(el, name), value), true),
                has: () => true,
                ownKeys: () => [...store.keys()],
                getOwnPropertyDescriptor: () => ({ enumerable: true, configurable: true }),
            },
        );
    }

    // The global namespace, for plugins and for the console.
    const $ = namespace(document.documentElement);

    // -------------------------------------------------------------------------
    //                                EXPRESSIONS
    // -------------------------------------------------------------------------

    // Compiled once per source string and cached, so the same handler on a
    // thousand rows costs one Function.

    const compiled = new Map();

    function compile(source, statement) {
        const key = (statement ? "!" : "=") + source;
        const hit = compiled.get(key);
        if (hit) return hit;

        let fn;
        try {
            fn = new Function(
                "$", "el", "ev",
                "get", "post", "put", "patch", "del",
                "attr", "append",
                statement ? source : `return (${source})`,
            );
        } catch (error) {
            console.error("[exos] bad expression:", source, error);
            fn = () => undefined;
        }

        compiled.set(key, fn);
        return fn;
    }

    // A speculative DOM write, for optimistic updates over state the server
    // owns.
    //
    // Deliberately not a signal. Mirroring server state into a signal gives one
    // attribute two sources of truth, and they drift the moment a patch lands:
    // the morph writes the server's value while the signal still holds the
    // client's. A speculative write has no second copy, so the next patch
    // corrects it either way.
    function speculate(el, name, value) {
        const target = el.closest("[id]") ?? el;

        // A `data-*` attribute is usually matched by CSS in both states, so
        // `false` is written as the string rather than removed. A missing
        // attribute and `="false"` are different selectors, and confusing them
        // silently breaks styling.
        if (value == null) target.removeAttribute(name);
        else if (name.startsWith("data-")) target.setAttribute(name, String(value));
        else if (value === false) target.removeAttribute(name);
        else target.setAttribute(name, value === true ? "" : String(value));
    }

    // Clones a <template> into a container, giving the clone a fresh id so it
    // becomes its own signal scope.
    let appended = 0;

    function appendTemplate(templateSelector, intoSelector) {
        const template = document.querySelector(templateSelector);
        const into = document.querySelector(intoSelector);

        if (!template || !into) {
            console.error("[exos] append: missing", templateSelector, intoSelector);
            return;
        }

        const clone = template.content.cloneNode(true);
        for (const el of clone.children) {
            if (!el.id) el.id = `exos-row-${++appended}`;
        }

        into.appendChild(clone);
    }

    function evaluate(source, el, ev, statement) {
        const action = (method) => (url, data) => request(method, url, el, data);

        return compile(source, statement)(
            // Scoped to where the expression is written, not to where the
            // runtime happens to be standing.
            namespace(el),
            el,
            ev,
            action("GET"),
            action("POST"),
            action("PUT"),
            action("PATCH"),
            action("DELETE"),
            (name, value) => speculate(el, name, value),
            appendTemplate,
        );
    }

    // -------------------------------------------------------------------------
    //                                 BINDINGS
    // -------------------------------------------------------------------------

    // Each is an effect, and the element owns its effects so they can be
    // disposed when it leaves the DOM.

    const bound = new WeakMap(); // element -> Effect[]
    const CHECKABLE = new Set(["checkbox", "radio"]);

    const BINDINGS = {
        // data-attr="{'data-favorite': $.fav ? 'true' : 'false'}"
        "data-attr": (el, source) => () => {
            const values = evaluate(source, el, null, false) || {};

            for (const [name, value] of Object.entries(values)) {
                if (value === false || value == null) el.removeAttribute(name);
                else el.setAttribute(name, value === true ? "" : String(value));
            }
        },

        // data-bind="picked" is two-way for a form control. The value flows
        // signal to element here; element to signal is the delegated listener
        // below, so a control a patch inserts needs no wiring.
        "data-bind": (el, name) => () => {
            const value = read(resolve(el, name));

            if (el.type === "checkbox") {
                // A checkbox bound to an array collects its `value`, which is
                // what makes selecting many rows need no per-row bookkeeping.
                el.checked = Array.isArray(value)
                    ? value.map(String).includes(el.value)
                    : Boolean(value);
            } else if (el.type === "radio") {
                el.checked = String(value) === el.value;
            } else if (el.value !== String(value ?? "")) {
                el.value = String(value ?? "");
            }
        },

        // data-class="{active: $.open}"
        "data-class": (el, source) => () => {
            const values = evaluate(source, el, null, false) || {};
            for (const [name, on] of Object.entries(values)) el.classList.toggle(name, !!on);
        },

        // data-prop="{value: $.query}", for what are properties rather than
        // attributes: value, checked, indeterminate.
        "data-prop": (el, source) => () => {
            const values = evaluate(source, el, null, false) || {};

            for (const [name, value] of Object.entries(values)) {
                if (el[name] !== value) el[name] = value;
            }
        },

        // data-show="$.open" toggles the `hidden` attribute.
        "data-show": (el, source) => () => {
            el.toggleAttribute("hidden", !evaluate(source, el, null, false));
        },

        // data-text="$.name"
        "data-text": (el, source) => () => {
            el.textContent = evaluate(source, el, null, false) ?? "";
        },
    };

    const BINDING_SELECTOR = Object.keys(BINDINGS)
        .concat("data-signals", "data-signals-root")
        .map((name) => `[${name}]`)
        .join(",");

    // Declares, and never overwrites, so a morph that re-delivers the same
    // markup does not reset live state, and a name a second row declares is
    // the one the first row already put there.
    function declare(el, attribute, key) {
        const declared = el.getAttribute(attribute);
        if (!declared) return;

        try {
            for (const [name, value] of Object.entries(JSON.parse(declared))) {
                const slot = key(name);
                if (!store.has(slot)) write(slot, value);
            }
        } catch (error) {
            console.error(`[exos] bad ${attribute}:`, declared, error);
        }
    }

    function bind(el) {
        if (initialized.has(el)) return;
        initialized.add(el);

        const effects = [];

        // data-signals='{"open": false}' belongs to this element, which
        // becomes the scope those names live in.
        declare(el, "data-signals", (name) => `${scopeOf(el)}/${name}`);

        // data-signals-root='{"s1f4c20a9": ""}' belongs to the document. A
        // model's fields are declared this way wherever they appear, so that
        // the signal a template binds is the one Effect::set writes.
        declare(el, "data-signals-root", (name) => name);

        for (const [attribute, make] of Object.entries(BINDINGS)) {
            const source = el.getAttribute(attribute);
            if (source !== null) effects.push(effect(el, make(el, source)));
        }

        if (effects.length) bound.set(el, effects);
    }

    function unbind(el) {
        initialized.delete(el);

        // The element is gone, so its scope's signals are unreachable and
        // would otherwise sit in the store forever. What it declared on the
        // document stays: those names belong to the page, and the row that
        // happened to carry the declaration is not what they were about.
        const scope = scopes.get(el);

        if (scope) {
            const prefix = `${scope}/`;
            for (const name of [...store.keys()]) {
                if (name.startsWith(prefix)) store.delete(name);
            }
            scopes.delete(el);
        }

        const effects = bound.get(el);
        if (effects) {
            for (const effect of effects) dispose(effect);
            bound.delete(el);
        }
    }

    function bindTree(root) {
        if (root.nodeType !== Node.ELEMENT_NODE) return;
        if (root.matches(BINDING_SELECTOR)) bind(root);
        for (const el of root.querySelectorAll(BINDING_SELECTOR)) bind(el);
    }

    function unbindTree(root) {
        if (root.nodeType !== Node.ELEMENT_NODE) return;
        unbind(root);
        for (const el of root.querySelectorAll(BINDING_SELECTOR)) unbind(el);
    }

    const observer = new MutationObserver((records) => {
        for (const record of records) {
            for (const node of record.removedNodes) {
                // A move shows up as a removal followed by an insertion. Only
                // a node that is really gone should lose its bindings, or
                // reordering a list would dispose and rebuild effects that
                // never stopped being valid.
                if (!node.isConnected) unbindTree(node);
            }

            for (const node of record.addedNodes) bindTree(node);
        }

        document.dispatchEvent(new CustomEvent("exos:mutated"));
    });

    observer.observe(document.documentElement, { childList: true, subtree: true });

    // -------------------------------------------------------------------------
    //                                  EVENTS
    // -------------------------------------------------------------------------

    // Pure delegation, and the reason a row the server renders five seconds
    // from now needs no initialization.

    const DELEGATED = [
        "change", "click", "dblclick", "focusin", "focusout",
        "input", "keydown", "keyup", "pointerdown", "pointerup", "submit",
    ];

    // Each control listens to exactly one event. A checkbox fires `input` and
    // `change` for a single click, so handling both toggles the value twice
    // and leaves it where it started, which looks exactly like a control that
    // does not respond.
    function bindingEvent(el) {
        if (CHECKABLE.has(el.type) || el.tagName === "SELECT") return "change";
        return "input";
    }

    // An `<input>` always yields a string, but the signal's Rust type decides
    // what the server accepts: pushing "1" into a Vec<u32> fails to
    // deserialize. `data-bind-kind` carries that type across.
    function coerce(value, kind) {
        if (kind === "number") return Number(value);
        if (kind === "bool") return value === "true";
        return value;
    }

    for (const type of ["change", "input"]) {
        document.addEventListener(type, (ev) => {
            const el = ev.target.closest?.("[data-bind]");
            if (!el || bindingEvent(el) !== type) return;

            const key = resolve(el, el.getAttribute("data-bind"));
            const current = read(key);
            const kind = el.getAttribute("data-bind-kind");

            if (el.type === "checkbox" && Array.isArray(current)) {
                const value = el.value;
                const next = current.map(String).includes(value)
                    ? current.filter((held) => String(held) !== value)
                    : [...current, coerce(value, kind)];

                write(key, next);
            } else if (CHECKABLE.has(el.type)) {
                write(key, el.type === "radio" ? coerce(el.value, kind) : el.checked);
            } else {
                write(key, coerce(el.value, kind));
            }
        });
    }

    for (const type of DELEGATED) {
        document.addEventListener(type, (ev) => {
            const attribute = `data-on-${type}`;
            const el = ev.target.closest?.(`[${attribute}]`);
            if (!el) return;

            if (type === "submit") ev.preventDefault();
            evaluate(el.getAttribute(attribute), el, ev, true);
        });
    }

    // -------------------------------------------------------------------------
    //                                 REQUESTS
    // -------------------------------------------------------------------------

    // An action sends what it is handed and nothing else:
    //
    //     post('/files/reorder', { order: $._order })
    //
    // Not the whole signal store. Ambient state made every endpoint's real
    // inputs invisible and let a renamed signal break a handler silently; a
    // named payload puts the dependency in the call and in the handler's
    // signature, where it can be typed.

    // Every round trip is announced, so an indicator can show one that outlives
    // what a user waits for without noticing. Always balanced: `idle` is
    // dispatched from a `finally`, or a request that failed would leave the
    // page looking busy forever.
    function announce(step, detail) {
        document.dispatchEvent(new CustomEvent(`exos:${step}`, { detail }));
    }

    async function request(method, url, el, data) {
        const init = { method, headers: { "X-Exos": "true" } };
        let target = url;

        if (data !== undefined) {
            if (method === "GET") {
                const query = new URL(url, location.origin);

                for (const [key, value] of Object.entries(data)) {
                    query.searchParams.set(
                        key,
                        typeof value === "string" ? value : JSON.stringify(value),
                    );
                }

                target = query.toString();
            } else {
                init.headers["Content-Type"] = "application/json";
                init.body = JSON.stringify(data);
            }
        }

        el?.setAttribute("aria-busy", "true");
        announce("busy", { kind: "request", method, url });

        try {
            const response = await fetch(target, init);

            // A rejected action used to fail in silence: the server answered
            // 4xx, nothing was applied, and the page simply did not change,
            // which reads exactly like a feature that is not wired up.
            if (!response.ok) {
                console.error(
                    `[exos] ${method} ${url} answered ${response.status} ${response.statusText}`,
                    "\nsent:", init.body ?? "(no body)",
                    "\nfrom:", el,
                );

                document.dispatchEvent(
                    new CustomEvent("exos:error", {
                        detail: { method, url, status: response.status, body: init.body },
                    }),
                );

                return response;
            }

            const type = response.headers.get("content-type") ?? "";
            if (type.includes("text/event-stream")) await consume(response);
            else if (type.includes("text/html")) applyPatch(await response.text());

            return response;
        } finally {
            el?.removeAttribute("aria-busy");
            announce("idle", { kind: "request", method, url });
        }
    }

    // -------------------------------------------------------------------------
    //                                  EFFECTS
    // -------------------------------------------------------------------------

    // An action answers with server-sent-event frames, the same format the
    // live stream uses, so this is the only place that interprets either.

    async function consume(response) {
        const reader = response.body.getReader();
        const decoder = new TextDecoder();
        let buffer = "";

        for (;;) {
            const { done, value } = await reader.read();
            if (done) break;

            buffer += decoder.decode(value, { stream: true });

            let boundary = buffer.indexOf("\n\n");
            while (boundary !== -1) {
                dispatch(buffer.slice(0, boundary));
                buffer = buffer.slice(boundary + 2);
                boundary = buffer.indexOf("\n\n");
            }
        }
    }

    function dispatch(frame) {
        let event = "message";
        const data = [];

        for (const line of frame.split("\n")) {
            if (line.startsWith("event:")) event = line.slice(6).trim();
            else if (line.startsWith("data:")) data.push(line.slice(5).replace(/^ /, ""));
        }

        apply(event, data.join("\n"));
    }

    function apply(step, payload) {
        switch (step) {
            case "focus":
                document.querySelector(payload)?.focus();
                break;

            case "navigate":
                navigate(payload, true);
                break;

            case "page": {
                const next = new DOMParser().parseFromString(payload, "text/html");
                morph(document.body, next.body);
                if (next.title) document.title = next.title;
                break;
            }

            case "patch":
                applyPatch(payload);
                break;

            case "reload":
                location.reload();
                break;

            case "remove":
                for (const node of document.querySelectorAll(payload)) {
                    unbindTree(node);
                    node.remove();
                }
                break;

            case "scroll":
                document.querySelector(payload)?.scrollIntoView({ behavior: "smooth" });
                break;

            case "signals":
                try {
                    Object.assign($, JSON.parse(payload));
                } catch (error) {
                    console.error("[exos] bad signals payload:", payload, error);
                }
                break;

            default:
                console.warn("[exos] unknown effect step:", step);
        }
    }

    // -------------------------------------------------------------------------
    //                                 PATCHING
    // -------------------------------------------------------------------------

    // A patch is plain HTML. Every top-level element with an id is morphed
    // over the element that already has that id. No swap strategies and no
    // target selectors on the client: the server names what it is replacing by
    // giving it an id, which it had to do anyway.

    function applyPatch(html) {
        const template = document.createElement("template");
        template.innerHTML = html;

        for (const incoming of [...template.content.children]) {
            const id = incoming.id;
            if (!id) continue;

            // querySelectorAll rather than getElementById: one fragment can
            // legitimately appear more than once on a page, and those copies
            // share an id because they are the same fragment. Updating only
            // the first would leave the rest stale.
            const existing = document.querySelectorAll(`[id="${CSS.escape(id)}"]`);

            if (existing.length) {
                for (const node of existing) morph(node, incoming.cloneNode(true));
            } else {
                document.body.appendChild(incoming);
            }
        }
    }

    // Morphing, keyed by id and falling back to position. Preserving element
    // identity is what keeps focus, scroll, selection, media playback and
    // binding state alive across an update.
    function morph(from, to) {
        if (from.nodeType !== to.nodeType || from.nodeName !== to.nodeName) {
            from.replaceWith(to);
            bindTree(to);
            return;
        }

        if (from.nodeType === Node.TEXT_NODE || from.nodeType === Node.COMMENT_NODE) {
            if (from.nodeValue !== to.nodeValue) from.nodeValue = to.nodeValue;
            return;
        }

        if (from.nodeType !== Node.ELEMENT_NODE) return;

        // An element can opt out of ever being touched: a media player, an
        // open <details>, a third-party widget.
        if (from.hasAttribute("data-preserve")) return;

        syncAttributes(from, to);
        morphChildren(from, to);
    }

    function syncAttributes(from, to) {
        for (const { name, value } of [...to.attributes]) {
            if (from.getAttribute(name) !== value) from.setAttribute(name, value);
        }

        for (const { name } of [...from.attributes]) {
            if (!to.hasAttribute(name)) from.removeAttribute(name);
        }

        // Properties drift from their attributes once a user touches them, so
        // the server's markup has to be re-applied, unless a binding owns the
        // property. Clobbering a bound control would throw away a selection or
        // a half-typed value every time an unrelated patch arrived, which is
        // the same two-sources-of-truth mistake as mirroring server state into
        // a signal.
        const isBound = from.hasAttribute("data-bind");
        const isField = from instanceof HTMLInputElement || from instanceof HTMLTextAreaElement;

        if (!isBound && isField) {
            const value = to.getAttribute("value");
            if (value !== null && from.value !== value) from.value = value;
            if (from instanceof HTMLInputElement) from.checked = to.hasAttribute("checked");
        }
    }

    function morphChildren(from, to) {
        // Index the survivors by id so a reorder moves nodes rather than
        // rebuilding them.
        const keyed = new Map();
        for (const child of from.children) {
            if (child.id) keyed.set(child.id, child);
        }

        let cursor = from.firstChild;

        for (const incoming of [...to.childNodes]) {
            const key = incoming.nodeType === Node.ELEMENT_NODE && incoming.id;
            const match = key ? keyed.get(key) : null;

            if (match) {
                if (match === cursor) cursor = cursor.nextSibling;
                else from.insertBefore(match, cursor);

                morph(match, incoming);
                keyed.delete(key);
                continue;
            }

            // A same-shaped unkeyed node in the same slot updates in place.
            const reusable =
                cursor &&
                cursor.nodeType === incoming.nodeType &&
                cursor.nodeName === incoming.nodeName &&
                !(cursor.nodeType === Node.ELEMENT_NODE && cursor.id && keyed.has(cursor.id)) &&
                !(incoming.nodeType === Node.ELEMENT_NODE && incoming.id);

            if (reusable) {
                const next = cursor.nextSibling;
                morph(cursor, incoming);
                cursor = next;
                continue;
            }

            const fresh = incoming.cloneNode(true);
            from.insertBefore(fresh, cursor);
            bindTree(fresh);
        }

        while (cursor) {
            const next = cursor.nextSibling;
            unbindTree(cursor);
            cursor.remove();
            cursor = next;
        }

        for (const orphan of keyed.values()) {
            unbindTree(orphan);
            orphan.remove();
        }
    }

    // -------------------------------------------------------------------------
    //                                NAVIGATION
    // -------------------------------------------------------------------------

    // The same fetch-and-morph machinery applied to whole documents, so the
    // shell keeps its identity and anything live inside it survives.

    document.addEventListener("click", (ev) => {
        if (ev.defaultPrevented || ev.button !== 0) return;
        if (ev.altKey || ev.ctrlKey || ev.metaKey || ev.shiftKey) return;

        const link = ev.target.closest?.("a[href]");
        if (!link || link.hasAttribute("download") || link.hasAttribute("data-reload")) return;
        if (link.target && link.target !== "_self") return;
        if (link.origin !== location.origin) return;
        if (link.hash && link.pathname === location.pathname) return;

        ev.preventDefault();
        navigate(link.href, true);
    });

    window.addEventListener("popstate", () => navigate(location.href, false));

    async function navigate(url, push) {
        announce("busy", { kind: "navigate", url });

        try {
            const response = await fetch(url, { headers: { "X-Exos-Navigate": "true" } });
            const next = new DOMParser().parseFromString(await response.text(), "text/html");

            morph(document.body, next.body);
            document.title = next.title;
            if (push) history.pushState(null, "", response.url || url);
            window.scrollTo(0, 0);
        } catch (error) {
            console.error("[exos] navigation failed, falling back to a load:", error);
            location.assign(url);
        } finally {
            announce("idle", { kind: "navigate", url });
        }
    }

    // -------------------------------------------------------------------------
    //                              LIVE FRAGMENTS
    // -------------------------------------------------------------------------

    // One stream per tab, opened the first time an <exos-live> appears and
    // kept for the life of the page. The tab tells the server which fragments
    // it currently has on screen, and gets back patches for those and nothing
    // else.
    //
    // The client never names a topic. It reads the id and token the server put
    // on the element and hands them straight back, which is also why there is
    // no authorization to do here: the token is the proof, and it could only
    // have come from being served the fragment.

    const CONNECTION = crypto.randomUUID();
    let source = null;
    let subscribed = "";
    let syncPending = false;

    function openStream() {
        if (source) return;

        source = new EventSource(`/_exos/live?connection=${CONNECTION}`);

        for (const step of ["navigate", "page", "patch", "remove", "signals"]) {
            source.addEventListener(step, (ev) => apply(step, ev.data));
        }

        // EventSource reconnects on its own, but the server forgets the
        // connection when the stream drops, so the subscription has to be
        // re-sent once it is back.
        source.addEventListener("open", () => {
            subscribed = "";
            syncSubscriptions();
        });
    }

    // Coalesced: a patch that replaces fifty rows should produce one request
    // rather than fifty.
    function scheduleSync() {
        if (syncPending) return;
        syncPending = true;

        queueMicrotask(() => {
            syncPending = false;
            syncSubscriptions();
        });
    }

    async function syncSubscriptions() {
        const live = [...document.querySelectorAll("exos-live[id][data-token]")];
        if (!live.length && !source) return;

        openStream();

        // A set, not a list. The server keeps these in a hash set, so neither
        // the order the fragments sit in the document nor the same fragment
        // appearing twice on the page changes what this connection watches, and
        // the comparison below has to agree. Canonicalising is what makes a
        // drag silent: reordering a row is a real DOM move, so the pairs come
        // back in a new order, and comparing them in document order posted an
        // identical subscription on every pointer move.
        const tokens = new Map(live.map((el) => [el.id, el.dataset.token]));
        const topics = [...tokens.keys()].sort().map((id) => [id, tokens.get(id)]);

        // The visible set usually survives a patch unchanged, and re-sending
        // it would be pure chatter.
        const encoded = JSON.stringify(topics);
        if (encoded === subscribed) return;
        subscribed = encoded;

        try {
            const response = await fetch("/_exos/subscribe", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({ connection: CONNECTION, topics }),
            });

            // The server forgot us, so the stream is stale: drop it and let
            // EventSource open a fresh one.
            if (response.status === 410) {
                subscribed = "";
                source?.close();
                source = null;
                openStream();
            }
        } catch (error) {
            console.error("[exos] could not subscribe:", error);
            subscribed = "";
        }
    }

    document.addEventListener("exos:mutated", scheduleSync);

    // -------------------------------------------------------------------------
    //                              PUBLIC SURFACE
    // -------------------------------------------------------------------------

    window.exos = {
        // The global signal namespace. Reads and writes here resolve from the
        // document root, so they see what `data-signals-root` declared and
        // nothing an element holds. That is what an effect's `signals` step
        // writes, and why a model's fields are declared on the document.
        //
        // A plugin writing a signal a template will read must use `setIn`: the
        // template's expression resolves against its own element, and a name
        // declared on an enclosing scope is a different signal from the global
        // one that happens to share its spelling.
        signals: $,

        // Writes `name` in the scope `el` resolves it to, which is the lookup
        // an expression on `el` would do.
        setIn(el, name, value) {
            write(resolve(el, name), value);
        },

        // Reads `name` as `el` would see it.
        readIn(el, name) {
            return read(resolve(el, name));
        },

        applyPatch,
        bindTree,
        effect,
        dispose,
        evaluate,
        morph,
        navigate,
        unbindTree,

        // Registers an extra delegated event type, custom events included.
        listen(type) {
            if (DELEGATED.includes(type)) return;
            DELEGATED.push(type);

            document.addEventListener(type, (ev) => {
                const el = ev.target.closest?.(`[data-on-${type}]`);
                if (el) evaluate(el.getAttribute(`data-on-${type}`), el, ev, true);
            });
        },

        // Registers a binding attribute, applied to existing and future nodes.
        binding(name, make) {
            BINDINGS[name] = make;

            for (const el of document.querySelectorAll(`[${name}]`)) {
                unbind(el);
                bind(el);
            }
        },
    };

    bindTree(document.documentElement);

    // The observer only fires on changes, so a page that arrives with live
    // fragments already in it would never announce them and would receive
    // nothing until something else happened to mutate the DOM.
    syncSubscriptions();
})();
