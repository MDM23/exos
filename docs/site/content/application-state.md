# Application state

Provided once, reachable by type. No `State<T>` threaded through signatures:

```rust
exos::provide(Files::seed());

let files = exos::data::<Files>();          // panics if missing
let maybe = exos::try_data::<Files>();      // Option<Arc<Files>>
```

A missing value is a wiring mistake made once at startup, not a per-request
condition, so `data` panicking and naming the type is the right default.

The type is the key, so wrap distinct things in distinct newtypes.

## Per request

`data` lives for the process. What belongs to the request being served goes in
the request scope, which is the same idea with a shorter lifetime:

```rust
let scope = exos::scope();

scope.set(Principal { id: 7 });
let who = scope.get::<Principal>();     // Option<Arc<Principal>>
```

It is a task-local set by a layer rather than an extractor, for one specific
reason: a `view!` fragment is a plain function, not a handler, so it cannot
extract anything, and making it able to would mean threading a parameter
through every template.

Two rules are enforced rather than documented:

- Outside a request there is no scope, and `exos::scope()` panics rather than
  answering `None`. A background job reading the request is a mistake made
  once, not a case every caller handles, and `None` would quietly render the
  logged-out view of something and then publish it.
- A live fragment never sees it, so `exos::scope()` panics inside one whether
  or not a request is being served. A fragment's arguments are its whole input.

Tests use `exos::with_scope`, which runs a closure in a scope of its own. Being
per task rather than process-global, two tests can hold different scopes at
once, which `provide` cannot do.
