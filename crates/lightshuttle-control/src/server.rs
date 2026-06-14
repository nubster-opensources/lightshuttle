//! HTTP server for the local control plane.
//!
//! The two public entry points are:
//!
//! - [`bind`]: opens a [`tokio::net::TcpListener`] before the server is
//!   constructed (allows reading back the OS-assigned port when port `0` is
//!   passed).
//! - [`ControlServer`]: wraps a [`crate::ControlState`] and drives the axum
//!   router until a caller-supplied shutdown future resolves.

use std::future::Future;
use std::net::SocketAddr;

use lightshuttle_runtime::LifecycleHandle;
use tokio::net::TcpListener;

use crate::routes::router;
use crate::state::ControlState;

/// Open a TCP listener that will be passed to [`ControlServer::serve`].
///
/// Pass a port of `0` to let the OS assign a free port; read it back
/// with [`TcpListener::local_addr`] after the call returns.
///
/// This is a free function rather than an associated function on the
/// generic [`ControlServer`] so callers can open the socket before
/// constructing state, without needing a turbofish to pin `H`.
///
/// Always bind to a loopback address (`127.0.0.1`) in practice: the
/// control plane carries no authentication and is intended only for
/// local developer use.
///
/// # Errors
///
/// Returns an [`std::io::Error`] if the bind fails (address already in
/// use, permission denied, etc.).
///
/// # Example
///
/// ```rust,no_run
/// use std::net::SocketAddr;
/// use lightshuttle_control::bind;
///
/// # async fn run() -> std::io::Result<()> {
/// // Loopback only: the control plane has no authentication.
/// let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
/// let listener = bind(addr).await?;
/// let port = listener.local_addr()?.port();
/// println!("control plane listening on port {port}");
/// # Ok(())
/// # }
/// ```
pub async fn bind(addr: SocketAddr) -> std::io::Result<TcpListener> {
    TcpListener::bind(addr).await
}

/// HTTP server that hosts the control plane router.
///
/// Generic over `H`, which must implement
/// [`lightshuttle_runtime::LifecycleHandle`]. The handle is held inside a
/// [`crate::ControlState`] and shared across all route handlers via axum's
/// state mechanism.
///
/// # Usage
///
/// 1. Call [`bind`] to open a listener (loopback only, no authentication).
/// 2. Build a [`crate::ControlState`] with the project name and handle.
/// 3. Construct a [`ControlServer`] via [`ControlServer::new`].
/// 4. Await [`ControlServer::serve`] with a shutdown future.
///
/// For in-process integration tests, use [`ControlServer::into_router`] to
/// get the raw axum router and drive it with `tower::ServiceExt::oneshot`
/// without opening a TCP socket.
pub struct ControlServer<H>
where
    H: LifecycleHandle + Clone + Send + Sync + 'static,
{
    state: ControlState<H>,
}

impl<H> ControlServer<H>
where
    H: LifecycleHandle + Clone + Send + Sync + 'static,
{
    /// Build a server from the given shared state.
    ///
    /// The state holds the project name, the lifecycle handle, and the
    /// Prometheus metrics renderer. It is moved into the axum router when
    /// [`ControlServer::serve`] or [`ControlServer::into_router`] is called.
    #[must_use]
    pub fn new(state: ControlState<H>) -> Self {
        Self { state }
    }

    /// Consume the server and return the underlying [`axum::Router`].
    ///
    /// Useful for in-process integration tests: pass the router to
    /// `tower::ServiceExt::oneshot` to send synthetic requests without the
    /// overhead of a real TCP bind.
    pub fn into_router(self) -> axum::Router {
        router(self.state)
    }

    /// Run the control plane on `listener` until `shutdown` resolves.
    ///
    /// Starts accepting connections immediately. When `shutdown` resolves,
    /// axum performs a graceful shutdown: it stops accepting new connections
    /// and waits for in-flight requests to complete before returning.
    ///
    /// # Errors
    ///
    /// Propagates any [`std::io::Error`] from the underlying TCP accept loop.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use std::net::SocketAddr;
    /// use lightshuttle_control::{ControlState, ControlServer, bind};
    /// # use lightshuttle_runtime::{
    /// #     LifecycleEvent, LifecycleHandle, LifecycleHandleError,
    /// #     LogChunkStream, ResourceView,
    /// # };
    /// # use tokio::sync::broadcast;
    /// # #[derive(Clone)]
    /// # struct MyHandle;
    /// # impl LifecycleHandle for MyHandle {
    /// #     async fn list(&self) -> Result<Vec<ResourceView>, LifecycleHandleError> { Ok(vec![]) }
    /// #     async fn get(&self, _: &str) -> Result<ResourceView, LifecycleHandleError> {
    /// #         Err(LifecycleHandleError::NotSupported("get"))
    /// #     }
    /// #     async fn restart(&self, _: &str) -> Result<(), LifecycleHandleError> {
    /// #         Err(LifecycleHandleError::NotSupported("restart"))
    /// #     }
    /// #     async fn logs(&self, _: &str, _: bool) -> Result<LogChunkStream, LifecycleHandleError> {
    /// #         Err(LifecycleHandleError::NotSupported("logs"))
    /// #     }
    /// #     fn subscribe_events(&self) -> broadcast::Receiver<LifecycleEvent> {
    /// #         broadcast::channel(1).1
    /// #     }
    /// # }
    ///
    /// # async fn run() -> std::io::Result<()> {
    /// let addr: SocketAddr = "127.0.0.1:9090".parse().unwrap();
    /// let listener = bind(addr).await?;
    /// let state = ControlState::new("my-project", MyHandle);
    /// let server = ControlServer::new(state);
    ///
    /// // Shutdown when Ctrl-C is received.
    /// server
    ///     .serve(listener, async { tokio::signal::ctrl_c().await.ok(); })
    ///     .await
    /// # }
    /// ```
    pub async fn serve<F>(self, listener: TcpListener, shutdown: F) -> std::io::Result<()>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let app = router(self.state);
        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown)
            .await
    }
}
