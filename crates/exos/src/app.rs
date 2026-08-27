//! The application, and everything exos puts up around it.
//!
//! [`app`] hands back every route the binary [declared](crate::discover),
//! before exos has put anything around them. What is added here is added
//! *inside* exos's own layers, because those layers go on when the application
//! is first served: laying them down last is what leaves room underneath them.
//!
//! ```ignore
//! axum::serve(listener, exos::app().route_layer(from_fn(guard))).await?;
//! ```
//!
//! It is also where an application says the things it says once. A key, a
//! resolver, a base: each of them is a decision the whole process shares, and
//! each is written where the application is built rather than in a `boot`
//! function somebody has to remember to call first.
//!
//! # Building it more than once
//!
//! Saying the same thing again says nothing new, so [`app`] can be called
//! wherever an application is wanted: a test that serves one request per test
//! builds one application per test and every one of them is the same
//! application. Two *different* answers are still the panic they always were,
//! since that is two parts of a program disagreeing about a decision the
//! process makes once.

use core::{
    convert::Infallible,
    future::{Future, Ready, ready},
    panic::Location,
    task::{Context, Poll},
};

use axum::{
    BoxError, Router,
    body::{Bytes, HttpBody},
    extract::Request,
    response::{IntoResponse, Response},
    routing::Route,
    routing::future::RouteFuture,
    serve::{IncomingStream, Listener},
};
use tower::{Layer, Service};

use crate::{Frame, Id, Keys, Resolution, Sent, Violation, discover};

/// Every route the binary declared, and whatever else is put on it.
///
/// What goes on here goes on *inside* the layers exos puts up, so a middleware
/// added with [`route_layer`](Self::route_layer) reads
/// [`session`](crate::session), [`locale`](crate::locale) and
/// [`scope`](crate::scope) the way a handler does, and writes a scope the views
/// under it can read. It also wraps the application's own routes and nothing
/// else: exos's endpoints are merged around it later, since a stream and a
/// field check answer the client runtime rather than a browser that could
/// follow a redirect.
///
/// ```ignore
/// axum::serve(
///     listener,
///     exos::app()
///         .keys(Keys::from_secret(std::env::var("EXOS_SECRET")?))
///         .provide(Sessions::new(pool))
///         .route_layer(axum::middleware::from_fn(guard)),
/// )
/// .await?;
/// ```
///
/// Those layers and those endpoints go on when the application is first served.
/// Anything that needs neither the session nor the scope can go outside them
/// instead, on the `Router` this converts into.
#[must_use = "an application that is never served serves nothing"]
#[derive(Debug)]
pub struct App {
    routes: Router,
    serving: Option<Router>,
}

impl App {
    /// Says where this application is mounted, rather than letting exos find
    /// out.
    ///
    /// Only needed where the path is rewritten by something that is not an axum
    /// `Router`, since that is the one case exos cannot work out for itself: it
    /// is the outermost `Router` that records the arriving URI. A reverse proxy
    /// serving this application at `/admin` while forwarding `/` to it is the
    /// ordinary example.
    ///
    /// ```no_run
    /// let app = exos::app().base("/admin");
    /// ```
    ///
    /// Nesting needs no call: `Router::nest("/admin", exos::app().into())` is
    /// found on its own.
    ///
    /// A trailing slash is dropped, so `/admin` and `/admin/` mean the same
    /// thing. `/` means the root and is the same as saying nothing.
    ///
    /// # Panics
    ///
    /// If `path` is not empty and does not start with `/`, because a relative
    /// base would resolve against whichever page happened to be open.
    ///
    /// If a different base is already in place. That means either two parts of
    /// the program disagree about where the application lives, or this arrived
    /// after the first request had already answered the question, and both are
    /// worth being loud about rather than resolving by whichever ran first.
    pub fn base(self, path: impl Into<String>) -> Self {
        crate::base::set(path);
        self
    }

    /// Says how a frame reaches the rest of the cluster.
    ///
    /// Until this is called, exos is a single node and builds no frames at all.
    ///
    /// ```ignore
    /// exos::app().keys(key).bus(move |frame| {
    ///     let redis = redis.clone();
    ///
    ///     async move {
    ///         redis.publish("exos", frame.to_bytes()).await?;
    ///         Ok(())
    ///     }
    /// })
    /// ```
    ///
    /// # Panics
    ///
    /// If no signing key is configured. A random key per process is right for
    /// `cargo run` and is a broken cluster: a token minted by one node verifies
    /// nowhere else, and the symptom is fragments that stop updating after a
    /// reconnect, which reads as a network glitch. Registering a bus is the
    /// moment exos can know that rather than warn about it, so
    /// [`keys`](Self::keys) goes first.
    ///
    /// And if called from outside a tokio runtime. A publish is synchronous and
    /// an adapter is not, so the handle to spawn on is taken here, where an
    /// application registering the bus from the wrong place finds out at
    /// startup rather than from a delivery that silently never left.
    pub fn bus<F, U>(self, cross: F) -> Self
    where
        F: Fn(Frame) -> U + Send + Sync + 'static,
        U: Future<Output = Sent> + Send + 'static,
    {
        crate::live::set_bus(cross);
        self
    }

