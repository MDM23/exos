//! Finding the routes and assets linked into the binary.
//!
//! A route attribute is the whole registration. The function says where it
//! lives and [`app`] finds it, so there is no second list to keep in sync and
//! no way to add a handler and forget to mount it.
//!
//! ```ignore
//! #[exos::get("/files")]
//! async fn files() -> Page { /* ... */ }
//!
//! axum::serve(listener, exos::app()).await?;
//! ```
//!
//! Registration goes through [`inventory`], which collects entries at link
//! time. That has one consequence worth knowing: routes in a crate that
//! nothing links do not exist. In a binary this never comes up, but if routes
//! are split into a library, the binary has to depend on it.

use std::collections::HashMap;

use axum::{Router, routing::MethodRouter};

use crate::AssetSet;

/// One discovered route, submitted by the method attributes.
///
/// Built by `#[exos::get]` and friends. There is no reason to name this type
/// yourself.
#[derive(Clone, Copy)]
pub struct RouteEntry {
    path: &'static str,
    build: fn() -> MethodRouter,
}

impl RouteEntry {
    /// Describes a route for [`app`] to mount.
    ///
    /// `build` returns the handler already wrapped in its method: a function
    /// pointer rather than a value, because an axum handler is generic and
    /// this is the shape that erases those generics without boxing.
    pub const fn new(path: &'static str, build: fn() -> MethodRouter) -> Self {
        Self { path, build }
    }

    /// The path this route mounts at, in axum's syntax.
    pub const fn path(&self) -> &'static str {
        self.path
    }
}

impl core::fmt::Debug for RouteEntry {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("RouteEntry")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

inventory::collect!(RouteEntry);

/// One discovered asset set, submitted by [`asset!`](crate::asset).
#[derive(Clone, Copy, Debug)]
pub struct AssetSetEntry(AssetSet);

impl AssetSetEntry {
    /// Registers a set of assets for [`app`] to serve.
    pub const fn new(set: AssetSet) -> Self {
        Self(set)
    }
}

inventory::collect!(AssetSetEntry);

/// The application's guard, submitted by [`guard`](macro@crate::guard).
///
/// It carries the wrapping rather than the middleware, because
/// [`from_fn`](axum::middleware::from_fn) returns a type naming the function it
/// was given. A function pointer taking the router and handing it back erases
/// that without boxing, which is the trick [`RouteEntry`] plays with a method
/// router.
#[derive(Clone, Copy, Debug)]
pub struct GuardEntry(fn(Router) -> Router);

impl GuardEntry {
    /// Registers a guard for [`app`] to mount.
    pub const fn new(wrap: fn(Router) -> Router) -> Self {
        Self(wrap)
    }
}

inventory::collect!(GuardEntry);

/// The guard the binary declared, if it declared one.
///
/// # Panics
///
/// If it declared two. One would have to wrap the other, and inventory has no
/// order to decide which with.
fn guard() -> Option<fn(Router) -> Router> {
    let mut declared = inventory::iter::<GuardEntry>.into_iter();
    let first = declared.next()?;

    assert!(
        declared.next().is_none(),
        "two guards; one would have to wrap the other and link order does not \
         decide which, so write the second one's work into the first"
    );

    Some(first.0)
}

/// Every asset the binary embedded, the client runtime included.
///
/// The runtime is not a special case: [`runtime`](crate::runtime) expands the
/// same macro every application does, and registers the same way.
pub(crate) fn asset_sets() -> Vec<AssetSet> {
    inventory::iter::<AssetSetEntry>
        .into_iter()
        .map(|set| set.0)
        .collect()
}

/// Builds the application: every discovered route, every asset, the runtime.
///
/// Several routes may share a path, which is how `GET` and `POST` on the same
/// URL are written, and their method routers are merged.
///
/// # Before there is anything to serve
///
/// A dev build that discovered no routes at all mounts a
/// [welcome](../../src/welcome.rs) page as its fallback, since a binary with
/// none is somebody's first run and a bare 404 tells them nothing about which
/// part of it went wrong. A release build never does: an application that lost
/// its routes in production should fail like one. So can a test binary that
/// declares none, which is what [`tests/welcome.rs`](../../tests/welcome.rs)
/// relies on.
///
/// # The guard goes inside
///
/// A [`guard`](macro@crate::guard) is mounted around the discovered routes and
/// inside exos's own layers, so it reads the session and the request scope the
/// way a handler does. Anything layered onto the router this returns sits
/// outside all of that, which is the right place for what needs neither.
///
/// # Panics
///
/// If two handlers claim the same method on the same path, or if two guards
/// were declared. Finding either out at startup beats finding out from
/// whichever one happened to win.
pub fn app() -> Router {
    let mut by_path: HashMap<&'static str, MethodRouter> = HashMap::new();

    for entry in inventory::iter::<RouteEntry> {
        let router = (entry.build)();

        let merged = match by_path.remove(entry.path) {
            Some(existing) => existing.merge(router),
            None => router,
        };

        by_path.insert(entry.path, merged);
    }

    let first_run = by_path.is_empty() && cfg!(debug_assertions);

    let router = by_path
        .into_iter()
        .fold(Router::new(), |router, (path, methods)| {
            router.route(path, methods)
        });

    // Before the merges below, which carry axum's default fallback: two real
    // ones would be a conflict, and a default one loses to this.
    let router = if first_run {
        router.fallback(crate::welcome::page)
    } else {
        router
    };

    // Inside every layer below and around the application's own routes alone.
    // A guard reads the session and writes the scope, which is what mounting it
    // outside `app()` cannot do, and exos's own endpoints are not pages to
    // redirect: a stream and a field check answer a runtime rather than a
    // browser that could follow one.
    let router = match guard() {
        Some(wrap) => wrap(router),
        None => router,
    };

    router
        .merge(crate::asset_routes(asset_sets()))
        .merge(crate::live::routes())
        .merge(crate::valid::routes())
        // Inside the scope, which it reads, and outside everything else: the
        // stream needs the name as much as a handler does, and an asset request
        // that carries the cookie costs a header lookup and nothing more.
        .layer(axum::middleware::from_fn(crate::session::layer))
        // Beside the session and for the same reason: what the browser asked
        // for has to be readable from a view, which is not a handler and can
        // extract nothing, and the response has to say whether it was read.
        .layer(axum::middleware::from_fn(crate::locale::layer))
        // Last, so that it wraps the merges above rather than only the routes
        // discovered before it: the stream and the assets are requests too.
        .layer(axum::middleware::from_fn(crate::scope::layer))
}
