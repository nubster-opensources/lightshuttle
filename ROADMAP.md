# Roadmap

LightShuttle is pre-alpha. This document captures the intended trajectory of
the project up to v1.0, ordered by release. **No dates are committed.** The
project is sponsored on a best-effort basis by Nubster, and
releases ship when they are ready, not when a calendar says so.

The roadmap mirrors the GitHub milestones one-for-one. Each section here is
the public, prose form of a milestone; each milestone groups the issues that
must close before the release ships. The full design notes for any given
release live under `docs/design/` and `docs/spec/`.

## Out of scope

LightShuttle is a developer-time orchestrator. The following will never be
in scope, regardless of demand:

- **Production runtime.** We generate manifests; we do not run them.
- **Kubernetes replacement.** No control plane, no scheduler, no operator.
- **Service mesh.** No sidecar proxies, no mTLS, no traffic shaping.
- **CI/CD pipelines.** Orthogonal to existing pipeline tooling.

These boundaries are deliberate and non-negotiable. If a feature request
crosses one of them, it belongs in another project.

## v0.1.0: Minimum Viable Orchestrator

**Goal.** A polyglot developer writes a minimal `lightshuttle.yml` and boots
a Postgres database, an arbitrary container and a locally built Dockerfile
on their laptop with a single command.

**Manifest.**
- `manifest-v0` specification frozen and published under `docs/spec/`.
- Top-level sections: `project:`, `resources:`.
- Resource kinds: `postgres`, `redis`, `container`, `dockerfile`.
- Interpolation: `${resources.<name>.<property>}`, `${env.<NAME>}`.
- JSON Schema generated from Rust types via `schemars` for IDE validation.

**Runtime.**
- Docker only, via the `bollard` crate.
- Topological dependency resolution.
- `wait_for_health` startup policy.
- Graceful shutdown on `SIGINT` and `SIGTERM`, coordinated across resources.

**Discovery.**
- Environment variables auto-injected into each container, following the
  `LSH_<SERVICE>_<PROPERTY>` convention.

**CLI.**
- `lightshuttle up`, `down`, `ps`, `logs`, `validate`, `manifest`.

## v0.2.0: Dashboard and Observability

**Goal.** A developer sees the live state of their stack in a browser and
can stream logs and traces without leaving the dashboard.

**Dashboard.**
- Web UI listing every resource with its current state
  (`running`, `starting`, `failed`, `stopped`).
- Real-time log streaming over WebSocket, scoped per resource.
- Restart action per resource, mirrored by the CLI.

**Observability.**
- Integrated OpenTelemetry collector. `OTEL_EXPORTER_OTLP_ENDPOINT` and
  `OTEL_SERVICE_NAME` injected automatically into each managed process.
- Prometheus-compatible metrics endpoint exposed by the orchestrator.
- The orchestrator itself emits structured traces via `tracing` and
  `tracing-opentelemetry`.

**CLI.**
- `lightshuttle restart <resource>`.

## v0.3.0: Production Export

**Goal.** The same manifest that drives the dev stack produces the artefacts
needed for production deployment.

**Targets.**
- `docker-compose.yml`.
- Plain Kubernetes YAML manifests.
- Helm chart.

**Manifest.**
- Top-level `export:` section to override per target
  (resource sizing, replicas, image references, ingress rules).

**Mapping.**
- `postgres` resources mapped to a `StatefulSet` plus appropriate volume,
  or to a community chart (for example `bitnami/postgresql`) at the user's
  choice.
- `container` resources mapped to `Deployment` plus `Service`.
- `static` resources mapped to a `ConfigMap` plus `Deployment` serving the
  bundle through `nginx` or an equivalent.

**Validation.**
- Generated output linted with `docker compose config` and `helm lint`
  as part of the test suite.

**CLI.**
- `lightshuttle export <target>` where target is `compose`, `kubernetes`
  or `helm`.

## v0.4.0: Secrets and Env Management

**Goal.** Secrets and environment variables flow from local sources into
the dev runtime with fail-fast diagnostics, and the dev stack gets a
per-project network.

**Secrets.**
- `lightshuttle-secrets` crate: `.env` file source and system environment
  resolver, pattern-based interpolation.