    /// Says how this application words a refusal.
    ///
    /// The field arrives under the name it is declared with, which never leaves
    /// the server, so an application can answer per field where the general
    /// sentence is not good enough:
    ///
    /// ```no_run
    /// # use exos::Violation;
    /// let app = exos::app().complaints(|field, violation| match (field, violation) {
    ///     ("vat", Violation::Required) => String::from("An invoice needs a VAT id."),
    ///     (_, Violation::Required) => String::from("This is needed."),
    ///     _ => String::from("That does not look right."),
    /// });
    /// ```
    ///
    /// Said once. A second one is ignored rather than racing the first.
    pub fn complaints(
        self,
        say: impl Fn(&str, Violation) -> String + Send + Sync + 'static,
    ) -> Self {
        crate::valid::set_complaints(say);
        self
    }

    /// Says what a session name stands for.
    ///
    /// The resolver runs once per connection, on the stream's `GET`, which is
    /// the one place identity can be established without inventing a second
    /// channel: an `EventSource` is opened with an ordinary request and
    /// therefore carries cookies.
    ///
    /// The name arrives as an `Option` because a stream cannot start a session.
    /// Its response headers went out when it opened, so there is no cookie to
    /// set, and a visitor whose very first request is the stream has no name
    /// yet.
    ///
    /// ```no_run
    /// use exos::{Audience, Audiences};
    ///
    /// #[derive(Hash)]
    /// struct Viewer(u32);
    ///
    /// impl Audience for Viewer {
    ///     const NAME: &'static str = "viewer";
    /// }
    ///
    /// # struct Sessions;
    /// # impl Sessions {
    /// #     async fn viewer(&self, _: &exos::Id) -> Result<Option<u32>, std::io::Error> {
    /// #         Ok(Some(7))
    /// #     }
    /// # }
    /// let app = exos::app().provide(Sessions).identify(async |name| {
    ///     // What anonymous means is not exos's to decide: a visit with no name
    ///     // has nothing to be addressed by, and a name with nobody behind it
    ///     // may still be worth addressing.
    ///     let Some(name) = name else {
    ///         return Ok(Audiences::none());
    ///     };
    ///
    ///     Ok(match exos::data::<Sessions>().viewer(&name).await? {
    ///         Some(user) => Audiences::of(&Viewer(user)),
    ///         None => Audiences::none(),
    ///     })
    /// });
    /// ```
    ///
    /// The lookup is the application's, and so is what it means for a name to
    /// resolve to nobody. exos never learns what a user is.
    ///
    /// # Panics
    ///
    /// If a resolver was said somewhere else, which means two parts of the
    /// program disagree about who a connection is. Keeping the first one
    /// quietly would show up later as tabs that receive nothing for no visible
    /// reason.
    #[track_caller]
    pub fn identify<F, Fut>(self, resolver: F) -> Self
    where
        F: Fn(Option<Id>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Resolution> + Send + 'static,
    {
        crate::identity::set(resolver, Location::caller());
        self
    }

    /// Configures the key everything signed derives from.
    ///
    /// ```no_run
    /// # fn main() -> Result<(), std::env::VarError> {
    /// let app = exos::app().keys(exos::Keys::from_secret(std::env::var("EXOS_SECRET")?));
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Panics
    ///
    /// If a different key is already in place, either because two applications
    /// disagree about the secret or because something was signed first and got
    /// the random one. Quietly keeping the older one would show up later as
    /// tokens that intermittently fail to verify.
    pub fn keys(self, keys: Keys) -> Self {
        crate::keys::set(keys);
        self
    }

    /// Wraps the application's routes in `layer`, as [`Router::layer`] does.
    ///
    /// Middleware that can answer by itself belongs in
    /// [`route_layer`](Self::route_layer) instead, for the reason axum gives:
    /// this one wraps the fallback too, and a guard that wraps the fallback
    /// reports every mistyped URL as somewhere to sign in.
    pub fn layer<L>(self, layer: L) -> Self
    where
        L: Layer<Route> + Clone + Send + Sync + 'static,
        L::Service: Service<Request> + Clone + Send + Sync + 'static,
        <L::Service as Service<Request>>::Response: IntoResponse + 'static,
        <L::Service as Service<Request>>::Error: Into<Infallible> + 'static,
        <L::Service as Service<Request>>::Future: Send + 'static,
    {
        self.map(|routes| routes.layer(layer))
    }

    /// Stores `value` for [`data`](crate::data) to hand back.
    ///
    /// This says what the application starts with, so a line that has already
    /// said it says nothing the second time it runs: an application built again
    /// does not reset a store that has been written to since. Use
    /// [`provide`](crate::provide) to replace one outright, which is what a
    /// test swapping a value wants.
    #[track_caller]
    pub fn provide<T: Send + Sync + 'static>(self, value: T) -> Self {
        crate::context::declare(value, Location::caller());
        self
    }

