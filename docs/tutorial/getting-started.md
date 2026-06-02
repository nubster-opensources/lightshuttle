# Getting started with LightShuttle

This tutorial takes about ten minutes. By the end you will have booted a
two-service stack on your laptop with a single command, observed it
through the CLI, shut it down cleanly and extended it with a second
backing service. No prior Rust knowledge is required; basic familiarity
with Docker is assumed.

> **Pre-1.0.** LightShuttle is published and works end to end on a real
> Docker daemon, but the public API may still change between minor
> versions. See the [SemVer policy](../SEMVER_POLICY.md).

A reminder of what LightShuttle is **not**, so the rest of the tutorial
is read with the right expectations:

- Not a production runtime.
- Not a Kubernetes replacement.
- Not a service mesh.
- Not a CI/CD pipeline.

LightShuttle is the local stack runner you reach for instead of
`docker-compose` while you are coding. When you are ready to ship,
[`lightshuttle export`](export.md) turns the same manifest into a
`docker-compose.yml`, Kubernetes manifests or a Helm chart.

## Prerequisites

You need:

- A running Docker daemon. Docker Desktop on macOS or Windows works out
  of the box; on Linux any modern Docker Engine or `colima` works.
- A Rust toolchain. The recommended way to install it is
  [`rustup`](https://rustup.rs/). LightShuttle's MSRV is documented in
  [`docs/MSRV_POLICY.md`](../MSRV_POLICY.md).
- A terminal. Examples below use a POSIX-style shell. On Windows,
  PowerShell works fine; replace the line continuation backticks if you
  copy-paste multi-line commands.

Verify Docker is reachable:

```sh
$ docker version --format '{{.Server.Version}}'
27.3.1
```

If that command fails, start Docker before continuing.

## Step 1: Install LightShuttle

Install the CLI from crates.io:

```sh
$ cargo install lightshuttle
```

Cargo compiles the binary in release mode and drops it in
`~/.cargo/bin/lightshuttle`. Confirm the install:

```sh
$ lightshuttle --version
lightshuttle 0.2.0
```

If `lightshuttle` is not found, make sure `~/.cargo/bin` is on your
`PATH`.

## Optional: shell alias `lsh`

Typing `lightshuttle` twelve times an hour gets old. A short alias
makes the binary feel like a native command. We deliberately did
**not** ship `lsh` as the default binary name to avoid colliding with
the legacy [GNU lsh][gnu-lsh] SSH client still packaged on some Linux
distributions, but you can opt in if your environment is free of it.

### Automatic setup (recommended)

`lightshuttle alias install` detects your shell, refuses to run when a
conflicting `lsh` executable is on your `PATH`, and writes the alias to
the right startup file:

```sh
$ lightshuttle alias install
Detected shell: zsh
Will add `alias lsh='lightshuttle'` to /home/you/.zshrc
Proceed? [y/N]: y
ok: added `lsh` alias. Restart your shell or reload /home/you/.zshrc
```

It is idempotent, so re-running it is a no-op. Companion commands:

- `lightshuttle alias check` reports what `install` would do without
  writing anything.
- `lightshuttle alias uninstall` removes the alias.
- `--shell <bash|zsh|fish|powershell>` overrides auto-detection and
  `--yes` skips the prompt for scripts and CI.

`cmd.exe` has no startup file, so there it stays manual; use PowerShell
or the `.bat` shim described below.

The rest of this section documents the manual procedure for reference.

[gnu-lsh]: https://www.lysator.liu.se/~nisse/lsh/

### Check availability

Before adding the alias, confirm nothing else owns `lsh` on your
machine:

```sh
# Linux / macOS
$ command -v lsh
# Windows PowerShell
PS> Get-Command lsh -ErrorAction SilentlyContinue
```

If the command prints a path (typically `/usr/bin/lsh` or similar),
something else is already there. Stick with the full `lightshuttle`
name to avoid silent confusion.

If the command returns nothing, you are clear to alias.

### Add the alias

**bash or zsh** — append to `~/.bashrc` or `~/.zshrc`:

```sh
alias lsh='lightshuttle'
```

Reload with `source ~/.bashrc` (or open a new terminal).

**fish** — once per shell session, or persisted with `funcsave`:

```fish
alias --save lsh='lightshuttle'
```

**PowerShell** — append to your profile (`$PROFILE`):

```powershell
Set-Alias -Name lsh -Value lightshuttle
```

Open a new PowerShell window to pick it up.

**Windows `cmd.exe`** has no native alias mechanism for executables.
Either use PowerShell, or drop a one-line `lsh.bat` shim somewhere on
your `PATH`:

```bat
@echo off
lightshuttle %*
```

### Verify

```sh
$ lsh --version
lightshuttle 0.2.0
```

If anything in this tutorial reads `lightshuttle`, you can substitute
`lsh` from this point on.

## Step 2: Your first manifest

Create a fresh directory and an empty `lightshuttle.yml` next to it.

```sh
$ mkdir hello-lightshuttle && cd hello-lightshuttle
```

Open `lightshuttle.yml` in your editor and paste:

```yaml
# yaml-language-server: $schema=https://raw.githubusercontent.com/nubster-opensources/lightshuttle/main/docs/spec/manifest-v0.schema.json
project:
  name: hello

resources:
  db:
    postgres:
      version: "16"

  app:
    container:
      image: alpine:3.20
      command: ["sh", "-c", "echo connected to $DATABASE_URL && sleep 3600"]
      env:
        DATABASE_URL: ${resources.db.url}
```

Line by line:

- The `yaml-language-server` modeline points editors at the JSON Schema
  shipped with the spec. With it, Visual Studio Code, IntelliJ IDEs and
  neovim provide autocompletion and inline validation. It is optional
  but recommended.
- `project.name` identifies the stack. The orchestrator uses it as a
  prefix for every container it creates, so two LightShuttle projects
  never collide.
- The `resources` section is a map of resource names to resource
  definitions. Each entry has exactly one *kind* key (`postgres`,
  `redis`, `container`, `dockerfile`).
- `db` is a Postgres 16 instance. With no further configuration, the
  runtime expands `version: "16"` into the official `postgres:16-alpine`
  image, generates a random password and binds an auto-named persistent
  volume.
- `app` is a plain container based on `alpine:3.20`. Its `env` block
  uses the interpolation form `${resources.db.url}`, which the
  orchestrator resolves at boot to the full Postgres URL of the `db`
  resource. That reference also creates an implicit dependency: `app`
  will not start until `db` is healthy.

For the full grammar see [the manifest specification][spec].

## Step 3: Boot the stack

LightShuttle exposes three commands you typically chain while
iterating on a manifest:

```sh
$ lightshuttle validate
ok: project `hello` with 2 resource(s)
```

`validate` parses the file, resolves every interpolation and checks the
dependency graph without touching Docker. Use `--strict` in continuous
integration to upgrade warnings to errors.

```sh
$ lightshuttle manifest
```

`manifest` prints the fully resolved YAML to stdout: defaults are
materialised, interpolations are expanded with the values that will be
used at runtime. It is the source of truth when you debug "why did my
container get *that* environment variable".

```sh
$ lightshuttle up
```

`up` boots the stack:

1. The manifest is validated.
2. Resources are started in topological order. `db` starts first.
3. The orchestrator polls the Postgres healthcheck until it succeeds.
4. `app` starts, with `DATABASE_URL` injected and pointing at `db`.
5. The process stays in the foreground, supervising containers, until
   you press `Ctrl+C`.

You will see lines similar to:

```
project `hello`: starting 2 resource(s)
db: starting
db: healthy
app: starting
app: running
```

## Step 4: Observe

In a second terminal, list what is running:

```sh
$ lightshuttle ps
NAME  KIND       STATUS   READY  IMAGE
db    postgres   running  yes    postgres:16-alpine
app   container  running  yes    alpine:3.20
```

Stream the application's logs:

```sh
$ lightshuttle logs app
connected to postgres://postgres:<generated>@db:5432/db
```

Add `--follow` (or `-f`) to keep tailing.

## Step 5: Shutdown

Back in the first terminal, press `Ctrl+C`. LightShuttle sends
`SIGTERM` in reverse topological order, gives each container ten
seconds to exit cleanly, then escalates to `SIGKILL` if needed.

If anything is left over (for example you closed the laptop), run:

```sh
$ lightshuttle down
stopped: app
stopped: db
```

`down` is idempotent: running it a second time prints
`nothing to stop for project hello`.

## Step 6: Multi-resource stack

Real applications need more than one backing service. Extend the
manifest with a Redis cache:

```yaml
project:
  name: hello

resources:
  api_db:
    postgres:
      version: "16"

  cache:
    redis:
      version: "7"

  app:
    container:
      image: alpine:3.20
      command:
        - sh
        - -c
        - |
          echo "db   = $DATABASE_URL"
          echo "redis= $REDIS_URL"
          echo "db host (auto) = $LSH_API_DB_HOST"
          sleep 3600
      env:
        DATABASE_URL: ${resources.api_db.url}
        REDIS_URL: ${resources.cache.url}
```

Boot it:

```sh
$ lightshuttle up
project `hello`: starting 3 resource(s)
api_db: starting
cache: starting
api_db: healthy
cache: healthy
app: starting
app: running
```

`api_db` and `cache` start in parallel because they have no dependency
between them. `app` waits for both before starting.

### Two ways to consume a resource

The `app` container reads three environment variables. Two of them are
declared **explicitly** in the manifest via interpolation:

- `DATABASE_URL` from `${resources.api_db.url}`.
- `REDIS_URL` from `${resources.cache.url}`.

The third, `LSH_API_DB_HOST`, is injected **automatically** by the
runtime. For every dependency, LightShuttle exposes each property of
the dependency as an environment variable named
`LSH_<DEP>_<PROPERTY>`, upper-cased. With `api_db` as a dependency, the
container therefore receives:

| Variable | Source |
|---|---|
| `LSH_API_DB_HOST` | `${resources.api_db.host}` |
| `LSH_API_DB_PORT` | `${resources.api_db.port}` |
| `LSH_API_DB_DATABASE` | `${resources.api_db.database}` |
| `LSH_API_DB_USER` | `${resources.api_db.user}` |
| `LSH_API_DB_PASSWORD` | `${resources.api_db.password}` |
| `LSH_API_DB_URL` | `${resources.api_db.url}` |

The same pattern applies to `cache`: `LSH_CACHE_HOST`,
`LSH_CACHE_PORT`, `LSH_CACHE_URL`, and so on.

Two consumption styles coexist on purpose. Explicit interpolation keeps
your application portable: it reads the standard `DATABASE_URL` that
every language ecosystem already understands. The automatic
`LSH_<DEP>_<PROP>` variables give you a zero-configuration escape hatch
when you want to wire a quick script without editing the manifest.

Shut everything down:

```sh
$ # Ctrl+C in the foreground terminal, then:
$ lightshuttle down
```

## What's next

- Read the [manifest specification][spec] for every supported field,
  resource kind and interpolation rule.
- Explore the [dashboard tutorial](dashboard.md) for the web UI, live
  logs and the OpenTelemetry collector.
- Generate deployment artifacts with the [export tutorial](export.md).
- Browse the [`examples/`](../../examples/) folder for ready-to-run
  manifests.
- Track upcoming features in the [roadmap](../../ROADMAP.md).
- To contribute, read [`CONTRIBUTING.md`](../../CONTRIBUTING.md) and
  [`SECURITY.md`](../../SECURITY.md) first.

[spec]: ../spec/manifest-v0.md
