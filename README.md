# LightShuttle

> Lightweight dev orchestrator for polyglot teams: typed `docker-compose` successor with built-in dashboard, OpenTelemetry and production export.

[![crates.io](https://img.shields.io/crates/v/lightshuttle.svg?label=crates.io)](https://crates.io/crates/lightshuttle)
[![docs.rs](https://img.shields.io/docsrs/lightshuttle?label=docs.rs)](https://docs.rs/lightshuttle)
[![CI](https://github.com/nubster-opensources/lightshuttle/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/nubster-opensources/lightshuttle/actions/workflows/ci.yml)
[![MSRV](https://img.shields.io/badge/MSRV-1.88-blue.svg)](./docs/MSRV_POLICY.md)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg)](#license)
[![Made with Rust](https://img.shields.io/badge/made%20with-Rust-orange?logo=rust)](https://www.rust-lang.org/)

LightShuttle (binary: `lightshuttle`) is a developer-time orchestrator written in Rust. You declare your service stack once in `lightshuttle.yml` (databases, queues, containers, Dockerfiles, static SPAs), and `lightshuttle up` boots the whole thing on your laptop with automatic service discovery, an integrated web dashboard, OpenTelemetry traces and logs, and a one-command export to `docker-compose.yml`, Kubernetes manifests or a Helm chart for production.

LightShuttle is sponsored by [Nubster](https://nubster.com).

## Status

LightShuttle is published on crates.io and under active development. The
public API is still pre-1.0 and may change between minor versions; see
the [SemVer policy](docs/SEMVER_POLICY.md).

- **v0.1.0** Minimum viable orchestrator: typed manifest, topological
  startup, healthchecks, graceful shutdown, env-var service discovery,
  and the core CLI.
- **v0.2.0** Dashboard and observability: a local web dashboard, live log
  and event streaming, a bundled OpenTelemetry collector, Prometheus
  metrics and the `restart` command.
- **v0.3.0** _(in progress)_ Production export: `lightshuttle export`
  to `docker-compose.yml`, Kubernetes manifests or a Helm chart.

See the [roadmap](ROADMAP.md) for what comes next.

## Quickstart

**Prerequisites:** Docker Desktop or a Docker Engine daemon running locally, plus a Rust toolchain (`rustup` is the recommended installer).

Install LightShuttle from crates.io:

```sh
cargo install lightshuttle
```

> Typing `lightshuttle` is verbose. If your shell has no `lsh` already (check with `command -v lsh`), see the [optional shell alias](docs/tutorial/getting-started.md#optional-shell-alias-lsh) section of the tutorial.

Create a `lightshuttle.yml` file at the root of your project:

```yaml
# lightshuttle.yml
project:
  name: hello
resources:
  db:
    postgres:
      version: "16"
```

Boot the stack:

```sh
$ lightshuttle up
```

The orchestrator validates the manifest, pulls the image, starts Postgres, waits for the healthcheck to pass and supervises the container until you press `Ctrl+C`. Shutdown is coordinated and idempotent.

For a complete walkthrough see [`docs/tutorial/getting-started.md`](docs/tutorial/getting-started.md). Runnable manifests live in [`examples/`](examples/).

## Why LightShuttle

`docker-compose` is the de facto local stack runner for polyglot teams, but it suffers from real ergonomic gaps that LightShuttle aims to close:

- **No typing**, a typo in a service name silently breaks the stack.
- **No dashboard**, you fall back to `docker logs` and `docker ps` for everything.
- **No service discovery**, you manually wire `DATABASE_URL` and `REDIS_URL` between containers.
- **No production export**, the same YAML cannot be reused or transformed into Kubernetes resources.
- **No custom lifecycle**, running a migration before serving traffic requires shell scripts and `depends_on: service_healthy` ceremonies.

LightShuttle keeps the simplicity of a single YAML file but layers type validation, a live dashboard, automatic environment variable injection between services, and a first-class production export, with a clean Rust extension point for custom hooks and lifecycles on the roadmap.

## What LightShuttle is **not**

To stay focused, the following are explicitly out of scope:

- **Not a production runtime.** LightShuttle is a developer-time orchestrator. The production export targets Kubernetes, Helm or plain `docker-compose`, but LightShuttle itself does not run in production.
- **Not a Kubernetes replacement.** We generate manifests; we do not run a control plane.
- **Not a service mesh.** Service discovery is provided by environment variables and an optional local DNS; sidecar proxies, mTLS and traffic shaping are deliberately left to dedicated tools.
- **Not a CI/CD pipeline.** LightShuttle is orthogonal to GitHub Actions, GitLab CI or any other pipeline tool.

## Configuration model

Everything lives in a single `lightshuttle.yml`, a typed declarative manifest readable by every developer regardless of their primary language. It declares a `project`, a set of `resources` (`postgres`, `redis`, `container`, `dockerfile`), their dependencies and `${...}` interpolations, and optional `dashboard`, `observability` and `export` sections. The full schema is in the [manifest specification](docs/spec/manifest-v0.md); a JSON Schema for editor autocompletion ships at [`docs/spec/manifest-v0.schema.json`](docs/spec/manifest-v0.schema.json). Custom lifecycle hooks through a Cargo-style `xtask/` crate are on the [roadmap](ROADMAP.md), not yet implemented.

## Commands

| Command | Purpose |
| --- | --- |
| `lightshuttle up` | Boot the stack and supervise it until interrupted. |
| `lightshuttle down` | Stop every managed container. |
| `lightshuttle ps` | List managed resources and their status. |
| `lightshuttle logs <resource>` | Stream a resource's logs. |
| `lightshuttle restart <resource>` | Restart one resource through the running control plane. |
| `lightshuttle validate` | Parse and validate the manifest without starting anything. |
| `lightshuttle manifest` | Print the resolved manifest. |
| `lightshuttle export <target>` | Generate `docker-compose.yml`, Kubernetes manifests or a Helm chart. |
| `lightshuttle alias install` | Manage the optional `lsh` shell alias. |

## Documentation

- Tutorials: [getting started](docs/tutorial/getting-started.md), [the dashboard](docs/tutorial/dashboard.md), [export](docs/tutorial/export.md).
- Specifications: [manifest](docs/spec/manifest-v0.md), [control plane API](docs/spec/control-api.md), [observability](docs/spec/observability.md), [export](docs/spec/export.md).
- Project policies: [roadmap](ROADMAP.md), [SemVer](docs/SEMVER_POLICY.md), [MSRV](docs/MSRV_POLICY.md), [release process](docs/RELEASE_PROCESS.md), [governance](docs/GOVERNANCE.md).

The workspace is split into published crates: `lightshuttle` (the CLI), `lightshuttle-manifest`, `lightshuttle-spec`, `lightshuttle-runtime`, `lightshuttle-otel`, `lightshuttle-control` and `lightshuttle-export`.

## Contributing

LightShuttle is open source and pre-1.0; the public API may still change between minor versions. See [`CONTRIBUTING.md`](CONTRIBUTING.md) for the contribution model and the Contributor License Agreement, and [`SECURITY.md`](SECURITY.md) to report a vulnerability. Feedback and discussion on the direction are welcome.

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT License ([LICENSE-MIT](LICENSE-MIT))

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual-licensed as above, without any additional terms or conditions.

See [CONTRIBUTING.md](CONTRIBUTING.md) for details, including the Contributor License Agreement (CLA).

Copyright © Nubster.
