//! Local HTTP control plane and dashboard for LightShuttle.
//!
//! This crate hosts the developer-facing control surface that runs alongside
//! the orchestrator: a REST API, a WebSocket log stream and an SSR dashboard
//! served on `127.0.0.1`. CLI subcommands such as `lightshuttle restart` are
//! thin clients of the same endpoints.
//!
//! The crate is intentionally empty at this scaffold stage; subsequent PRs
//! land the `LifecycleHandle` consumer, the HTTP server, the REST routes,
//! the WebSocket log channel and the dashboard.
