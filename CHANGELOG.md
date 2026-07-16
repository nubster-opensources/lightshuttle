# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- `entrypoint` manifest field for `container` and `dockerfile` resources, overriding the image `ENTRYPOINT` independently of `command` (#259).

### Changed
- `ContainerSpec` (`lightshuttle-spec`), `ContainerConfig` and `DockerfileConfig` (`lightshuttle-manifest`) gained the `entrypoint` field. These are public structs with public fields and no `#[non_exhaustive]`, so this is a breaking change for any struct-literal construction of these types downstream. The workspace is bumped to 0.5.0 accordingly.

### Fixed
- The Helm emitter no longer drops the resolved `command`: it was silently lost, for example a redis `--requirepass` value (#261).
- The Kubernetes emitter and the Helm emitter no longer write the resolved `command` into the `command` field. In Kubernetes and Helm, `command` is the entrypoint, not the argument list: writing the resolved `command` there replaced the image entrypoint, so `lightshuttle up` and `lightshuttle export kubernetes`/`export helm` produced different containers. Previously exported Kubernetes manifests and Helm charts put the command in the wrong field: re-export them (#262).

## [0.4.0] - 2026-06-03

_See diff against the previous tag for details._

## [0.3.1] - 2026-06-02

_See diff against the previous tag for details._

## [0.3.0] - 2026-06-02

Production Export release. Adds the `export:` manifest section and the `lightshuttle export <target>` command, which transpiles a manifest to Docker Compose, plain Kubernetes manifests or a Helm chart. Ships two new published crates, a full offline validation CI job and a round of documentation harmonisation.

### Added

- New crate `lightshuttle-spec` (#105): extracts `ContainerSpec`, `from_resource` resolution and `SpecError` from `lightshuttle-runtime` into a dedicated, dependency-light crate, enabling downstream consumers (the export crate) to perform lowering without pulling in the full runtime.
- New crate `lightshuttle-export` (#87): manifest-to-artifact transpilation pipeline.
  - `lower()` converts a resolved manifest to a target-neutral `ExportModel` (reuses `from_resource` from `lightshuttle-spec`, zero drift).
  - `Emitter` trait with three implementations: Compose, Kubernetes and Helm.
  - `resolve` module: six pure helper functions shared across emitters (port defaults, image tags, DNS-1123 normalisation, healthcheck command extraction, volume mount classification, secret heuristic).
  - Secret heuristic: environment variable names containing `PASSWORD`, `PASSWD`, `SECRET`, `TOKEN` or `KEY` are classified as `Secret` in Kubernetes and Helm output.
- `export:` typed manifest section with per-target overrides for Compose, Kubernetes and Helm (#86).
- `lightshuttle export <target> [--output <dir>] [--force]` CLI command (#88): guards against overwriting a non-empty output directory without `--force`, prints a summary of emitted files.
- Docker Compose emitter (#89): emits a `docker-compose.yml` with typed service model, `depends_on: condition: service_healthy`, top-level named volumes and loopback port bindings by default.
- Kubernetes manifests emitter (#90): emits `Deployment`, `Service`, `ConfigMap`, `Secret` and `PersistentVolumeClaim` resources plus a `namespace.yaml`; maps healthcheck probes to liveness/readiness probes; DNS-1123 name normalisation.
- Helm chart emitter (#91): emits `Chart.yaml`, `values.yaml` and per-resource `templates/*.yaml`; parametrised via `index .Values.services`; full parity with the Kubernetes emitter.
- External validation CI job `validate-export` (#92): probes tool availability with `--help` before running; validates Compose output with `docker compose config`, Kubernetes with `kubeconform` and Helm with `helm lint`; non-blocking, runs offline.

### Fixed

- Relative host volume paths (e.g. `./data:/var/lib/data`) are now resolved to absolute paths at manifest load time via `Manifest::resolve_host_volume_paths(base_dir)`, so emitted artefacts are portable across working directories (#107).

### Documentation

- `docs/spec/export.md`: `export:` section schema, target matrix and per-target mapping rules (Compose, Kubernetes, Helm), cross-cutting rules (secret heuristic, DNS-1123 names, built images, reproducibility).
- `docs/tutorial/export.md`: end-to-end walkthrough of `lightshuttle export` against a runnable four-service manifest.
- `examples/04-export`: runnable manifest demonstrating the `export:` section and all three targets.
- Full doc-set harmonisation: README and getting-started dropped pre-alpha framing and now install from crates.io; Commands table and Documentation index added to README; manifest spec moved `dashboard:` and `export:` out of future-reserved; release process lists all seven crates; semver policy names the `lightshuttle` CLI.

### Notes for upgraders

- Two new crates are published: `lightshuttle-spec` and `lightshuttle-export`.
- The `export:` manifest key is no longer future-reserved; it is parsed and validated.
- Relative host volume paths in `volumes` are now resolved at manifest load time. Manifests that relied on the raw relative form being forwarded as-is should switch to absolute paths or keep using relative paths (they will be resolved correctly from the manifest's directory).

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

[Unreleased]: https://github.com/nubster-opensources/lightshuttle/compare/v0.4.0...HEAD
[0.4.0]: https://github.com/nubster-opensources/lightshuttle/releases/tag/v0.4.0
[0.3.1]: https://github.com/nubster-opensources/lightshuttle/releases/tag/v0.3.1
[0.3.0]: https://github.com/nubster-opensources/lightshuttle/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/nubster-opensources/lightshuttle/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/nubster-opensources/lightshuttle/releases/tag/v0.1.0
