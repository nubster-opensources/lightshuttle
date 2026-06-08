# Introduction

LightShuttle is a developer-time orchestrator written in Rust. You declare
your service stack once in `lightshuttle.yml` (databases, queues, containers,
Dockerfiles), and `lightshuttle up` boots the whole thing on your laptop with
automatic service discovery, an integrated web dashboard, OpenTelemetry traces
and logs, and a one-command export to `docker-compose.yml`, Kubernetes
manifests or a Helm chart for production.

LightShuttle is sponsored by [Nubster](https://nubster.com) and dual-licensed
under MIT or Apache-2.0.

## Install

LightShuttle needs a running Docker daemon and a Rust toolchain
([`rustup`](https://rustup.rs/) is the recommended installer). Install the CLI
from crates.io:

```sh
cargo install lightshuttle
```

Confirm the install:

```sh
$ lightshuttle --version
lightshuttle 0.4.0
```

## Quickstart

Create a `lightshuttle.yml` at the root of your project:

```yaml
project:
  name: hello
resources:
  db:
    postgres:
      version: "16"
```

Boot the stack:

```sh
lightshuttle up
```

LightShuttle validates the manifest, pulls the image, starts Postgres, waits
for the healthcheck to pass and supervises the container until you press
`Ctrl+C`. Shutdown is coordinated and idempotent.

## How this documentation is organised

This site follows the [Diátaxis](https://diataxis.fr/) framework: four kinds of
documentation, each serving a different need.

- **[Tutorials](tutorials/getting-started.md)** are lessons that take you by the
  hand through a series of steps. Start here if you are new.
- **[How-to guides](how-to/index.md)** are recipes for solving a specific task
  once you know the basics.
- **[Reference](reference/index.md)** is the exhaustive, normative description
  of the manifest, the CLI and the public APIs.
- **[Explanation](explanation/index.md)** discusses the design and the reasoning
  behind it.

## Status

LightShuttle is published on crates.io and under active development. The public
API is pre-1.0 and may change between minor versions; see the
[SemVer policy](https://github.com/nubster-opensources/lightshuttle/blob/main/docs/SEMVER_POLICY.md)
and the
[roadmap](https://github.com/nubster-opensources/lightshuttle/blob/main/ROADMAP.md).
