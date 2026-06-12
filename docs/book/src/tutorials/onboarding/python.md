# Onboarding: Python

This tutorial takes about fifteen minutes. You will build a small
Python HTTP service that queries Postgres, then boot it next to a
database with a single `lightshuttle up`. You do not need Python
installed locally: LightShuttle builds the service inside a container
from the `Dockerfile` you write.

If you have not installed the CLI yet, do
[Step 1 of getting started](../getting-started.md#step-1-install-lightshuttle)
first, then come back.

## Step 1: Scaffold the project

Create an empty directory and move into it:

```sh
$ mkdir onboarding-python && cd onboarding-python
```

By the end you will have four files in it:

```text
onboarding-python/
  app.py             the HTTP service
  requirements.txt   its single dependency
  Dockerfile         how LightShuttle builds it
  lightshuttle.yml   the stack: Postgres + the service
```

## Step 2: Write the service

The service reads the connection string from `DATABASE_URL`, runs one
query on each request, and answers with JSON. Create `app.py`:

```python
import json
import os
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

import psycopg

DATABASE_URL = os.environ["DATABASE_URL"]
PORT = int(os.environ.get("PORT", "8080"))


class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path != "/":
            self.send_response(404)
            self.end_headers()
            self.wfile.write(b"not found\n")
            return
        try:
            with psycopg.connect(DATABASE_URL) as conn:
                row = conn.execute("select now() as now").fetchone()
            body = json.dumps({"db": "ok", "now": row[0].isoformat()})
            self.send_response(200)
        except Exception as error:
            body = json.dumps({"db": "error", "message": str(error)})
            self.send_response(500)
        self.send_header("content-type", "application/json")
        self.end_headers()
        self.wfile.write(body.encode())


if __name__ == "__main__":
    print(f"api listening on {PORT}", flush=True)
    ThreadingHTTPServer(("0.0.0.0", PORT), Handler).serve_forever()
```

Two things are worth noting:

- `DATABASE_URL` is never hard-coded. LightShuttle injects it at boot,
  pointing at the database resource. The same code runs unchanged
  against any Postgres.
- `psycopg.connect` opens one connection per request rather than
  holding a pool at startup, so the service starts even if the
  database needs a moment to become reachable. `row[0]` is a Python
  `datetime`, hence the `.isoformat()` call before serialising.

Declare the one dependency in `requirements.txt`:

```text
psycopg[binary]==3.2.*
```

## Step 3: Write the Dockerfile

LightShuttle builds the service from this `Dockerfile`. A two-stage
layout keeps dependency installation cached separately from your source,
so editing `app.py` does not reinstall `psycopg`:

```dockerfile
FROM python:3.12-slim AS base
WORKDIR /app
COPY requirements.txt ./
RUN pip install --no-cache-dir -r requirements.txt

FROM base AS dev
COPY app.py ./
EXPOSE 8080
CMD ["python", "app.py"]
```

The manifest will select the `dev` stage explicitly through
`target: dev`. A real project would add a leaner `release` stage on top
of the same `base`; here one stage is enough.

## Step 4: Write the manifest

Now tie the two resources together. Create `lightshuttle.yml`:

```yaml
# yaml-language-server: $schema=https://raw.githubusercontent.com/nubster-opensources/lightshuttle/main/docs/spec/manifest-v0.schema.json
project:
  name: onboarding-python

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
ok: project `onboarding-python` with 2 resource(s)
```

Then boot:

```sh
$ lightshuttle up
```

The first `up` builds the image, so it takes a little longer. You will
see the database come up, then the service:

```text
project `onboarding-python`: starting 2 resource(s)
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
api   dockerfile  running  yes    onboarding-python-api
```

Call the service:

```sh
$ curl http://localhost:8080/
{"db":"ok","now":"2026-06-12T09:41:08.512Z"}
```

The `now` value comes straight from Postgres: the request reached your
Python service, which queried the database and serialised the answer.
Stream its logs to confirm:

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
`nothing to stop for project onboarding-python`.

## What's next

- Add a secret with `${env.<NAME>}` and a `.env` file, as shown in
  [Step 7 of getting started](../getting-started.md#step-7-secrets-from-a-env-file).
- Try the same exercise in another stack:
  [Node.js](nodejs.md), [Go](go.md) or [Rust](rust.md).
- Generate deployment artifacts from this manifest with the
  [export tutorial](../export.md).
- Read the [manifest specification][spec] for every supported field.

[spec]: https://github.com/nubster-opensources/lightshuttle/blob/main/docs/spec/manifest-v0.md
