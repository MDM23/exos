// playlist/sleeve: hold the blur until the sleeve is really there.
//
// The placeholder is already in the page, as a `data:` URL the server rendered
// into a custom property, so nothing here decodes anything. What is left is the
// one thing markup cannot say: an image paints the rows it has as they arrive,
// so a slow sleeve wipes down over its own blur. The sleeve is therefore
// transparent until it is complete, and this is what says when that is.
//
// Nothing here defends the flag against a patch, and nothing should: the image
// carries `data-preserve`, so the morph leaves it alone entirely. An earlier
// version of this file re-read the state on every `exos:mutated` instead, and
// that is a repair that cannot work. The runtime's observer watches for nodes
// arriving and leaving, so a patch that only changes attributes, which is
// exactly what a drag publishes once the sortable plugin has already moved the
// rows, fires nothing at all. The blur came back and stayed until the track
// changed.
//
// The sweep below is for the other thing that event does say, which is that new
// images may have arrived. One that was already in the cache can be complete
// before anything here could have heard it load.

(() => {
    "use strict";

    // `load` does not bubble, so delegating it means listening in the capture
    // phase. The alternative is a listener per image, which is the thing this
    // framework does not do.
    document.addEventListener("load", (event) => show(event.target), true);

    document.addEventListener("exos:mutated", () => {
        for (const el of document.querySelectorAll("img[data-fade]")) show(el);
    });

    function show(el) {
        if (el.matches?.("img[data-fade]") && el.complete) el.dataset.fade = "in";
    }
})();
