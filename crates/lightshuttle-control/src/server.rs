//! HTTP server for the local control plane.

use std::future::Future;
use std::net::SocketAddr;

use lightshuttle_runtime::LifecycleHandle;
use tokio::net::TcpListener;

use crate::routes::router;
use crate::state::ControlState;

/// Bind a TCP listener on `addr`. Pass a port of `0` to let the OS pick
/// a free port; read it back from [`TcpListener::local_addr`].
///
/// Exposed as a free function (rather than an associated function on
/// the generic [`ControlServer`]) so callers do not need a turbofish
/// to pin the handle type before they own a state value.
pub async fn bind(addr: SocketAddr) -> std::io::Result<TcpListener> {
    TcpListener::bind(addr).await
}

/// HTTP server hosting the control plane.
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
    /// Build a server bound to `state`.
    #[must_use]
    pub fn new(state: ControlState<H>) -> Self {
        Self { state }
    }

    /// Consume the server and return the underlying axum router. Useful
    /// for in-process integration tests via `tower::ServiceExt::oneshot`,
    /// which avoid the cost of a real TCP bind.
    pub fn into_router(self) -> axum::Router {
        router(self.state)
    }

    /// Serve the control plane on `listener` until `shutdown` resolves.
    ///
    /// Performs a graceful shutdown that drains in-flight requests
    /// before returning.
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