    /// Wraps the application's routes in `layer`, as [`Router::route_layer`]
    /// does: around what the routes answer and nothing else.
    ///
    /// This is where a guard goes. A request matching no route never reaches
    /// it, so a mistyped URL stays a 404 rather than becoming a redirect to the
    /// sign-in form.
    ///
    /// # Panics
    ///
    /// If the binary declared no routes at all, which axum refuses because
    /// there would be nothing for the layer to wrap.
    pub fn route_layer<L>(self, layer: L) -> Self
    where
        L: Layer<Route> + Clone + Send + Sync + 'static,
        L::Service: Service<Request> + Clone + Send + Sync + 'static,
        <L::Service as Service<Request>>::Response: IntoResponse + 'static,
        <L::Service as Service<Request>>::Error: Into<Infallible> + 'static,
        <L::Service as Service<Request>>::Future: Send + 'static,
    {
        self.map(|routes| routes.route_layer(layer))
    }

    /// Anything else an [`axum::Router`] can do, still inside exos's layers.
    ///
    /// ```ignore
    /// exos::app().with(|routes| routes.merge(api()).nest("/admin", admin()))
    /// ```
    pub fn with(self, build: impl FnOnce(Router) -> Router) -> Self {
        self.map(build)
    }

    fn map(self, build: impl FnOnce(Router) -> Router) -> Self {
        Self {
            routes: build(self.routes),
            serving: None,
        }
    }

    /// The finished router, built the first time it is asked for.
    ///
    /// Once, rather than per connection: sealing merges three routers and lays
    /// three layers over every endpoint, and none of that depends on who is
    /// connecting.
    fn serving(&mut self) -> &mut Router {
        if self.serving.is_none() {
            self.serving = Some(seal(self.routes.clone()));
        }

        self.serving
            .as_mut()
            .expect("it was sealed on the line above; nothing else clears this")
    }
}

/// Nesting an application into a larger axum router, and what a test that wants
/// the router itself reaches for.
impl From<App> for Router {
    fn from(app: App) -> Self {
        match app.serving {
            Some(router) => router,
            None => seal(app.routes),
        }
    }
}

/// What [`axum::serve()`] asks for: something handing out one service per
/// connection. A [`Router`] answers this with a clone of itself, and an
/// application answers it with the router it seals into.
impl<L: Listener> Service<IncomingStream<'_, L>> for App {
    type Response = Router;
    type Error = Infallible;
    type Future = Ready<Result<Router, Infallible>>;

    fn poll_ready(&mut self, _context: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, _stream: IncomingStream<'_, L>) -> Self::Future {
        ready(Ok(self.serving().clone()))
    }
}

/// So that an application answers a request wherever a [`Router`] would, which
/// is what `oneshot` in a test asks of it.
impl<B> Service<Request<B>> for App
where
    B: HttpBody<Data = Bytes> + Send + 'static,
    B::Error: Into<BoxError>,
{
    type Response = Response;
    type Error = Infallible;
    type Future = RouteFuture<Infallible>;

    fn poll_ready(&mut self, _context: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: Request<B>) -> Self::Future {
        self.serving().call(request)
    }
}

/// The application, with every discovered route already on it.
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
/// # Panics
///
/// If two handlers claim the same method on the same path. Finding that out at
/// startup beats finding out from whichever one happened to win.
pub fn app() -> App {
    let (routes, first_run) = discover::routes();

    // Before anything is merged in below, which carries axum's default
    // fallback: two real ones would be a conflict, and a default one loses to
    // this.
    let routes = if first_run {
        routes.fallback(crate::welcome::page)
    } else {
        routes
    };

    App {
        routes,
        serving: None,
    }
}

/// Everything exos puts up around an application.
///
/// Laid down last and therefore outermost, which is what lets a middleware the
/// application added read a session this has not parsed yet at the time it was
/// mounted.
fn seal(routes: Router) -> Router {
    routes
        .merge(crate::asset_routes(discover::asset_sets()))
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
        // the application brought: the stream and the assets are requests too.
        .layer(axum::middleware::from_fn(crate::scope::layer))
        // Once, here, rather than per connection: this is what turns the
        // handlers into services, and nothing about it depends on the request.
        .with_state(())
}
