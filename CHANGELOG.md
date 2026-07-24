# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- `secrets:` manifest map for `container` and `dockerfile` resources, declaring which environment variables are sensitive. Values are injected at runtime exactly like `env:`, but export targets emit a placeholder instead of the resolved value. Declaring the same key under both `env:` and `secrets:` is rejected.
- `ContainerSpec` (`lightshuttle-spec`) gained the `secret_env_keys` field. The struct is `#[non_exhaustive]`, so this is not a breaking change.
- `lightshuttle_manifest::canonical` module, holding one parser per normalised grammar the workspace relies on: `ImageReference` for OCI image references, `DnsName` for RFC 1123 labels, `parse_duration` and `to_whole_seconds` for durations, `VolumeMapping` for volume entries, and `encode_userinfo` and `encode_path_segment` for URL components. Downstream crates consume the parsed type instead of splitting or sanitising the string themselves, so each grammar is understood in one place.
- `lightshuttle_export::resolve::namespace_label_for`, which resolves the export namespace as a validated DNS label. `namespace_for` is unchanged and still returns the raw value.
- `ExportArtifacts::ensure_unique_paths`, run by every emitter before returning, so two resources can never write to the same output path.
- Helm `values.yaml` gained a per-service `image.digest` entry, emitted only when the reference is digest pinned. A chart for an ordinary tagged image is unchanged.

### Changed
- `lightshuttle export compose` now writes `${KEY}` for every value classified as sensitive, instead of its resolved contents. A generated `docker-compose.yml` is therefore no longer self contained: supply those variables through the environment or a `.env` file placed next to it. This applies to values derived from other resources, such as a database URL, since those carry credentials.
- The control plane rejects browser requests whose `Origin` does not match the request `Host`, and requests whose `Host` is not a loopback authority. Binding to loopback does not prevent a hostile local page from reaching the API, so the boundary is enforced on the headers as well. Non-browser clients such as the CLI send neither `Origin` nor Fetch Metadata headers and are unaffected.
- A manifest name that is not already a DNS label now receives a deterministic suffix when it is normalised, so distinct names stay distinct. A name that already is a label is untouched, so ordinary resources keep the identifiers they have. Resources named with an underscore, an uppercase letter or a trailing separator will be renamed on the next export or `up`: re-export the artifacts and expect the corresponding Kubernetes objects and Docker networks to be recreated under their new names.

