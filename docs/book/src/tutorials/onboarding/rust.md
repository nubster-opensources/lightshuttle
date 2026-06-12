# Onboarding: Rust

This tutorial takes about twenty minutes. You will build a small Rust
HTTP service with `axum` that queries Postgres, then boot it next to a
database with a single `lightshuttle up`. You do not need Rust installed
locally: LightShuttle builds the service inside a container from the
`Dockerfile` you write.

If you have not installed the CLI yet, do
[Step 1 of getting started](../getting-started.md#step-1-install-lightshuttle)
first, then come back.

## Step 1: Scaffold the project

Create an empty directory and move into it:

```sh
$ mkdir onboarding-rust && cd onboarding-rust
```

By the end you will have four files in it:

```text
onboarding-rust/
  Cargo.toml         the crate manifest and dependencies
  src/main.rs        the HTTP service
  Dockerfile         how LightShuttle builds it
  lightshuttle.yml   the stack: Postgres + the service
```

## Step 2: Write the service

The service reads the connection string from `DATABASE_URL`, opens a
Postgres connection on each request, and answers with JSON.
Create `Cargo.toml`:

```toml
[package]
name = "onboarding-rust"
version = "0.1.0"
edition = "2021"

[dependencies]
axum = "0.7"
tokio = { version = "1", features = ["full"] }
tokio-postgres = "0.7"
serde_json = "1"
```

Then create `src/main.rs`:

```rust
use std::env;

use axum::{routing::get, Json, Router};
use serde_json::json;
use tokio_postgres::NoTls;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

async fn root() -> Json<serde_json::Value> {
    match query_now().await {
        Ok(now) => Json(json!({ "db": "ok", "now": now })),
        Err(error) => Json(json!({ "db": "error", "message": error.to_string() })),
    }
}

async fn query_now() -> Result<String, BoxError> {
    let url = env::var("DATABASE_URL")?;
    let (client, connection) = tokio_postgres::connect(&url, NoTls).await?;
    tokio::spawn(async move {
        let _ = connection.await;
    });
    let row = client.query_one("select now()::text as now", &[]).await?;
    Ok(row.get("now"))
}

#[tokio::main]
async fn main() {
    let port = env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let app = Router::new().route("/", get(root));
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}"))
        .await
        .unwrap();
    println!("api listening on {port}");
    axum::serve(listener, app).await.unwrap();
}
```

Two things are worth noting:

- `DATABASE_URL` is never hard-coded. LightShuttle injects it at boot,
  pointing at the database resource. `tokio_postgres::connect` returns a
  pair `(client, connection)`: the `connection` drives the protocol wire
  and must be polled to completion, so we hand it to `tokio::spawn` as a
  background task. The `client` is then free to issue queries while the
  connection task runs independently. We use `select now()::text` to
  fetch the timestamp as a plain string, avoiding any dependency on a
  date library such as `chrono`.
- The `main` function reads `PORT` with a fallback of `8080`, so
  the same binary runs locally or inside a container without changes.

## Step 3: Write the Dockerfile

LightShuttle builds the service from this `Dockerfile`. The build uses
two stages: the full `rust:1.83-slim` image compiles a release binary,
then only that binary is copied into a minimal `debian:bookworm-slim`
image. Because `tokio-postgres` uses `NoTls` (plain TCP on the private
Docker network), no OpenSSL runtime is needed in the final image, keeping
it small and the dependency surface narrow:

```dockerfile
FROM rust:1.83-slim AS build
WORKDIR /src
COPY Cargo.toml ./
COPY src ./src
RUN cargo build --release

FROM debian:bookworm-slim AS dev
COPY --from=build /src/target/release/onboarding-rust /bin/api
EXPOSE 8080
CMD ["/bin/api"]
```

The manifest will select the `dev` stage through `target: dev`. A real
project would tag a separate `release` stage from the same build; here
one final stage is enough.

## Step 4: Write the manifest

Now tie the two resources together. Create `lightshuttle.yml`:

```yaml
# yaml-language-server: $schema=https://raw.githubusercontent.com/nubster-opensources/lightshuttle/main/docs/spec/manifest-v0.schema.json
project:
  name: onboarding-rust

resources:
  db:
    postgres:
      version: "16"

  api:
    dockerfile:
      context: .
      target: dev
      env:
        DATABASE_URL: ${resources.db.url}
      ports:
        - 8080
```

What each part does:

- `db` is a Postgres 16 instance. LightShuttle expands it to the
  official `postgres:16-alpine` image, generates a password and binds a
  persistent volume.
- `api` is built from the `Dockerfile` in the current directory
  (`context: .`), selecting the `dev` stage.
- `env.DATABASE_URL` is set to `${resources.db.url}`. That interpolation
  resolves at boot to the full Postgres URL of `db`, and it also makes
  `api` depend on `db`: the service will not start until the database is
  healthy. No explicit `depends_on` is needed.
- `ports: [8080]` publishes the container port on your host so you can
  reach the service from a browser or `curl`.

## Step 5: Boot the stack

Validate first. This parses the manifest and resolves interpolations
without touching Docker:

```sh
$ lightshuttle validate
ok: project `onboarding-rust` with 2 resource(s)
```

Then boot:

```sh
$ lightshuttle up
```

The first `up` builds the image. Because `cargo build --release` runs
inside the build stage, the Docker layer cache downloads and compiles
all crates on the first run, which takes noticeably longer than
interpreted stacks. Subsequent builds reuse the cache unless `Cargo.toml`
changes. You will see the database come up, then the service:

```text
project `onboarding-rust`: starting 2 resource(s)
db: starting
db: healthy
api: building
api: starting
api: running
LightShuttle dashboard ready at http://127.0.0.1:54321/
```

`up` stays in the foreground supervising the stack until you press
`Ctrl+C`. Leave it running and open a second terminal for the next step.

## Step 6: Observe

List what is running:

```sh
$ lightshuttle ps
NAME  KIND        STATUS   READY  IMAGE
db    postgres    running  yes    postgres:16-alpine
api   dockerfile  running  yes    onboarding-rust-api
```

Call the service:

```sh
$ curl http://localhost:8080/
{"db":"ok","now":"2026-06-12 09:41:08.512306+00"}
```

The `now` value comes straight from Postgres: the request reached your
Rust service, which opened a connection, issued `select now()::text`, and
serialised the answer as JSON. Stream its logs to confirm:

```sh
$ lightshuttle logs api
api listening on 8080
```

Add `--follow` (or `-f`) to keep tailing.

## Step 7: Visit the dashboard

The boot log printed a dashboard URL
(`http://127.0.0.1:54321/` above; your port will differ). Open it in a
browser. The index lists both resources with a live status that refreshes
every two seconds, and each row links to a detail page with a streaming
log pane.

For a full tour of every dashboard page, see the
[dashboard walkthrough](../dashboard.md).

## Step 8: Shut down

Back in the first terminal, press `Ctrl+C`. LightShuttle stops the
resources in reverse order, giving each container ten seconds to exit
cleanly. If anything is left over, run:

```sh
$ lightshuttle down
stopped: api
stopped: db
```

`down` is idempotent: a second run prints
`nothing to stop for project onboarding-rust`.

## What's next

- Add a secret with `${env.<NAME>}` and a `.env` file, as shown in
  [Step 7 of getting started](../getting-started.md#step-7-secrets-from-a-env-file).
- Try the same exercise in another stack:
  [Node.js](nodejs.md), [Python](python.md) or [Go](go.md).
- Generate deployment artifacts from this manifest with the
  [export tutorial](../export.md).
- Read the [manifest specification][spec] for every supported field.

[spec]: https://github.com/nubster-opensources/lightshuttle/blob/main/docs/spec/manifest-v0.md
