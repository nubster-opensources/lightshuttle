//! HTTP server for the local control plane.

use std::future::Future;
use std::net::SocketAddr;

use tokio::net::TcpListener;

use crate::routes::router;
use crate::state::ControlState;

/// HTTP server hosting the control plane.
///
/// Binding is exposed as a standalone async function so the caller can
/// learn the resolved port (random binds with `:0` are reported back
/// via [`TcpListener::local_addr`]) before [`Self::serve`] starts
/// driving the router.
pub struct ControlServer {
    state: ControlState,
}

impl ControlServer {
    /// Build a server bound to `state`.
    #[must_use]
    pub fn new(state: ControlState) -> Self {
        Self { state }
    }

    /// Bind a TCP listener on `addr`. Pass a port of `0` to let the OS
    /// pick a free port; read it back from
    /// [`TcpListener::local_addr`].
    pub async fn bind(addr: SocketAddr) -> std::io::Result<TcpListener> {
        TcpListener::bind(addr).await
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
