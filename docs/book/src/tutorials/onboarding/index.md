# Onboarding by language

The [getting started](../getting-started.md) tutorial is deliberately
language agnostic: it boots `alpine` stubs that only echo their
environment. These onboarding tutorials do the opposite. Each one takes
a single programming stack and walks you from an empty directory to a
real HTTP service that connects to Postgres, all booted with one
`lightshuttle up`.

Pick your stack:

- [Node.js](nodejs.md)
- [Python](python.md)
- [Go](go.md)
- [Rust](rust.md)

Every tutorial follows the same arc, so once you have done one the
others read quickly:

1. Scaffold an empty project directory.
2. Write a small HTTP service that reads `DATABASE_URL` and runs one
   query.
3. Add a `Dockerfile` so LightShuttle builds the service for you.
4. Declare a two-resource manifest: a Postgres database and your service.
5. Boot, observe through the CLI, visit the dashboard, shut down.

## What you need

- A running Docker daemon.
- The LightShuttle CLI. If you have not installed it yet, follow
  [Step 1 of getting started](../getting-started.md#step-1-install-lightshuttle).

You do **not** need a local toolchain for the language you pick. The
service is compiled and run inside a container that LightShuttle builds
from your `Dockerfile`, so an empty machine with only Docker and the
CLI is enough. That is the whole point: the same workflow regardless of
the stack underneath.

## What the services have in common

| Stack | HTTP layer | Postgres driver | Base image |
|---|---|---|---|
| Node.js | built-in `node:http` | `pg` | `node:22-alpine` |
| Python | built-in `http.server` | `psycopg` 3 | `python:3.12-slim` |
| Go | `net/http` | `pgx` v5 | `golang:1.23-alpine` |
| Rust | `axum` | `tokio-postgres` | `rust:1.83-slim` |

Each service exposes a single route:

```text
GET / -> 200 {"db":"ok","now":"2026-..."}
```

The handler runs `select now()` against the database and returns the
result as JSON. It is intentionally tiny: the value is seeing the same
LightShuttle workflow wrap four very different ecosystems without
changing a single command.

When you are done, read the [manifest specification][spec] for every
field these manifests use, or jump to the
[dashboard walkthrough](../dashboard.md) for the web UI in depth.

[spec]: https://github.com/nubster-opensources/lightshuttle/blob/main/docs/spec/manifest-v0.md
