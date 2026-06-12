# Onboarding: Go

This tutorial takes about fifteen minutes. You will build a small Go
HTTP service that queries Postgres, then boot it next to a database with
a single `lightshuttle up`. You do not need Go installed locally:
LightShuttle builds the service inside a container from the `Dockerfile`
you write.

If you have not installed the CLI yet, do
[Step 1 of getting started](../getting-started.md#step-1-install-lightshuttle)
first, then come back.

## Step 1: Scaffold the project

Create an empty directory and move into it:

```sh
$ mkdir onboarding-go && cd onboarding-go
```

By the end you will have four files in it:

```text
onboarding-go/
  main.go              the HTTP service
  go.mod               its single dependency
  Dockerfile           how LightShuttle builds it
  lightshuttle.yml     the stack: Postgres + the service
```

## Step 2: Write the service

The service reads the connection string from `DATABASE_URL`, runs one
query on each request, and answers with JSON. Create `main.go`:

```go
package main

import (
	"context"
	"encoding/json"
	"log"
	"net/http"
	"os"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
)

func main() {
	pool, err := pgxpool.New(context.Background(), os.Getenv("DATABASE_URL"))
	if err != nil {
		log.Fatalf("connect: %v", err)
	}
	defer pool.Close()

	port := os.Getenv("PORT")
	if port == "" {
		port = "8080"
	}

	http.HandleFunc("/", func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/" {
			http.NotFound(w, r)
			return
		}
		w.Header().Set("content-type", "application/json")
		var now time.Time
		if err := pool.QueryRow(r.Context(), "select now()").Scan(&now); err != nil {
			w.WriteHeader(http.StatusInternalServerError)
			json.NewEncoder(w).Encode(map[string]string{"db": "error", "message": err.Error()})
			return
		}
		json.NewEncoder(w).Encode(map[string]any{"db": "ok", "now": now})
	})

	log.Printf("api listening on %s", port)
	log.Fatal(http.ListenAndServe(":"+port, nil))
}
```

Two things are worth noting:

- `DATABASE_URL` is never hard-coded. LightShuttle injects it at boot,
  pointing at the database resource. The same code runs unchanged
  against any Postgres.
- `pgxpool.New` creates a connection pool that connects lazily. The pool
  validates the DSN immediately but opens actual connections on the first
  query, so the service starts even if the database needs a moment to
  become reachable.

Declare the one dependency in `go.mod`:

```text
module onboarding-go

go 1.23

require github.com/jackc/pgx/v5 v5.7.1
```

## Step 3: Write the Dockerfile

LightShuttle builds the service from this `Dockerfile`. A two-stage
layout separates the compile step from the final image, keeping the
runtime image small:

```dockerfile
FROM golang:1.23-alpine AS build
WORKDIR /src
COPY go.mod ./
COPY main.go ./
RUN go mod tidy && go build -o /bin/api .

FROM alpine:3.20 AS dev
COPY --from=build /bin/api /bin/api
EXPOSE 8080
CMD ["/bin/api"]
```

The `build` stage copies `main.go` before running `go mod tidy`. That
order matters: `tidy` reads the import paths in the source to figure out
which modules are actually used. Because this tutorial does not commit a
`go.sum` file, `tidy` (which has network access during the build)
resolves and locks the dependency before `go build`. A real project
would commit both `go.mod` and `go.sum` and replace `go mod tidy` with
`go mod download`.

The manifest selects the `dev` stage explicitly through `target: dev`.
A real project would add a leaner `release` stage on top of the same
`build`; here one stage is enough.

## Step 4: Write the manifest

Now tie the two resources together. Create `lightshuttle.yml`:

```yaml
# yaml-language-server: $schema=https://raw.githubusercontent.com/nubster-opensources/lightshuttle/main/docs/spec/manifest-v0.schema.json
project:
  name: onboarding-go

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
ok: project `onboarding-go` with 2 resource(s)
```

Then boot:

```sh
$ lightshuttle up
```

The first `up` compiles the Go binary inside the container, so it takes
a little longer. You will see the database come up, then the service:

```text
project `onboarding-go`: starting 2 resource(s)
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
api   dockerfile  running  yes    onboarding-go-api
```

Call the service:

```sh
$ curl http://localhost:8080/
{"db":"ok","now":"2026-06-12T09:41:08.512306Z"}
```

The `now` value comes straight from Postgres: the request reached your
Go service, which queried the database and serialised the answer. Stream
its logs to confirm:

```sh
$ lightshuttle logs api
api listening on 8080
```

Add `--follow` (or `-f`) to keep tailing.

## Step 7: Visit the dashboard

The boot log printed a dashboard URL
(`http://127.0.0.1:54321/` above; your port will differ). Open it in a
browser. The index lists both resources with a live status that
refreshes every two seconds, and each row links to a detail page with a
streaming log pane.

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
`nothing to stop for project onboarding-go`.

## What's next

- Add a secret with `${env.<NAME>}` and a `.env` file, as shown in
  [Step 7 of getting started](../getting-started.md#step-7-secrets-from-a-env-file).
- Try the same exercise in another stack:
  [Node.js](nodejs.md), [Python](python.md) or [Rust](rust.md).
- Generate deployment artifacts from this manifest with the
  [export tutorial](../export.md).
- Read the [manifest specification][spec] for every supported field.

[spec]: https://github.com/nubster-opensources/lightshuttle/blob/main/docs/spec/manifest-v0.md
