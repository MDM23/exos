// The exos client runtime, and the plugins this build ships.
//
// One request: the runtime and its plugins are concatenated in the order these
// imports give, which is also the order they depend on each other in. The
// sortable plugin reaches for `window.exos`, so it has to run second, and this
// is where that is said.
import "./runtime.js";
import "./sortable.js";
