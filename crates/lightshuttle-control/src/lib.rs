#![deny(missing_docs)]
//! Local HTTP control plane and dashboard for LightShuttle.
//!
//! This crate is the developer-facing control surface that runs alongside the
//! LightShuttle orchestrator. It depends on [`lightshuttle_runtime`] for the
//! [`lightshuttle_runtime::LifecycleHandle`] trait and the resource view types,
//! and adds an HTTP layer on top: a REST API, WebSocket streams for logs and
//! lifecycle events, Prometheus metrics, and an SSR dashboard built with Askama
//! and HTMX.
//!
//! # Crate layout
//!
//! | Public item | Role |
//! |---|---|
//! | [`ControlState`] | Shared axum state (project name + lifecycle handle + metrics) |
//! | [`ControlServer`] | HTTP server wrapping an axum router |
//! | [`bind`] | Async helper to open a `TcpListener` before building the server |
//! | [`Metrics`] | Prometheus recorder and scrape renderer |
//! | [`observe_event_duration`] | Record a lifecycle-event duration histogram sample |
//! | [`ApiError`] / [`ApiErrorBody`] | HTTP error type returned by every REST handler |
//!
//! # Security note
//!
//! The control plane is designed for **local development only**. It carries no
//! authentication and the caller is expected to bind it to the loopback address
//! (`127.0.0.1`). Never expose this server on a non-loopback interface or in a
//! shared or production environment.
//!
//! # Quick start
//!
//! ```rust,no_run
//! use std::net::SocketAddr;
//! use lightshuttle_control::{ControlState, ControlServer, Metrics, bind};
//! # use lightshuttle_runtime::{
//! #     LifecycleEvent, LifecycleHandle, LifecycleHandleError,
//! #     LogChunkStream, ResourceView,
//! # };
//! # use tokio::sync::broadcast;
//! # #[derive(Clone)]
//! # struct MyHandle;
//! # impl LifecycleHandle for MyHandle {
//! #     async fn list(&self) -> Result<Vec<ResourceView>, LifecycleHandleError> { Ok(vec![]) }
//! #     async fn get(&self, _: &str) -> Result<ResourceView, LifecycleHandleError> {
//! #         Err(LifecycleHandleError::NotSupported("get"))
//! #     }
//! #     async fn restart(&self, _: &str) -> Result<(), LifecycleHandleError> {
//! #         Err(LifecycleHandleError::NotSupported("restart"))
//! #     }
//! #     async fn logs(&self, _: &str, _: bool) -> Result<LogChunkStream, LifecycleHandleError> {
//! #         Err(LifecycleHandleError::NotSupported("logs"))
//! #     }
//! #     fn subscribe_events(&self) -> broadcast::Receiver<LifecycleEvent> {
//! #         broadcast::channel(1).1
//! #     }
//! # }
//!
//! #[tokio::main]
//! async fn main() -> std::io::Result<()> {
//!     // Bind to loopback only. The control plane has no authentication.
//!     let addr: SocketAddr = "127.0.0.1:9090".parse().unwrap();
//!     let listener = bind(addr).await?;
//!
//!     let metrics = std::sync::Arc::new(Metrics::install());
//!     let state = ControlState::with_metrics("my-project", MyHandle, metrics);
//!     let server = ControlServer::new(state);
//!
//!     server.serve(listener, async { /* await shutdown signal */ }).await
//! }
//! ```

pub use crate::error::{ApiError, ApiErrorBody};
pub use crate::metrics::{Metrics, observe_event_duration};
pub use crate::server::{ControlServer, bind};
pub use crate::state::ControlState;

mod error;
mod metrics;
mod routes;
mod server;
mod state;
