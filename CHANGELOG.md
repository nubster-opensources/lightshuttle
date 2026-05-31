# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- _Items in flight will be listed here until the next release._

## [0.2.0] - 2026-05-30

Dashboard and observability release. Adds a local HTTP control plane with a web dashboard, live log and event streaming, a restart workflow, a bundled OpenTelemetry collector, orchestrator self-tracing and Prometheus metrics. Ships two new published crates and a round of network-surface hardening.

### Added

- New crate `lightshuttle-control` (#43): the local HTTP control plane and dashboard, served on `127.0.0.1`.
  - `LifecycleHandle` trait and `ManagerHandle` adapter exposing the running stack without leaking runtime types (#44).
  - HTTP control server wired into `up` with a `GET /healthz` probe (#45).
  - REST API `GET /api/resources` and `GET /api/resources/{name}` (#46).
  - WebSocket log streaming on `GET /ws/logs/{name}` (#47).
  - `POST /api/resources/{name}/restart` plus a `GET /ws/events` lifecycle event stream (#49).
  - Server-side rendered dashboard built with Askama and HTMX, with an embedded stylesheet and HTMX bundle (#50).
- New crate `lightshuttle-otel` (#51): bundles the `otel/opentelemetry-collector` container, injects the standard `OTEL_*` environment variables into resources and exposes an `observability.otel` manifest section.
  - Orchestrator self-tracing over OTLP and a Prometheus `/metrics` endpoint on the dashboard server (#52).
- `restart_one` lifecycle primitive on `LifecycleManager`, with three ordered lifecycle events (#48).
- `lightshuttle restart <resource>` CLI command that follows lifecycle events to completion, with a `--detach` flag (#49).
- `lightshuttle alias` command (`install`/`check`/`uninstall`) that manages the optional `lsh` shell alias: detects bash, zsh, fish and PowerShell, refuses to shadow a conflicting `lsh` on the PATH, and edits the startup file idempotently (#40).
- Optional `dashboard.port` manifest field (#45).

### Changed

- Dependency upgrades: `bollard` 0.18 to 0.21, `schemars` 0.8 to 1, `jsonschema` 0.17 to 0.46; `axum` gains the `ws` feature (#64). The generated JSON Schema now targets draft 2020-12.
- The lifecycle event channel moved from `mpsc` to `broadcast` so multiple consumers (dashboard, CLI, metrics) can subscribe; `LifecycleEvent` gained `serde::Serialize`.
- `LifecycleError::UnknownResource` was renamed to `LifecycleError::ResourceNotFound`.

### Security

- Published ports now bind to `127.0.0.1` by default instead of `0.0.0.0`, so managed services are not exposed to the wider network; a broader bind requires an explicit `address:host:container` mapping (#65).
- Generated resource passwords now use a cryptographically secure random source (#66).
- The control plane sets baseline security headers (`X-Content-Type-Options`, `X-Frame-Options`) and a same-origin Content Security Policy (#73).
- The `restart` client validates that `.lightshuttle/control.url` points at a loopback address (parsed, not prefix matched) and disables HTTP redirects (#72).

### Fixed

- Starting a resource now removes any container left over from a previous run before recreating it, so a second `up` or a `restart` no longer fails with a name conflict (#82).
- Container log chunks now carry the Docker emission timestamp instead of the read time, and the timestamp prefix is stripped from the forwarded bytes (#68).
- `augment_manifest` no longer overwrites a user resource named `lightshuttle_otel` (#67).
- The tracing subscriber is installed with `try_init`, returning an error instead of panicking on a double install (#69).
- The metrics pump no longer leaks a pending entry when a resource fails before becoming healthy (#71).
- The bundled collector healthcheck no longer always reports healthy; a crash now surfaces through the container exit status (#70).
- Database identifier length is bounded to the PostgreSQL 63 byte limit (#75).
- Interpolation references inside `command` and `healthcheck.test` are now validated statically (#76).

### Documentation

- `docs/spec/control-api.md` (REST and WebSocket surface), `docs/spec/observability.md` (spans and metrics) and `docs/tutorial/dashboard.md` (dashboard walkthrough).

### Notes for upgraders

- Two new crates are published: `lightshuttle-control` and `lightshuttle-otel`.
- Managed services now bind to loopback by default. Use the explicit `address:host:container` port form to expose a service on another interface.
- `LifecycleError::UnknownResource` is now `LifecycleError::ResourceNotFound`. This is a breaking change for direct consumers of `lightshuttle-runtime`.

## [0.1.0] - 2026-05-25

First public release. Ships a local development orchestrator able to read a Cargo-style manifest, build and run a graph of Docker services with lifecycle management, signal handling and service discovery via environment variables.

### Added

- Workspace and lint baseline (#14):
  - Cargo workspace with three crates: `lightshuttle` (CLI binary), `lightshuttle-manifest` (manifest model, parser and validator), `lightshuttle-runtime` (Docker runtime and lifecycle).
  - Workspace-wide lint baseline with `clippy::all` and `clippy::pedantic` configured as warnings, escalated to deny in CI.
  - Shared `rust-toolchain.toml` pinning a stable toolchain for reproducible builds.
- Continuous integration workflow (#15):
  - GitHub Actions pipeline running `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings` and `cargo test --workspace --all-features` on every push and pull request.
  - Toolchain pinned via `dtolnay/rust-toolchain` to match `rust-toolchain.toml`.
- Manifest v0 specification (#13, #17):
  - Prose specification at `docs/spec/manifest-v0.md` describing the YAML manifest format, the supported service kinds, the lifecycle contract and the deterministic ordering rules.
  - JSON Schema for the manifest, usable by editors for completion and validation.
- Manifest types, parser and validator (#16):
  - Strongly typed model for plans, services, build sources and dependencies.
  - YAML parser with structured error reporting and source spans.
  - Validator enforcing unique service names, well-formed dependencies, absence of cycles and consistency between service kinds and their fields.
- Docker runtime backend (#18):
  - Runtime backend built on `bollard`, talking to the local Docker daemon.
  - Pulls images, creates containers, attaches them to a per-plan network and streams logs.
  - Idempotent teardown with best-effort cleanup of containers and networks.
- Dockerfile build support (#20):
  - Services declared with a local Dockerfile source are built before being started, with build context resolution and per-service tags.
- Lifecycle manager (#19):
  - `LifecycleManager` orchestrating start-up in dependency order, readiness propagation and graceful shutdown.
  - `run_until_signal(...)` entry point that blocks until Ctrl+C or `SIGTERM`, then tears the plan down in reverse order.
- Service discovery via environment variables (#22):
  - Each service automatically receives `LIGHTSHUTTLE_<DEPENDENCY>_HOST` and `LIGHTSHUTTLE_<DEPENDENCY>_PORT` variables for every declared dependency, so dependants can reach them through Docker's user-defined network DNS without hard-coded host names.
- Command-line interface (#21):
  - Binary `lsh` (crate `lightshuttle`) with top-level commands: `up` to start a plan from a manifest, `down` to stop it, `validate` to check a manifest without running it, `logs` to tail container logs and `version`.

### Documentation

- README with vision, six features and anti-scope.
- CONTRIBUTING with trunk-based development conventions.
- SECURITY policy and Code of Conduct (Contributor Covenant 2.1).
- `docs/spec/manifest-v0.md` describing the manifest format end to end.

### Notes for upgraders

This is the first published version, so no upgrade path applies.

[Unreleased]: https://github.com/nubster-opensources/lightshuttle/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/nubster-opensources/lightshuttle/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/nubster-opensources/lightshuttle/releases/tag/v0.1.0
