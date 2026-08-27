//! Finding the routes and assets linked into the binary.
//!
//! A route attribute is the whole registration. The function says where it
//! lives and [`app`](crate::app) finds it, so there is no second list to keep
//! in sync and no way to add a handler and forget to mount it.
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
    /// Describes a route for [`app`](crate::app) to mount.
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
    /// Registers a set of assets for [`app`](crate::app) to serve.
    pub const fn new(set: AssetSet) -> Self {
        Self(set)
    }
}

inventory::collect!(AssetSetEntry);

/// Every route the binary declared, and whether it declared none at all.
///
/// Several routes may share a path, which is how `GET` and `POST` on the same
/// URL are written, and their method routers are merged.
///
/// # Panics
///
/// If two handlers claim the same method on the same path.
pub(crate) fn routes() -> (Router, bool) {
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

    let routes = by_path
        .into_iter()
        .fold(Router::new(), |router, (path, methods)| {
            router.route(path, methods)
        });

    (routes, first_run)
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
