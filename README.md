# LightShuttle

> Lightweight dev orchestrator for polyglot teams: typed `docker-compose` successor with built-in dashboard, OpenTelemetry and production export.

[![License: MIT OR Apache-2.0](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg)](#license)
[![Status](https://img.shields.io/badge/status-pre--alpha-orange)](#status)
[![Made with Rust](https://img.shields.io/badge/made%20with-Rust-orange?logo=rust)](https://www.rust-lang.org/)

LightShuttle (binary: `lightshuttle`) is a developer-time orchestrator written in Rust. You declare your service stack once in `lightshuttle.yml` (databases, queues, containers, Dockerfiles, static SPAs), and `lightshuttle up` boots the whole thing on your laptop with automatic service discovery, an integrated web dashboard, OpenTelemetry traces and logs, and a one-command export to `docker-compose.yml`, Kubernetes manifests or a Helm chart for production.

LightShuttle is sponsored by [Encelade Technologies](https://encelade.tech).

## Status

🚧 **Pre-alpha, no usable release yet.**

| Phase | State |
| --- | --- |
| 1. Product vision and scope | ✅ Closed (2026-05-21) |
| 2. Technical architecture | ⏳ In progress |
| 3. Open source strategy | ⏳ Pending |
| 4. Proof of concept (1 to 2 weeks spike) | ⏳ Pending |
| 5. v0.1.0 public release | ⏳ Target Q3 2026 |

The repository is intentionally public from day one to capture the name and make the design discussion visible. **Do not depend on it yet**, anything can change until v0.1.0.

## Quickstart

> Pre-alpha. The CLI works but everything below may change before v0.1.0.

**Prerequisites:** Docker Desktop or a Docker Engine daemon running locally, plus a Rust toolchain (`rustup` is the recommended installer).

Install LightShuttle from the cloned repository (it is not on crates.io yet):

```sh
git clone https://github.com/nubster-opensources/lightshuttle.git
cd lightshuttle
cargo install --path crates/lightshuttle
```

Once published, the install command will simply be `cargo install lightshuttle`.

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

LightShuttle keeps the simplicity of a single YAML file but layers type validation, a live dashboard, automatic environment variable injection between services, a clean extension point in Rust for custom hooks and lifecycles, and a first-class production export.

## What LightShuttle is **not**

To stay focused, the following are explicitly out of scope:

- **Not a production runtime.** LightShuttle is a developer-time orchestrator. The production export targets Kubernetes, Helm or plain `docker-compose`, but LightShuttle itself does not run in production.
- **Not a Kubernetes replacement.** We generate manifests; we do not run a control plane.
- **Not a service mesh.** Service discovery is provided by environment variables and an optional local DNS; sidecar proxies, mTLS and traffic shaping are deliberately left to dedicated tools.
- **Not a CI/CD pipeline.** LightShuttle is orthogonal to GitHub Actions, GitLab CI or any other pipeline tool.

## Configuration model

The primary configuration lives in `lightshuttle.yml`, a declarative manifest readable by every developer regardless of their primary language. For the 10 % of cases that need custom lifecycle logic, for example running a one-shot migration container before starting the long-running service, LightShuttle reuses the Cargo `xtask/` pattern: a small Rust crate inside the project exposes hooks that the orchestrator calls at the right moments.

A minimal example will be added once the architecture phase finalises the YAML schema.

## Contributing

LightShuttle is in pre-alpha and the public API is unstable. The repository is open so the design phase can happen in public, not so that external contributions can be accepted yet. Once v0.1.0 ships, the contribution model will be documented in `CONTRIBUTING.md`.

Until then, feel free to open a discussion if you want to give early feedback on the direction.

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT License ([LICENSE-MIT](LICENSE-MIT))

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual-licensed as above, without any additional terms or conditions.

See [CONTRIBUTING.md](CONTRIBUTING.md) for details, including the Contributor License Agreement (CLA).

Copyright © Encelade Technologies.
