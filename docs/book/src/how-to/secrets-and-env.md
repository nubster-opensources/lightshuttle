# Manage secrets and environment variables

Your manifest is committed to version control; your secrets are not. This
guide shows how to feed credentials and other environment values into a
stack without ever writing them into `lightshuttle.yml`, how to audit what
a stack needs before it boots, and how to read the diagnostics when
something is missing.

It assumes you have already booted a stack once. If you have not, start
with the [getting started tutorial](../tutorials/getting-started.md), which
introduces secrets at the end. For the exhaustive command surface, see the
[`secrets` CLI reference](../reference/cli/secrets.md).

The one rule to remember: a `${env.<NAME>}` reference resolves from two
sources, in this order of precedence.

1. A dotenv file (`.env` in the working directory by default).
2. The process environment.

When both define the same name, **the dotenv file wins**. A value set to an
empty string counts as unset.

The examples below use the runnable
[`examples/05-secrets`](https://github.com/nubster-opensources/lightshuttle/tree/main/examples/05-secrets)
project. Clone it to follow along, or copy the manifest into a fresh
directory.

## Require a secret

Use `${env.<NAME>}` with no default for a value the stack cannot run
without. If it resolves to nothing, `lightshuttle up` refuses to boot and
names the variable, so a misconfigured stack fails fast instead of
half-starting.

```yaml
# yaml-language-server: $schema=https://raw.githubusercontent.com/nubster-opensources/lightshuttle/main/docs/spec/manifest-v0.schema.json
project:
  name: secrets-demo
  description: "Secrets from a .env file: required and optional references"

resources:
  db:
    postgres:
      version: "16"
      # Required: `up` refuses to boot while DEMO_DB_PASSWORD is unset.
      password: ${env.DEMO_DB_PASSWORD}

  app:
    container:
      image: alpine:3.20
      command: ["sh", "-c", "echo token=$API_TOKEN && sleep 3600"]
      secrets:
        DATABASE_URL: ${resources.db.url}
        # Optional: falls back to `dev-token` when unset.
        API_TOKEN: ${env.DEMO_API_TOKEN:-dev-token}
```

Provide the value through a `.env` file next to the manifest, and make sure
that file is ignored by git:

```sh
$ echo 'DEMO_DB_PASSWORD=local-dev-password' > .env
$ echo '.env' >> .gitignore
```

Boot as usual; `DEMO_DB_PASSWORD` is now injected into the Postgres
resource:

```sh
$ lightshuttle up
```

## Make a secret optional

Append `:-<default>` to give a reference a fallback. The form
`${env.DEMO_API_TOKEN:-dev-token}` resolves to `dev-token` whenever the
variable is unset or empty, so the stack always boots, and a developer can
override it locally without touching the manifest:

```yaml
      secrets:
        API_TOKEN: ${env.DEMO_API_TOKEN:-dev-token}
```

Use `secrets:` instead of `env:` for every sensitive value consumed by a
container. Lightshuttle injects both maps into the process environment at
runtime, but export commands replace `secrets:` values with deployment-time
placeholders instead of writing their resolved contents to disk. A key cannot
appear in both maps on the same resource.

### What each export target writes

The placeholder differs per target, because each one has its own way of
supplying a value at deployment time:

| Target | Emitted value | How to supply the real value |
| --- | --- | --- |
| Compose | `${RESOURCE_KEY}` | Exported in the shell, or set in a `.env` file next to the generated `docker-compose.yml` |
| Kubernetes | `$(KEY)` in `command`/`args`, `'***'` in the `Secret` | Replaced before `kubectl apply`, or the `Secret` is managed out of band |
| Helm | `$(KEY)` in `command`/`args`, `'***'` in `values.yaml` | Overridden with `--set` or a private values file |

Because Compose emits `${RESOURCE_KEY}` rather than the resolved value, a
generated `docker-compose.yml` is no longer self contained: `docker compose
up` needs those variables to be present in its environment. Compose
substitutes an unset variable with an empty string, so supply every key the
export declares rather than relying on the service to fail loudly.

### Compose variable names are scoped to their resource

Compose interpolates every service from one project-wide `.env`, so a
reference named after the environment key alone would collapse every
resource that happens to share that key onto a single value: two `postgres`
resources both declaring `POSTGRES_PASSWORD` would receive whichever value
`docker compose` resolved first, silently.

The export avoids this by scoping every Compose reference to the resource
that declared it: `<RESOURCE>_<KEY>`, with the resource name normalised into
a shell-safe identifier (uppercased, every character outside `[A-Z0-9_]`
replaced with `_`). A resource named `db` with a `POSTGRES_PASSWORD` key
therefore exports `${DB_POSTGRES_PASSWORD}`, and a resource named `web-cache`
with a `REDIS_PASSWORD` key exports `${WEB_CACHE_REDIS_PASSWORD}`. The key
written inside the container (the left-hand side in Compose's `environment:`
block) keeps the name the image expects; only the interpolation reference is
qualified.

This normalisation cannot be proven collision free: `web-cache` and
`web_cache` both normalise to `WEB_CACHE`. Rather than silently hand two
resources the same reference, `lightshuttle export compose` refuses the
export and names every resource and key sharing the colliding variable. The
same holds within one resource: `db_password` and `DB_PASSWORD` declared side
by side normalise alike and are refused too.

**Migrating an existing `.env`.** If you already export to Compose, rename
every secret variable in your `.env` (and in any pipeline or secret store
that populates it) from the bare key to its resource-qualified form, for
example `POSTGRES_PASSWORD` becomes `DB_POSTGRES_PASSWORD` for a resource
named `db`. Re-run `lightshuttle export compose` and read the generated
`docker-compose.yml` to confirm the exact names your stack now expects.

Note that a value derived from another resource, such as
`${resources.db.url}`, is also replaced once it is declared under `secrets:`.
This is deliberate, since that URL carries credentials, but it does mean the
exported stack expects the value to be provided rather than recomputed.

### Redis's own password travels through `REDIS_PASSWORD`

A `redis` resource with a `password` set no longer writes that password
straight into the container's `command` as a `--requirepass <value>`
argument. The password is written to the `REDIS_PASSWORD` environment key
instead, marked sensitive the same way an explicit `secrets:` entry is, and
the command references that key rather than carrying the value itself:

```yaml
resources:
  cache:
    redis:
      version: "7"
      password: ${env.DEMO_REDIS_PASSWORD}
```

Exported, the redis service's command becomes `redis-server --requirepass
$(REDIS_PASSWORD)` on Kubernetes and Helm, and
`redis-server --requirepass ${CACHE_REDIS_PASSWORD}` on Compose (following
the same per-resource scoping described above). A `redis` resource with no
password declared still gets no `--requirepass` flag and no `REDIS_PASSWORD`
key at all.

**The accepted limit.** The container still runs `redis-server
--requirepass <value>` with the real password as a process argument. The
value is substituted at the last moment: by `lightshuttle up` immediately
before `docker create`, by Compose when it starts the service, by the
kubelet when it starts the container. From then on it is readable by anyone
who can see that process: a `ps` inside the container, a `ps` on the machine
that runs it, `docker inspect` on that container locally, or the node itself
on Kubernetes. What this closes is the value's presence everywhere else: the
exported Compose file, Kubernetes manifest and Helm chart, the pod spec
served by the cluster API, and your repository all stay free of the
plain-text password.

### A credential travels with the value, not with the key name

You do not have to declare such a key under `secrets:` for it to be
redacted. A reference to a resource property that carries a credential,
today `password` and `url`, makes the environment key receiving it a secret
on every target:

```yaml
resources:
  api:
    container:
      image: alpine:3.20
      env:
        DATABASE_URL: "${resources.main_db.url}"
```

`DATABASE_URL` matches none of the name markers listed above, yet the URL it
receives embeds the database password. The export marks the key as a secret
because of where its value came from, so Compose writes the resource-scoped
`${API_DATABASE_URL}` (see above), and Kubernetes and Helm write `'***'` in
their `Secret`.

The same reference **outside** an environment value is refused outright:

```yaml
      command: ["connect", "${resources.main_db.password}"]
```

An argv array has nowhere safe to hide a value: there is no `Secret` object
for it and no `${KEY}` indirection. Rather than emit the password in clear
text, the export fails and names the resource, the field and the reference.

Precedence still applies: a value in the `.env` file overrides the default,
and a value in the `.env` file also overrides the same name in the process
environment.

To emit a literal `${...}` instead of interpolating it, double the braces:
`${{not.interpolated}}` renders the string `${not.interpolated}` verbatim.

### A non-secret `${env.*}` reference survives to deployment time

A reference that carries no credential is not resolved at export time
either: the value belongs to the machine that deploys, not to the machine
that exports. Each target keeps it in its own syntax.

Compose keeps its own interpolation, so `image: "example/api:${env.TAG:-1.0}"`
is exported as `example/api:${TAG:-1.0}` and `docker compose up` resolves it.

Helm publishes every referenced variable under a top-level `variables:` key
of `values.yaml`, empty by default:

```yaml
variables:
  TAG: ''
```

The template then reads `{{ .Values.variables.TAG | default "1.0" }}` when
the manifest gave a default, and
`{{ required "variables.TAG is required" .Values.variables.TAG }}` when it
did not. Sprig treats an empty value as absent, which is exactly the `:-`
semantics of the manifest. Override one at deploy time:

```sh
helm template ./export/helm --set variables.TAG=2.0
```

Plain Kubernetes substitutes nothing at deploy time. A reference **with** a
default is therefore frozen to that default in the manifest, and a reference
**without** one makes the export fail, listing every such variable at once
rather than the first one found.

## Audit before boot with `secrets check`

`lightshuttle secrets check` reports every `${env.*}` reference the
manifest contains, with its status and source, without starting anything:

```sh
$ lightshuttle secrets check
secrets for project `secrets-demo`:

  DEMO_DB_PASSWORD                 set (.env)
  DEMO_API_TOKEN                   default (dev-token)

all required secrets are set
```

The status column tells you exactly where each value comes from:

| Status | Meaning |
|---|---|
| `set (.env)` | resolved from the dotenv file |
| `set (env)` | resolved from the process environment |
| `default (...)` | unset, falling back to the declared default |
| `missing` | unset, and at least one reference has no default |

When at least one variable is `missing`, the command exits non-zero.

> **`validate` does not check secrets.** `lightshuttle validate` parses the
> manifest, resolves `${resources.*}` references and checks the dependency
> graph, but it deliberately does **not** resolve `${env.*}` values. Use
> `secrets check` to audit secrets, and rely on the fail-fast preflight of
> `up` as the final guard. The two read from the same engine, so
> `secrets check` predicts what `up` will accept.

## Point at another `.env` file

Both `up` and `secrets check` accept `--env-file <path>` to read from a
file other than `.env`. This is how you keep one set of values per
environment, for example a `.env.ci` checked against in a pipeline:

```sh
$ lightshuttle secrets check --env-file .env.ci
```

The implicit `.env` is loaded only when it exists and is silently skipped
when absent. A file passed with `--env-file` is explicit, so it **must**
exist; the command errors if it does not.

## Diagnose a failed boot

When `up` aborts at the preflight, it lists every required variable that
resolved to nothing. Reproduce the same diagnosis without a Docker daemon
by running `secrets check`, which uses the identical resolution engine:

```sh
$ rm .env
$ lightshuttle secrets check
secrets for project `secrets-demo`:

  DEMO_DB_PASSWORD                 missing
  DEMO_API_TOKEN                   default (dev-token)

```

`DEMO_DB_PASSWORD` is `missing` and the command exits non-zero. Restore the
value (in `.env`, in the process environment, or via `--env-file`) and the
check passes again. Because `check` needs no container runtime, it is the
fastest way to confirm a fix before re-running `up`.

## Spot a divergent default

The same variable can be referenced in several places. When two references
declare **different** defaults, that is almost always a mistake: the value
your stack uses then depends on which resource reads it first. `secrets
check` surfaces this by listing every distinct default it saw, sorted and
joined with ` | `:

```yaml
project:
  name: divergent-demo

resources:
  app:
    container:
      image: alpine:3.20
      env:
        LOG_A: ${env.LOG_LEVEL:-info}
        LOG_B: ${env.LOG_LEVEL:-debug}
```

```sh
$ lightshuttle secrets check
secrets for project `divergent-demo`:

  LOG_LEVEL                        default (debug | info)

all required secrets are set
```

A single default in the parentheses is normal. Two or more is a signal:
pick one value and make every reference agree, or set `LOG_LEVEL`
explicitly so the defaults no longer matter.

## Gate a CI pipeline on secrets

Because `secrets check` exits non-zero when a required variable is missing,
it doubles as a cheap pipeline gate. Run it against the environment file
the pipeline provides, and the job fails before any container starts:

```sh
$ lightshuttle secrets check --env-file .env.ci
```

Pair it with `lightshuttle validate --strict` to catch both structural
manifest errors and missing secrets in the same stage.
