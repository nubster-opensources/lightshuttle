# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- _Items in flight will be listed here until the next release._

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

[Unreleased]: https://github.com/nubster-opensources/lightshuttle/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/nubster-opensources/lightshuttle/releases/tag/v0.1.0
