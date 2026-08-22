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
    //                                 THE BASE
    // -------------------------------------------------------------------------

    // Where this application's URLs start, worked out rather than configured.
    //
    // An application served under a prefix has to agree with its server about
    // what that prefix is, and the usual way to do that is a second place to
    // write it down: a meta tag, an attribute on <html>, a build-time constant.
    // Every one of those can be forgotten or can drift.
    //
    // There is no need. This file is itself an asset, served under the same
    // prefix as the endpoints below, so the URL it was loaded from already
    // carries the answer and everything before `/_exos/` in it is the base. It
    // cannot be out of step with the server, because a wrong URL would not have
    // loaded this script at all.

    const SCRIPT =
        document.currentScript?.src ??
        document.querySelector('script[src*="/_exos/"]')?.src ??
        "";

    const BASE = (() => {
        if (!SCRIPT) return "";

        // Last rather than first, so a base that happens to contain the segment
        // is still read as a base.
        const { pathname } = new URL(SCRIPT, location.href);
        const cut = pathname.lastIndexOf("/_exos/");

        return cut === -1 ? "" : pathname.slice(0, cut);
    })();

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

    // An element that declares signals opens a scope of its own. A row already
    // needs an id for morphing to key on, so a hundred rows each holding a
    // `fav` signal need no unique names invented for them: inside `#file-3`,
    // `$.fav` is `file-3-7/fav`, where the id is there to be read and the
    // counter is what makes the scope that element's.
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

        // The id is in the name because it makes the store readable in a
        // debugger; the counter is what makes the name this element's alone.
        //
        // Two elements can carry one id over a page's life, and the scope has
        // to survive that. A morph that replaces a subtree rather than
        // updating it builds the new one before tearing the old one down, so
        // there is a moment when both are here, and a fragment rendered twice
        // is two elements with one id for as long as the page lasts. Sharing a
        // scope name means the departing element's cleanup deletes the
        // arriving element's signals: the row is still on screen, still
        // wired, and every write from it goes somewhere nothing is watching.
        //
        // A scope is per element rather than per id, so the element that
        // survives a morph keeps its state through the WeakMap and the element
        // that replaces one starts from what its markup declares.
        scope = `${el.id || "exos"}-${++scopeCounter}`;
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

    // The nearest element that declares anything, without making one the way
    // `scopeOf` does. `debounce` keys off this so that a helper called once per
    // row gives every row its own timer, exactly as `resolve` gives every row
    // its own signal.
    function declaring(el) {
        for (let node = el; node; node = node.parentElement) {
            const scope = scopes.get(node);
            if (scope) return scope;
        }

        return "";
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
                "attr", "append", "focus", "debounce",
                "rows", "addRow", "dropRow", "rowError",
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

    // -------------------------------------------------------------------------
    //                                    ROWS
    // -------------------------------------------------------------------------

    // A repeating group is a <template> holding one row and however many rows
    // are on screen beside it. Adding one is a clone, and a clone is its own
    // signal scope, so nothing has to name a row: no id, no round trip, and no
    // list for the server to keep until the form is sent.

    function group(key) {
        return document.querySelector(`[data-rows="${key}"]`);
    }

    function rowsIn(el) {
        return el ? [...el.children].filter((child) => child.hasAttribute("data-row")) : [];
    }

    // The row `el` sits in, within the named group. Walked rather than
    // `closest`, so a group inside a row answers for the group that was asked
    // for rather than for whichever row is nearest.
    function rowOf(el, key) {
        const into = group(key);

        for (let node = el; node; node = node.parentElement) {
            if (node.parentElement === into && node.hasAttribute("data-row")) return node;
        }

        return null;
    }

    // Where a row sits, which is the whole of its name. A message about the
    // third row is written under the third row, because every row of one group
    // carries the same field names and only position tells them apart.
    function rowIndex(el, key) {
        const row = rowOf(el, key);
        return row ? rowsIn(row.parentElement).indexOf(row) : -1;
    }

    // The body's rows, read out of the group in the order they are shown. Not
    // a loop over data that renders: the values are in the DOM already and
    // this walks them once, when the request is built.
    function collectRows(el, key, fields) {
        return rowsIn(group(key)).map((row) =>
            Object.fromEntries(fields.map((field) => [field, read(resolve(row, field))])),
        );
    }

    function addRow(key) {
        const into = group(key);
        const template = into?.querySelector("template");
        if (!template) return;

        const clone = template.content.cloneNode(true);

        // An id only so the store reads well in a debugger. The scope is the
        // element's either way, which is what makes the clone's fields its own.
        for (const el of clone.children) {
            if (!el.id) el.id = `exos-row-${++appended}`;
        }

        into.appendChild(clone);
    }

    function dropRow(el, key) {
        rowOf(el, key)?.remove();
    }

    function rowError(record, key, field, el) {
        return (record ?? {})[`${key}.${rowIndex(el, key)}.${field}`] ?? "";
    }

    // Where a control's message lives in its model's record. A field of a row
    // is keyed by its group and its position, which is the same key the server
    // writes and the same one `rowError` reads, so the two sides cannot pick
    // different slots.
    function recordKey(el, name) {
        const group = el.getAttribute("data-bind-rows");
        return group ? `${group}.${rowIndex(el, group)}.${name}` : name;
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

    // Focus, after the effects this turn scheduled have run. A handler that
    // reveals a field and focuses it is the ordinary case, and at the moment
    // it asks, the field is still hidden: bindings flush on a microtask, and
    // an element that is not displayed cannot take focus. Queueing after them
    // is the difference between the caret landing in the field and nothing
    // happening at all.
    function focusLater(selector) {
        queueMicrotask(() => document.querySelector(selector)?.focus());
    }

    // A handler on `input` runs per keystroke, which is right for a signal
    // write and wrong for anything that leaves the machine. `timers` holds the
    // pending call per key; `issued` counts what has gone out under one, so a
    // reply older than the newest can be dropped.
    const timers = new Map();
    const issued = new Map();
    let arming = null;

    function debounceCall(el, key, delay, body) {
        const scoped = `${declaring(el)}/${key}`;

        clearTimeout(timers.get(scoped));
        timers.set(
            scoped,
            setTimeout(() => {
                timers.delete(scoped);

                // `request` reads this synchronously, before its first await,
                // so the key reaches the fetch without being threaded through
                // every helper an expression might call in between.
                arming = scoped;
                try {
                    body();
                } finally {
                    arming = null;
                }
            }, delay),
        );
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
            focusLater,
            (key, delay, body) => debounceCall(el, key, delay, body),
            (from, key, fields) => collectRows(from, key, fields),
            addRow,
            dropRow,
            rowError,
        );
    }

    // Whether a field has been edited, kept in the store like everything else
    // an effect reads.
    //
    // A set beside the store would have been smaller and would go stale: an
    // effect subscribes to what it reads, so dirtiness has to be readable the
    // same way or the rules gated on it never run again. The prefix is one no
    // generated name can wear, and nothing declares or sends these: what
    // somebody has typed so far is the client's alone.
    const dirtyKey = (key) => `~dirty/${key}`;

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

        // data-bind-rules="<expr>" is the field's own rules, answered here and
        // written into the record its model's messages live in. One slot per
        // field, whichever side decided what is in it, which is what lets a
        // template read one place and lets this mark the control below.
        //
        // Nothing is written until the field has been edited. A form that is
        // red before it is read is worse than no validation, and without the
        // guard a patch re-inserting a control would wipe the message that
        // arrived with it.
        "data-bind-rules": (el, source) => () => {
            const name = el.getAttribute("data-bind");
            const state = el.getAttribute("data-bind-state");
            if (!name || !state) return;

            const key = resolve(el, name);
            if (read(dirtyKey(key)) !== true) return;

            const said = String(evaluate(source, el, null, false) ?? "");
            const slot = resolve(el, state);
            const record = read(slot) ?? {};
            const at = recordKey(el, name);

            if ((record[at] ?? "") === said) return;

            // A fresh object rather than a write in place, for the reason a
            // collection is reassigned rather than pushed into.
            if (said) write(slot, { ...record, [at]: said });
            else {
                const { [at]: _gone, ...rest } = record;
                write(slot, rest);
            }
        },

        // The control says what is wrong with it, in the attribute the
        // language already had for it. A stylesheet needs no class of ours and
        // a screen reader is told what the border says.
        // Spelled out rather than toggled: aria-invalid is a token attribute
        // whose empty value means `false`, so an attribute that is merely
        // present says the opposite of what it is here to say.
        "data-bind-state": (el, state) => () => {
            const name = el.getAttribute("data-bind");
            const record = read(resolve(el, state)) ?? {};

            if (record[recordKey(el, name)]) el.setAttribute("aria-invalid", "true");
            else el.removeAttribute("aria-invalid");
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
            // Compared first, because a morph re-runs this and replacing a
            // text node with an identical one drops a selection that was
            // sitting in it.
            const value = String(evaluate(source, el, null, false) ?? "");
            if (el.textContent !== value) el.textContent = value;
        },
    };

    const BINDING_SELECTOR = Object.keys(BINDINGS)
        .concat("data-signals", "data-signals-root")
        .map((name) => `[${name}]`)
        .join(",");

    // Declares, and never overwrites, so a morph that re-delivers the same
    // markup does not reset live state, and a name a second row declares is
    // the one the first row already put there.
    function declare(el, attribute, key, force) {
        const declared = el.getAttribute(attribute);
        if (!declared) return;

        try {
            for (const [name, value] of Object.entries(JSON.parse(declared))) {
                const slot = key(name);
                if (force || !store.has(slot)) write(slot, value);
            }
        } catch (error) {
            console.error(`[exos] bad ${attribute}:`, declared, error);
        }
    }

    // The one thing that does overwrite, and the reason is what a navigation
    // is: a different page, saying what its own signals start as. A document
    // signal outlives the element that declared it, so without this the page
    // that arrives inherits whatever the last one was left holding, and a
    // model reused with a second meaning opens on the first one's value. Only
    // the names the new document actually declares are touched, so anything it
    // does not mention keeps what it has.
    function reseed() {
        for (const el of document.querySelectorAll("[data-signals-root]")) {
            declare(el, "data-signals-root", (name) => name, true);
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

            const name = el.getAttribute("data-bind");
            const key = resolve(el, name);
            const current = read(key);
            const kind = el.getAttribute("data-bind-kind");

            // Edited, so this control's own rules may speak. What they answer
            // replaces whatever the server last said, because a verdict on a
            // value that is no longer there is worse than none.
            write(dirtyKey(key), true);

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
        // Claimed synchronously, before anything awaits, so a call made under a
        // debounce key is stamped with the turn it went out on.
        const key = arming;
        const turn = key ? (issued.get(key) ?? 0) + 1 : 0;
        if (key) issued.set(key, turn);

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

            // A newer call went out under this key while this one was in
            // flight, so its answer is the one that counts. Debouncing alone
            // does not give this: two requests can still overlap on a slow
            // connection, and the older landing last paints the answer for a
            // prefix of what is in the box now.
            if (key && issued.get(key) !== turn) {
                response.body?.cancel();
                return response;
            }

            const type = response.headers?.get("content-type") ?? "";

            // An effect is applied whatever the status, and everything else is
            // applied only on success.
            //
            // A refusal is a thing the server has something to say about: which
            // field was wrong, which signal to put back, where to move the
            // caret. All of that is an Effect already, and answering 422 is how
            // a validation failure says it is one. Reading the body only on 2xx
            // meant the server could either be honest about the status or be
            // heard, never both, so the whole category came back as a console
            // line and a page that did not change.
            //
            // An error page is the other half of the rule and stays out: HTML
            // on a failure is a document about the failure, and morphing one
            // into the page would be a 500 eating the screen.
            if (type.includes("text/event-stream")) {
                await consume(response);
                return response;
            }

            // Nothing to apply, so this is the silent failure the rule above
            // is about, and it is announced rather than swallowed.
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

            if (type.includes("text/html")) applyPatch(await response.text());

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
                // The same deferral a handler's focus gets, so a field this
                // burst patched in and a binding is about to unhide is
                // focusable by the time this runs.
                focusLater(payload);
                break;

            case "navigate":
                navigate(payload, true);
                break;

            case "page": {
                const next = new DOMParser().parseFromString(payload, "text/html");
                morph(document.body, next.body);
                if (next.title) document.title = next.title;
                // A page handed over by an action is a navigation that saved a
                // fetch, so it starts the same way one does.
                reseed();
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

            case "title":
                document.title = payload;
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
        reapply(from);
    }

    // A binding owns what it writes, and the markup that arrives does not know
    // that. The server renders class="todo" over a row a data-class binding
    // has put "editing" on, syncAttributes takes the incoming word for it, and
    // the class is gone while the signal still says true. Nothing re-applies
    // it, because writing true over true is not a change, so the row is stuck
    // out of edit mode until something else happens to flip the signal, and
    // double-clicking it again does nothing at all.
    //
    // Re-running the element's effects after a morph is what makes the binding
    // the authority again, for everything one of them owns: class, hidden,
    // text, attributes and properties alike. A speculative write from attr_now
    // is deliberately not in that set. It has no second copy to disagree with
    // the patch, which is the whole reason it is not a signal.
    function reapply(el) {
        const effects = bound.get(el);
        if (!effects) return;

        for (const effect of effects) run(effect);
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
            reseed();
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
    //
    // It does not name the connection either. The server mints that and says it
    // in the stream's first event, because an id the client picks is an id
    // another client can guess, and naming a connection is what replaces the
    // topics it watches.

    // Whether this is a dev build, which the server says by adding `?dev` to
    // the runtime's own URL.
    //
    // There is no `cfg!` to read here and the file is the same bytes either
    // way, so the answer has to come from the server, and the script URL is the
    // channel already there for the base. It changes two things and nothing
    // else: a dev tab keeps a stream open on a page with nothing live on it, so
    // that it notices its server being rebuilt, and it answers a reconnect with
    // a reload rather than a repair.
    const DEV = new URL(SCRIPT, location.href).searchParams.has("dev");

    let connection = null;
    let greeted = false;
    let source = null;
    let subscribed = "";
    let syncPending = false;

    function openStream() {
        if (source) return;

        source = new EventSource(`${BASE}/_exos/live`);

        // Every step there is, and not the five a patch burst happens to use.
        // The stream and a handler's reply carry the same `Effect` through the
        // same parser, so a runtime that delivered half the vocabulary one way
        // round would make where an effect came from something an application
        // had to learn rather than read. A focus or a scroll from a background
        // job is rude, but it is rude in the way whoever sent it chose, and
        // answering that choice with silence is the worse surprise.
        for (const step of [
            "focus", "navigate", "page", "patch",
            "reload", "remove", "scroll", "signals", "title",
        ]) {
            source.addEventListener(step, (ev) => apply(step, ev.data));
        }

        // Both the introduction and the cue to subscribe. EventSource
        // reconnects on its own and the server forgets the connection when the
        // stream drops, so a reconnect is a new connection with a new id and
        // the subscription has to be re-sent under it. That makes this the only
        // place a subscription can start from: `open` fires before the greeting
        // arrives, when there is still nothing to subscribe with.
        source.addEventListener("connection", (ev) => {
            connection = ev.data;
            subscribed = "";
            syncSubscriptions();

            // A second name means this tab was dropped and the server forgot
            // it, so something may have been published into the gap. The first
            // name has no gap behind it: the document arrived a moment ago.
            //
            // In a dev build the gap is a rebuild, and repairing one is the
            // wrong shape twice over: it morphs the body, where a rebuild
            // changes the head, and it keeps the stylesheet and the runtime the
            // previous build hashed. The whole document has to come back.
            if (greeted) {
                if (DEV) location.reload();
                else repair();
            }

            greeted = true;
        });
    }

    // What a reconnect lost, fetched back.
    //
    // EventSource reconnects on its own, and the server forgets a connection
    // when its stream drops, so anything published in between reached nobody.
    // A fragment that changed during the gap stays wrong until the next publish
    // of it, which may never come.
    //
    // The server cannot repair that alone. A topic is a hash of a name and its
    // arguments, and nothing can re-invoke the function from it. The client can,
    // because the page it is on renders every fragment it is showing, so one
    // fetch of the current URL brings all of them back at once.
    //
    // It morphs without re-seeding, which is the whole difference between this
    // and a navigation. A navigation is a different page saying what its signals
    // start as; a repair is the same page arriving again, and re-seeding it
    // would empty the field somebody is typing into every time their connection
    // hiccuped. Nothing is announced either, because a repair is not a round
    // trip anybody waited for.
    async function repair() {
        const url = location.href;

        try {
            const response = await fetch(url, { headers: { "X-Exos-Navigate": "true" } });

            // Only a page repairs a page. A 404 or an error page carries no
            // fragments to restore, and morphing one in would turn a hiccup
            // into a lost document.
            if (!response.ok) return;

            const next = new DOMParser().parseFromString(await response.text(), "text/html");

            // The tab may have gone somewhere else while this was in flight. A
            // navigation fetches the page it lands on, so it has already
            // repaired whatever the gap cost; laying the page it left over the
            // page it is on would be the worse bug of the two.
            if (location.href !== url) return;

            morph(document.body, next.body);
            if (next.title) document.title = next.title;
        } catch (error) {
            // Quietly, and without the fallback to a full load that a
            // navigation makes. The stream has only just come back and the
            // network may still be unsteady; a stale fragment until the next
            // reconnect is the smaller failure.
            console.error("[exos] could not repair after a reconnect:", error);
        }
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

        // A dev build opens one whatever the page holds, because the stream
        // going away is how it learns the server was rebuilt, and a page under
        // development is exactly the page with nothing live on it yet.
        if (!live.length && !source && !DEV) return;

        openStream();

        // The id arrives on the stream, so a mutation landing before it has
        // nothing to subscribe with. The greeting syncs when it does, and it
        // reads the DOM again rather than replaying whatever was pending here.
        if (!connection) return;

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
            const response = await fetch(`${BASE}/_exos/subscribe`, {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({ connection, topics }),
            });

            // The server forgot us, so the stream is stale: drop it and let
            // EventSource open a fresh one, which arrives with an id of its own.
            if (response.status === 410) {
                connection = null;
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
        // Where this application's URLs start, for a plugin that has to build
        // one. Empty at the root, and never with a trailing slash.
        base: BASE,

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