### Fixed
- `lightshuttle up` no longer corrupts an image reference served by a registry on a custom port (#290). The reference was split on its first colon, so `registry.example.com:5000/team/api:1.2` was pulled as repository `registry.example.com` with tag `5000/team/api:1.2`, which does not exist.
- The Helm emitter no longer corrupts a digest pinned or ported reference (#278). It split on the last colon, so `alpine@sha256:...` produced repository `alpine@sha256` with the digest payload as its tag, and `registry.example.com:5000/team/api` lost its repository path into the tag. A digest pinned image is now rendered as `repository@digest`.
- A malformed image reference is now reported with the offending resource named, instead of being passed on to the container daemon or written into an artifact.
- Two resource names differing only by their separator no longer overwrite each other's exported artifact (#281). `foo_bar` and `foo-bar` are both valid manifest names and both normalised to `foo-bar`, so Kubernetes and Helm wrote one file for the two and the Helm `values.yaml` dropped one service entirely. Names are now normalised injectively.
- A project name that is not a DNS label no longer produces an invalid Kubernetes namespace (#284). An explicit `export.kubernetes.namespace` that is not a valid namespace is now rejected with a diagnostic rather than silently rewritten, since it is a value the user wrote deliberately.
- Two projects whose names differ only by their separator no longer share a Docker network (#288), which broke the documented one-network-per-project isolation. An existing network is now reused only when it carries this project's `lightshuttle.project` label; a network owned by another project, or carrying no owner at all, is refused with a diagnostic, and `down` never removes one it does not own. Networks created by LightShuttle have always carried that label, so an upgrade is unaffected; a `lightshuttle-<project>` network created by hand must now be removed or labelled.
- `lightshuttle validate` no longer accepts durations that `up` and `export` reject (#279). Validation had its own looser reading, a run of digits and dots followed by a unit, so `.s`, `1..2s` and `..5s` passed validation and failed at lowering. Both stages now share one parser, which is what makes the promise of `validate` true by construction. Those three forms are now reported by `validate` itself.
- A sub-second healthcheck no longer produces a Kubernetes object the API rejects (#285). `interval: 200ms` was lowered with `Duration::as_secs` and floored to `periodSeconds: 0`, while the API requires at least 1. A probe with a period of zero does not fail loudly, it leaves the container permanently unready. Sub-second durations now round up to one second, and `retries: 0` emits `failureThreshold: 1`, the closest value the target can express.
- A duration too large to represent is now reported instead of silently saturating, and a positive duration shorter than one nanosecond is reported instead of becoming zero. Both were consequences of parsing through `f64`; parsing is now integer arithmetic.
- A Windows host path is no longer mistaken for a named volume (#282). A mapping was split on its first colon, so `C:\project\data:/data` resolved to a volume named `C` mounted at `\project\data:/data`: Compose declared a spurious top-level volume, and Kubernetes emitted a claim where a `hostPath` belonged. Drive qualified sources are recognised on every platform, so an export produces the same artifact wherever it runs.
- A volume mapping carrying a third field, such as `./data:/app:ro`, is now rejected with a diagnostic naming it (#300). The extra field used to be folded into the target, so the container mounted at the literal path `/app:ro` and nothing reported it. Mount options remain unsupported; this change is about refusing to guess rather than about adding them.
- The Helm target no longer discards host path and anonymous volumes (#301). Only named volumes were kept, so a chart declaring `./data:/app` mounted nothing at `/app`, and a `postgres` or `redis` resource exported without its data volume. The chart was well formed and passed `helm lint`, so the loss was visible only in a deployed cluster. Helm now represents all three sources exactly as the Kubernetes target does, under the same volume names. Re-export any chart generated by an earlier version: the volumes it should have carried were absent from it.
- Shutdown no longer leaves stopped containers and the project network behind (#289). `stop_all` stopped each container but never removed it, and a stopped container keeps its endpoint on the network, so the following network teardown was rejected and both survived. Containers are now removed (forced, named volumes preserved) in reverse dependency order before the network is torn down, and removal is attempted even for a container whose stop failed, so a single unresponsive container cannot strand the network. `lightshuttle down` gained the same removal step and now also tears down the project network, including when no container remains, so a network orphaned by a hard-killed manager is reclaimed.
- Concurrent restarts of the same resource no longer race and tear down each other's fresh container (#280). The control endpoint spawned an untracked task per POST and `restart_one` held no operation-level lock, so two overlapping restarts could both stop the container, then one recreate it while the other's pre-start cleanup removed that fresh container by name. Restarts are now serialized per resource: the first acquires an exclusive permit for the whole stop-then-start cycle, and a second restart of the same resource while one is in flight is rejected with `409 Conflict` (body `{"error":"restart already in progress","resource":"<name>"}`) instead of being scheduled. Restarts of distinct resources stay independent.
- Postgres and Redis connection URLs now percent encode their credentials (#286). A password such as `p@ss` produced `postgres://user:p@ss@host:5432/db`, where the first `@` closes the userinfo and the host becomes `ss@host`. Any reserved character had the same shape: it did not corrupt the value, it moved a boundary of the URL. The structured `user`, `password` and `database` outputs are unchanged and still raw, since they are handed to the service directly. A URL built from ordinary alphanumeric credentials is byte for byte what it was.

### Security
- Values declared under `secrets:` are no longer written into exported Compose, Kubernetes or Helm artifacts. The previous key-name heuristic is kept as a safety net, so a key such as `DB_PASSWORD` stays redacted even when it is not declared, but a name that carries no marker, such as `DATABASE_URL`, was previously exported in clear text.
- The reusable `ai-review` workflow is pinned by commit SHA instead of a moving branch reference. A reusable workflow referenced by branch executes whatever that branch contains, in a job that receives repository secrets.
- A relative host volume `src` that escapes the manifest directory through a `..` component is now rejected instead of being emitted verbatim (#118). The traversal check was present but its result was returned as `None`, which the caller treated as "leave the mapping unchanged", so a mapping such as `./foo/../../etc/passwd:/etc/passwd` survived into the exported Compose or Kubernetes `hostPath`. Anyone who controlled a manifest could mount an arbitrary host path into a container. The rejection is now a propagated `ManifestError::InvalidVolumePath`, so `resolve_host_volume_paths` fails loudly, and `ManifestError` is `#[non_exhaustive]` so future variants stay non breaking.

## [0.5.0] - 2026-07-16

### Added
- `entrypoint` manifest field for `container` and `dockerfile` resources, overriding the image `ENTRYPOINT` independently of `command` (#259): setting `entrypoint` discards the image `CMD`, so `command` must be set as well to supply arguments.

### Changed
- `ContainerSpec` (`lightshuttle-spec`), `ContainerConfig` and `DockerfileConfig` (`lightshuttle-manifest`) gained the `entrypoint` field. These are public structs with public fields and no `#[non_exhaustive]`, so this is a breaking change for any struct-literal construction of these types downstream. The workspace is bumped to 0.5.0 accordingly.
- All three structs are now `#[non_exhaustive]` and gain a `new` constructor for their required fields. Downstream code constructs them via `new(...)` plus field assignment instead of a struct literal; future field additions are no longer a breaking change.

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
