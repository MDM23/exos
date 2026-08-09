// exos/sortable: drag to reorder.
//
// This file is what the plugin layer is for. Sortable drag and drop is the
// canonical interaction that must not involve the server: it runs at pointer
// rate, it is stateful across dozens of events, and a round trip anywhere in
// the loop makes it feel broken. So it runs entirely in the browser and talks
// to the server exactly once, when the user lets go.
//
//   <ul data-sortable="post('/reorder', { order: $._order })">
//     <li data-sort-item="3">... <span data-drag-handle>drag</span></li>
//   </ul>
//
// On drop, `$._order` holds the item ids in their new order and the expression
// runs. Reordering during the drag is real DOM movement, so the runtime's
// morph sees the same order the user does.

(() => {
    "use strict";

    // Pixels of travel before a press becomes a drag, so clicks survive.
    const THRESHOLD = 4;

    let drag = null;

    document.addEventListener("pointerdown", (ev) => {
        if (ev.button !== 0) return;

        const item = ev.target.closest?.("[data-sort-item]");
        if (!item) return;

        const container = item.closest("[data-sortable]");
        if (!container) return;

        // When the author marked a handle, only the handle starts a drag.
        const handle = item.querySelector("[data-drag-handle]");
        if (handle && !handle.contains(ev.target)) return;

        drag = {
            item,
            container,
            pointerId: ev.pointerId,
            startX: ev.clientX,
            startY: ev.clientY,
            pointerY: ev.clientY,
            offsetY: 0,
            started: false,
            initialOrder: order(container),
        };
    });

    document.addEventListener("pointermove", (ev) => {
        if (!drag || ev.pointerId !== drag.pointerId) return;

        if (!drag.started) {
            const travelled = Math.hypot(ev.clientX - drag.startX, ev.clientY - drag.startY);
            if (travelled < THRESHOLD) return;
            begin(ev);
        }

        drag.pointerY = ev.clientY;
        track();
        reorder(ev.clientY);
    });

    document.addEventListener("pointerup", finish);
    document.addEventListener("pointercancel", finish);

    function begin(ev) {
        drag.started = true;
        drag.item.setPointerCapture?.(drag.pointerId);
        drag.item.classList.add("is-dragging");
        drag.item.style.willChange = "transform";
        document.body.style.userSelect = "none";

        // Suppress the click that would otherwise follow the drag. A drag does
        // not always produce one, and a `once` listener that never fires stays
        // attached, so it would eat the next real click on this row. That is
        // how a checkbox in a sortable list stops responding.
        const item = drag.item;
        item.addEventListener("click", swallow, { capture: true, once: true });
        setTimeout(() => item.removeEventListener("click", swallow, { capture: true }), 0);

        ev.preventDefault();
    }

    function swallow(ev) {
        ev.stopPropagation();
        ev.preventDefault();
    }

    /** Re-applies the transform that keeps the item under the pointer. */
    function track() {
        drag.offsetY = drag.pointerY - drag.startY;
        drag.item.style.transform = `translateY(${drag.offsetY}px)`;
    }

    /** Where layout put the item, ignoring its transform. */
    function layoutTop() {
        return drag.item.getBoundingClientRect().top - drag.offsetY;
    }

    // Moves the dragged item past whichever sibling the pointer has crossed
    // the midpoint of. The item carries a transform, so its own box is offset
    // from where layout put it: measure siblings, never the item itself.
    function reorder(pointerY) {
        const siblings = [...drag.container.querySelectorAll("[data-sort-item]")].filter(
            (el) => el !== drag.item,
        );

        for (const sibling of siblings) {
            const box = sibling.getBoundingClientRect();
            const middle = box.top + box.height / 2;
            const itemIsAfter =
                sibling.compareDocumentPosition(drag.item) & Node.DOCUMENT_POSITION_FOLLOWING;

            if (itemIsAfter && pointerY < middle) {
                move(() => sibling.before(drag.item));
                return;
            }

            if (!itemIsAfter && pointerY > middle) {
                move(() => sibling.after(drag.item));
                return;
            }
        }
    }

    // Moving the item in the DOM changes where layout puts it, which would
    // teleport it by that distance since the transform is relative. Shifting
    // the drag origin by the same amount holds the visual position fixed, so
    // the item stays under the cursor and only its neighbours appear to move.
    //
    //   visual = layoutTop + offsetY,  offsetY = pointerY - startY
    //
    // so holding `visual` fixed across a layout change of d means startY += d.
    function move(place) {
        const before = layoutTop();
        place();
        const after = layoutTop();

        drag.startY += after - before;
        track();
    }

    function finish(ev) {
        if (!drag || (ev && ev.pointerId !== drag.pointerId)) return;

        const { item, container, started, initialOrder } = drag;
        drag = null;

        if (!started) return;

        item.classList.remove("is-dragging");
        item.style.transform = "";
        item.style.willChange = "";
        document.body.style.userSelect = "";

        const next = order(container);
        if (same(next, initialOrder)) return;

        // The server hears about this once, with the final order.
        //
        // Written through the container's scope rather than the global one.
        // The expression is evaluated against this element, so `$._order`
        // resolves to whatever scope encloses it; writing globally would leave
        // the expression reading a different signal that merely shares the
        // name, and posting null.
        window.exos.setIn(container, "_order", next);

        const expression = container.getAttribute("data-sortable");
        if (expression) window.exos.evaluate(expression, container, null, true);

        container.dispatchEvent(
            new CustomEvent("exos:reordered", { detail: { order: next }, bubbles: true }),
        );
    }

    function order(container) {
        return [...container.querySelectorAll("[data-sort-item]")].map((el) =>
            el.getAttribute("data-sort-item"),
        );
    }

    function same(left, right) {
        return left.length === right.length && left.every((value, index) => value === right[index]);
    }
})();
