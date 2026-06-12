# Troubleshooting and FAQ

When `lightshuttle up` (or `validate`, or a build) fails, the message
usually tells you which of a handful of common situations you are in.
This page lists those situations in a fixed **symptom, cause, remedy**
shape so you can match the error you see and move on quickly. A short
[FAQ](#faq) at the end answers recurring questions.

Each entry has a stable anchor (for example
[`#docker-daemon-unreachable`](#docker-daemon-unreachable)) so error
messages and other docs can link straight to it.

## Exit codes

Before the individual cases, the exit code already narrows things down:

| Code | Meaning |
|---|---|
| `0` | Success: the stack started and shut down cleanly. |
| `1` | User error: invalid manifest, missing file, failed validation, or a resource that ended in a failed state. |
| `2` | Runtime error: the container runtime is unreachable or a container failed to start or build. |
| `130` | Interrupted with `Ctrl+C` (`SIGINT`). |

A `1` points at your manifest or environment. A `2` points at Docker or
a container.

## Docker daemon unreachable

**Symptom.** Any command that touches containers fails immediately:

```text
Error: failed to connect to the container runtime
```

followed by a lower-level message from the Docker client (for example
`error trying to connect: ... The system cannot find the file specified`
on Windows, or `Cannot connect to the Docker daemon at
unix:///var/run/docker.sock` on Linux). The exit code is `2`.

**Cause.** LightShuttle talks to Docker over its local socket through
`Docker::connect_with_local_defaults`. The daemon is not running, or
your user cannot reach the socket.

**Remedy.** Start Docker and confirm it answers before retrying:

```sh
$ docker version --format '{{.Server.Version}}'
27.3.1
```

On Linux, if the daemon is up but the socket is denied, add your user to
the `docker` group (and start a new session), or use `colima`/rootless
Docker. Then run `lightshuttle up` again.

## Port already in use

**Symptom.** The database or a published service container fails to
start, and the underlying Docker error mentions an address already in
use, for example:

```text
Bind for 0.0.0.0:8080 failed: port is already allocated
```

The exit code is `2`.

**Cause.** A `ports` entry maps a container port onto a host port that
another process (often a previous run that did not shut down, or an
unrelated service) already holds.

**Remedy.** Free the port or publish on a different host port. List what
holds it (`lsof -i :8080` on macOS/Linux,
`Get-NetTCPConnection -LocalPort 8080` on Windows). If it is a stale
LightShuttle run, clear it with `lightshuttle down`. To move the host
port, set the mapping explicitly in the manifest (`"8081:8080"` publishes
container `8080` on host `8081`). Postgres and Redis are reached by other
containers over the private network by name, so you rarely need to
publish them on the host at all.

## A resource never becomes healthy

**Symptom.** Boot stalls on one resource and eventually fails with a
timeout, for example:

```text
db: starting
Error: timed out waiting for `db` to become healthy
```

**Cause.** The resource started but its healthcheck never passed inside
the allotted time. Common reasons: the database is still initialising on
a slow first run, the container is crash-looping, or a custom
`healthcheck` probes the wrong command or port.

**Remedy.** Look at the resource's own logs first:

```sh
$ lightshuttle logs db
```

If it is simply slow to initialise, raise the healthcheck budget on the
resource (`healthcheck.timeout` and `healthcheck.retries`; see the
[manifest specification][spec]). If you wrote a custom healthcheck,
verify the test command succeeds inside the container. A healthcheck
whose `test` is empty is rejected earlier at validation with
`healthcheck.test cannot be empty`.

## Missing required secret

**Symptom.** `up` refuses to boot:

```text
Error: missing required environment variable(s): API_TOKEN
```

The audit command reports the same condition with an actionable hint:

```text
$ lightshuttle secrets check
...
1 required variable(s) not set: API_TOKEN (add them to a .env file or pass --env-file <PATH>)
```

The exit code is `1`.

**Cause.** The manifest references `${env.NAME}` with no default, and
`NAME` is set neither in a `.env` file next to the manifest nor in the
process environment. LightShuttle fails fast rather than booting a
half-configured stack.

**Remedy.** Add the variable to `.env` (and to `.gitignore`), or pass
`--env-file <path>`. Make the reference optional with a default if the
stack can run without it: `${env.NAME:-fallback}`. Use
`lightshuttle secrets check` as a cheap pre-boot or CI gate. The full
workflow is covered in
[Manage secrets and environment variables](secrets-and-env.md).

## Image build fails (BuildKit)

**Symptom.** A `dockerfile` resource fails during its build step:

```text
api: building
Error: image build failed: <output from the failing build step>
```

You may also see `failed to build image from Dockerfile` (the build
could not be started at all) or `failed to build tar archive: ...` (the
build context could not be packaged). The exit code is `2`.

**Cause.** The build context or `Dockerfile` is wrong: a missing file in
`context`, a failing `RUN` step, an unreachable base image, or a
`target` that does not name a real stage.

**Remedy.** Reproduce the build directly to get BuildKit's full output:

```sh
$ docker build --target dev .
```

Fix what `docker build` reports, then retry `lightshuttle up`. Check
that `context` points at the directory holding the files the `Dockerfile`
copies, and that `target` matches a stage name (`AS <name>`) actually
declared in the file.

## Dependency cycle rejected

**Symptom.** `validate` (and therefore `up`) refuses the manifest:

```text
Error: dependency cycle detected: a -> b -> a
```

The exit code is `1`.

**Cause.** Two or more resources depend on each other, directly or
transitively, so there is no order in which they can start. The
dependency can be an explicit `depends_on` or an implicit one created by
an `${resources.*}` interpolation. For example this is a cycle:

```yaml
resources:
  a:
    container:
      image: alpine:3.20
      depends_on: [b]
  b:
    container:
      image: alpine:3.20
      depends_on: [a]
```

**Remedy.** Break the cycle. One of the two resources almost always does
not really need the other at startup: drop that edge, or introduce a
third resource both depend on. LightShuttle starts resources in
dependency order, so the graph must be acyclic.

## Unknown resource reference

**Symptom.** `validate` rejects the manifest with:

```text
Error: unknown resource reference: `cache` (depended on by `api`)
```

The exit code is `1`.

**Cause.** A `depends_on` entry or an `${resources.NAME.*}` interpolation
names a resource that does not exist, usually a typo or a renamed
resource.

**Remedy.** Make the name match a key under `resources` exactly. Resource
names are case sensitive.

## OpenTelemetry collector issues

**Symptom.** Traces or metrics never reach your collector, but the stack
otherwise runs. At startup you may see a warning rather than an error:

```text
OTel tracer init failed; continuing without self-tracing
```

and on shutdown, occasionally:

```text
lightshuttle-otel: tracer flush on shutdown failed: <error>
```

**Cause.** Self-tracing is best effort by design. If the OTLP exporter
cannot be built or the collector is unreachable, LightShuttle logs a
warning and keeps running: it never fails `up` over telemetry, and spans
emitted while the collector is down are dropped silently. The collector
listens on loopback only, OTLP gRPC on `127.0.0.1:4317` and OTLP HTTP on
`127.0.0.1:4318`.

**Remedy.** Confirm the collector resource is healthy
(`lightshuttle ps`) and that your application points at the right
endpoint (`4317` for gRPC, `4318` for HTTP, on `127.0.0.1`). Because the
collector carries no healthcheck, a crash shows only as a stopped
container, so check its logs with `lightshuttle logs <collector>`. The
end-to-end setup is covered in
[Collect traces and metrics locally](observability.md).

## FAQ

**Do I need to know Rust to use LightShuttle?**
No. The CLI is distributed as a binary and your services run in
containers built from your own `Dockerfile`. The
[onboarding tutorials](../tutorials/onboarding/index.md) show the same
workflow for Node.js, Python, Go and Rust.

**Is this a `docker-compose` replacement?**
No. LightShuttle is the local stack runner you reach for while coding;
it is not a production runtime, a Kubernetes replacement or a service
mesh. When you ship, [`lightshuttle export`](../tutorials/export.md)
turns the same manifest into Compose, Kubernetes manifests or a Helm
chart.

**Can I run two projects at the same time?**
Yes. Each project runs on its own Docker network named after
`project.name`, so stacks stay isolated and do not collide. See
[Reach one resource from another](networking-and-discovery.md).

**Where does my Postgres data live between runs?**
A Postgres resource binds an auto-named persistent volume, so data
survives `down` and a later `up`. Remove the volume with the Docker CLI
if you want a clean database.

**Why is the dashboard on a different port every time?**
`up` picks a random free loopback port by default and prints it. Pin it
by setting `dashboard.port` in the manifest. See the
[dashboard walkthrough](../tutorials/dashboard.md).

**Can I use LightShuttle in production?**
No. Use it for local development, then generate deployment artifacts with
[`lightshuttle export`](../tutorials/export.md) and run those on your
production platform.

**Does it work on Windows?**
Yes. Docker Desktop on Windows works out of the box. Examples in the docs
use a POSIX shell; PowerShell equivalents are noted where they differ.

[spec]: https://github.com/nubster-opensources/lightshuttle/blob/main/docs/spec/manifest-v0.md
