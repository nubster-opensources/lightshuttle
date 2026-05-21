# LightShuttle

> Lightweight dev orchestrator for polyglot teams: typed `docker-compose` successor with built-in dashboard, OpenTelemetry and production export.

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
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

LightShuttle is distributed under the terms of the [MIT license](./LICENSE).

Copyright © Encelade Technologies.