- `lightshuttle secrets check` lists required and missing secrets without
  booting anything, sharing the exact resolution engine used by `up`.
- `--env-file` flag overriding the default `.env` path.
- Fail-fast on missing secrets at boot, with every divergent default
  reported.

**Runtime.**
- Per-project Docker bridge network; containers reach each other by
  hostname.
- BuildKit progress stream handled correctly during image builds.

## v0.5.0: Polish and Stability

**Goal.** LightShuttle is usable by external early adopters without hand
holding and the documentation lives somewhere permanent.

**Documentation.**
- Documentation site built with mdBook, published on Pages.
- Onboarding tutorials per primary stack
  (Node.js, Python, Go, Rust).

**Testing.**
- Integration tests built on `testcontainers`.
- Cross-OS smoke tests in CI (Linux first, macOS next, Windows last).

**Performance.**
- Cold-start benchmark target: Postgres plus one container booted in
  under five seconds on a developer-grade laptop.

**Housekeeping.**
- OpenTelemetry stack upgraded to the 0.32 line.
- Workspace standards compliance (ADR rust 001).

## v0.6.0: Polyglot Resources

**Goal.** Cover the rest of the common polyglot stack and provide a clean
migration path from existing `docker-compose` setups.

**Resources.**
- `mysql`, `mariadb`, `mongodb`.
- `static` (single-page application bundle served behind a small web
  server).
- `process` (native command running outside any container, for example
  `cargo run` or `npm run dev`). Requires a prior design pass on
  networking and discovery, since a native process does not join the
  per-project Docker network.

**Migration.**
- `lightshuttle import docker-compose.yml` translates an existing
  Compose file into an equivalent `lightshuttle.yml`.

## v0.7.0: Extension and Lifecycle

**Goal.** The manifest expresses the full lifecycle of a stack, and power
users extend the orchestrator without forking it. This is the last
release allowed to change the shape of the manifest: `manifest-v1`
freezes when it ships.

**Lifecycle.**
- Additional startup policies: `wait_for_completion`,
  `wait_for_log_line`.
- Failure policies: `restart_on_failure`, `fail_fast`,
  `kill_on_dependent_failure`.

**Manifest.**
- Top-level `hooks:` section for global lifecycle hooks.

**Extension.**
- `xtask/` Rust extension contract documented and exercised by tests.
  The orchestrator compiles the user's xtask crate on demand and invokes
  the declared hooks at the right lifecycle moments.

**Specification.**
- `manifest-v1` frozen: only additive changes are accepted until a v2.

## v1.0.0: Stable

**Goal.** The public API is frozen, upgrades are predictable and the
project carries the maturity signals expected from a stable tool.

**API.**
- Public Rust API surface frozen, with SemVer guarantees documented per
  crate and enforced in CI.
- `manifest-v1` is the stable contract.

**Release.**
- At least one release candidate exercised through the full release
  pipeline before the final tag.
- Migration guide from v0.x published in the documentation site.

**Maturity.**
- OpenSSF Best Practices badge.

## Post-1.0 backlog

The items below have been discussed during the design phase but are not
committed to any release. They will only ship if the project gains enough
traction to justify the maintenance cost, and each will require its own
design pass before any code lands.

- **Podman runtime.** Second backend, via the Docker-compatible API.
- **Local DNS resolver.** Optional resolver exposing
  `<service>.lightshuttle.local`.
- **DSL Model B.** External Rust crates referenced via `uses:` in the
  manifest, designed for forward compatibility from v0.1.
- **Containerd runtime.** Third backend after Docker and Podman.
- **Plugins ecosystem.** Third-party hook registry.
- **Live update.** Hot reload of code without rebuilding the container
  image, in the spirit of Tilt.
- **Local TLS automation.** Integrated `mkcert` for `https://` URLs in
  the dev stack.
- **Sustainability.** Open Core or hosted premium tier, GitHub Sponsors.

## How this roadmap is maintained

Changes to this document are made by pull request, with a
`docs(roadmap):` Conventional Commit. The scope of v0.1.0 is locked once
the `manifest-v0` specification is merged; the scope of later releases
stays adjustable until the previous release ships.

If you spot something missing, redundant or out of scope, open an issue
against the relevant milestone and tag it `discussion`.
