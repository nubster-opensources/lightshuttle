//! Local HTTP control plane and dashboard for LightShuttle.
//!
//! This crate hosts the developer-facing control surface that runs
//! alongside the orchestrator: a REST API, a WebSocket log stream and
//! an SSR dashboard served on `127.0.0.1`. CLI subcommands such as
//! `lightshuttle restart` are thin clients of the same endpoints.
//!
//! At v0.2.0 this crate exposes `GET /healthz` plus REST endpoints
//! under `/api/resources*`; subsequent PRs land the WebSocket log
//! channel, the restart endpoint and the dashboard.

pub use crate::error::{ApiError, ApiErrorBody};
pub use crate::metrics::{Metrics, observe_event_duration};
pub use crate::server::{ControlServer, bind};
pub use crate::state::ControlState;

mod error;
mod metrics;
mod routes;
mod server;
mod state;
