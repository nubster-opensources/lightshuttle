# LightShuttle Manifest Specification, version `v0`

This document specifies the `lightshuttle.yml` manifest format consumed by
LightShuttle v0.1.0. It is the contract between the developer who writes
the manifest and the orchestrator that interprets it.

## Status

`v0` is **pre-stable**. It will be frozen when LightShuttle ships v0.1.0
and any later breaking change will produce a `v1` specification published
side by side with this one. Until v0.1.0 ships, every section of this
document may evolve.

## Scope

LightShuttle is a **developer-time orchestrator**. The manifest describes
the resources that make up a development stack: databases, caches,
containers, locally built images, optional native processes. The same
manifest is later transformed into production artefacts
(`docker-compose.yml`, Kubernetes resources, Helm chart) by the
`lightshuttle export` command.

The following are explicitly out of scope of this manifest, regardless of
demand:

- Production runtime concerns (autoscaling, rollout strategies,
  control plane configuration).
- Service mesh primitives (sidecars, mTLS, traffic policies).
- CI/CD pipeline declarations.

## Document conventions

The key words **MUST**, **MUST NOT**, **SHOULD**, **SHOULD NOT**, **MAY**
and **OPTIONAL** in this document are to be interpreted as described in
[RFC 2119][rfc2119] when, and only when, they appear in all capitals.

[rfc2119]: https://www.rfc-editor.org/rfc/rfc2119

In all YAML examples, comments starting with `#` are informative and not
part of the file format.

## File location and discovery

The manifest file is named `lightshuttle.yml` and lives at the root of the
project that it describes. The CLI **MUST** search for it in the current
working directory first, then walk up parent directories until it finds
one. The first match wins, in the spirit of `cargo` looking for
`Cargo.toml`.

The `--file <path>` global flag overrides the discovery and points the CLI
directly to the manifest. When `--file` is provided, no upward search
takes place.

The file **MUST** be UTF-8 encoded and **MUST** parse as valid YAML 1.2.

## Editor integration

The JSON Schema corresponding to this specification is published at
`docs/spec/manifest-v0.schema.json` in the LightShuttle repository.
Editors that follow the `yaml-language-server` convention pick it up
automatically when the manifest starts with the modeline:

```yaml
# yaml-language-server: $schema=https://raw.githubusercontent.com/nubster-opensources/lightshuttle/main/docs/spec/manifest-v0.schema.json
project:
  name: my-app
# ...
```

With the modeline in place, Visual Studio Code (with the YAML
extension), IntelliJ-family IDEs and neovim through
`yaml-language-server` provide autocompletion of fields, inline
validation of values and hover documentation taken from the Rust
doc comments.

The schema is regenerated from the Rust types of the
`lightshuttle-manifest` crate by `cargo xtask schema` and the
continuous integration pipeline rejects any change that lets the
on-disk schema drift from the model.

## Top-level structure

```yaml
project:
  name: my-app
resources:
  api_db:
    postgres:
      version: "16"
  api:
    container:
      image: my-org/api:1.0
```

Two top-level sections are recognised in `v0`:

| Section | Required | Purpose |
|---|---|---|
| `project` | yes | Identity of the project as a whole. |
| `resources` | yes | The set of declared resources. |

A `lightshuttle` discriminator field at the top level is **OPTIONAL** in
`v0`. When present, it **MUST** equal the string `"v0"`. When absent, the
CLI treats the file as `v0`. Future major versions of the specification
will introduce their own discriminator (`"v1"`, `"v2"`) and the field
will become **REQUIRED** for those versions, while remaining absent-means-v0
for backward compatibility.

Any unknown top-level field **MUST** produce a warning at validate time,
and a hard error under `--strict`.

## The `project` section

```yaml
project:
  name: my-app
  version: "0.1.0"        # optional
  description: "Some text" # optional
```

| Field | Type | Required | Default | Description |
|---|---|---|---|---|
| `name` | string | yes | — | Project identifier, used as a prefix in container names. **MUST** match `^[a-z][a-z0-9_-]{0,31}$`. |
| `version` | string | no | none | Free-form version label. Informational only; LightShuttle does not interpret it. |
| `description` | string | no | none | Free-form description shown in the dashboard. |

Additional fields under `project` **MUST** produce a warning at validate
time. They are reserved for future versions.

## The `resources` section

```yaml
resources:
  <resource_name>:
    <kind>:
      <kind-specific fields>
```

The `resources` value is a YAML mapping where each key is a **resource
name** and each value is a single-entry mapping whose key is the
**resource kind** and whose value is the kind-specific configuration.

This shape is **externally tagged**: the resource kind is the key inside
the resource entry, not a sibling field named `kind`. The shape was chosen
for compactness and for alignment with idioms familiar to Helm and
GitHub Actions users.

### Resource names

A resource name **MUST** match the regular expression
`^[a-z][a-z0-9_-]{0,31}$`. Lowercase, ASCII letter first, then letters,
digits, underscores or hyphens, up to 32 characters total.

A resource name **MUST** be unique within the manifest.

### Recognised resource kinds in `v0`

| Kind | Purpose |
|---|---|
| `postgres` | PostgreSQL database. |
| `redis` | Redis key-value store. |
| `container` | Arbitrary container pulled from an image registry. |
| `dockerfile` | Arbitrary container built locally from a Dockerfile. |

A resource entry **MUST** contain exactly one kind key. Multiple kind keys
under the same resource **MUST** produce a hard error at validate time.

Unknown kinds **MUST** produce a hard error at validate time.

### Common fields

The following fields are recognised on every resource kind:

| Field | Type | Required | Default | Description |
|---|---|---|---|---|
| `depends_on` | list of strings | no | `[]` | Explicit dependency resource names. |
| `healthcheck` | object | no | kind-specific built-in | Override the default healthcheck. See [Healthcheck](#healthcheck). |

Implicit dependencies are also created by `${resources.<name>.*}`
interpolation. They are merged with `depends_on` and de-duplicated.

### Resource kind: `postgres`

```yaml
api_db:
  postgres:
    version: "16"        # optional
    database: api        # optional, default = resource name
    user: postgres       # optional, default "postgres"
    password: ${env.DB_PWD:-}  # optional, default = generated
    port: 5432           # optional
    volume: true         # optional
    healthcheck: ...     # optional
    depends_on: []       # optional
```

| Field | Type | Required | Default | Description |
|---|---|---|---|---|
| `version` | string | no | `"16"` | Major version. Expanded to `postgres:<version>-alpine` by the runtime. |
| `image` | string | no | derived from `version` | Full image reference. When set, takes precedence over `version`. |
| `database` | string | no | resource name | Initial database name. **MUST** match `^[a-z][a-z0-9_]*$`. |
| `user` | string | no | `"postgres"` | Superuser name. |
| `password` | string | no | random | Superuser password. When omitted, the orchestrator generates a 24-character random password at `up` time and prints it on `lightshuttle ps`. |
| `port` | integer | no | `5432` | Container port. Host-side mapping is decided by the runtime. |
| `volume` | boolean or string | no | `true` | `true` = auto-named persistent volume, `false` = ephemeral (data lost on `down`), string = explicit named volume. |
| `healthcheck` | object | no | `pg_isready -U <user>` | Override. |
| `depends_on` | list of strings | no | `[]` | Explicit dependencies. |

Exposed properties (consumable through `${resources.<name>.<property>}`):

| Property | Description |
|---|---|
| `host` | Hostname reachable by other containers in the stack. |
| `port` | Port number on the container side. |
| `database` | Database name. |
| `user` | Username. |
| `password` | Password. |
| `url` | Convenience full URL `postgres://<user>:<password>@<host>:<port>/<database>`. |

### Resource kind: `redis`

```yaml
cache:
  redis:
    version: "7"
    password: ""         # optional, default empty (no auth)
    port: 6379
    volume: true
    healthcheck: ...
    depends_on: []
```

| Field | Type | Required | Default | Description |
|---|---|---|---|---|
| `version` | string | no | `"7"` | Major version. Expanded to `redis:<version>-alpine`. |
| `image` | string | no | derived from `version` | Full image reference. Takes precedence over `version`. |
| `password` | string | no | empty | Optional auth password. Empty string disables auth. |
| `port` | integer | no | `6379` | Container port. |
| `volume` | boolean or string | no | `true` | Persistent volume. |
| `healthcheck` | object | no | `redis-cli PING` | Override. |
| `depends_on` | list of strings | no | `[]` | Dependencies. |

Exposed properties:

| Property | Description |
|---|---|
| `host` | Hostname. |
| `port` | Port number. |
| `password` | Password (empty when auth disabled). |
| `url` | Convenience URL `redis://[:<password>@]<host>:<port>`. |

### Resource kind: `container`

```yaml
api:
  container:
    image: my-org/api:1.0       # required
    ports:                       # optional
      - 8080                     # short form: host port = container port
      - "9090:9090"              # full form: host:container
    env:                         # optional
      DATABASE_URL: ${resources.api_db.url}
    volumes: []                  # optional
    command: null                # optional
    working_dir: null            # optional
    depends_on: []
    healthcheck: ...
```

| Field | Type | Required | Default | Description |
|---|---|---|---|---|
| `image` | string | yes | — | Full image reference, including tag. Untagged references **MUST** produce a warning. |
| `ports` | list | no | `[]` | Port mappings. See [Port mappings](#port-mappings). |
| `env` | map of string to string | no | `{}` | Environment variables. Values support interpolation. |
| `volumes` | list of strings | no | `[]` | Volume mappings. See [Volume mappings](#volume-mappings). |
| `command` | string or list of strings | no | image default | Command override. |
| `working_dir` | string | no | image default | Working directory inside the container. |
| `depends_on` | list of strings | no | `[]` | Explicit dependencies. |
| `healthcheck` | object | no | none | Optional healthcheck. |

Exposed properties:

| Property | Description |
|---|---|
| `host` | Hostname reachable by other containers in the stack. |
| `ports` | List of declared container ports. |

### Resource kind: `dockerfile`

```yaml
frontend:
  dockerfile:
    context: ./apps/frontend     # required
    dockerfile: Dockerfile       # optional
    build_args: {}               # optional
    target: null                 # optional
    ports: []
    env: {}
    volumes: []
    command: null
    working_dir: null
    depends_on: []
    healthcheck: ...
```

In addition to the same fields as `container` (except `image`):

| Field | Type | Required | Default | Description |
|---|---|---|---|---|
| `context` | string | yes | — | Build context path, relative to the manifest file. |
| `dockerfile` | string | no | `"Dockerfile"` | Dockerfile path within the context. |
| `build_args` | map of string to string | no | `{}` | Build-time arguments passed to `docker build`. |
| `target` | string | no | none | Multi-stage build target. |

Exposed properties are the same as `container`.

### Port mappings

A port mapping entry **MAY** be:

- An **integer** `8080`: the container port. The host port equals the
  container port. This is the recommended form.
- A **string** `"9090:9090"`: explicit host:container mapping.
- A **string** `"127.0.0.1:9090:9090"`: explicit bind address and mapping.

The runtime **MAY** rewrite a clashing host port and report the rewrite
through `lightshuttle ps`, except under `--strict`.

### Volume mappings

A volume entry **MAY** be:

- A **string** `"./data:/var/lib/data"`: explicit host path to container
  path mapping.
- A **string** `"named-volume:/var/lib/data"`: named volume to container
  path mapping.

Relative host paths are resolved against the manifest directory.

### Healthcheck

The `healthcheck` object follows the Docker Compose convention:

| Field | Type | Required | Default | Description |
|---|---|---|---|---|
| `test` | list of strings | yes | — | Command to run; first element **SHOULD** be `"CMD"` or `"CMD-SHELL"`. |
| `interval` | duration string | no | `"5s"` | Between checks. |
| `timeout` | duration string | no | `"3s"` | Per check. |
| `retries` | integer | no | `5` | Consecutive failures before considered unhealthy. |
| `start_period` | duration string | no | `"5s"` | Grace period after start. |

Duration strings follow the Go duration format (`"5s"`, `"500ms"`,
`"2m"`).

## Dependencies and ordering

Dependencies between resources are declared in two ways:

1. **Explicit** via `depends_on`.
2. **Implicit** via interpolation: writing
   `${resources.api_db.url}` inside `api`'s `env` declares a dependency
   from `api` on `api_db`.

The two sources are merged and de-duplicated. A dependency on a resource
that does not exist **MUST** produce a hard error at validate time.

A dependency cycle **MUST** produce a hard error at validate time, with
all resources involved in the cycle named in the error message.

The orchestrator starts resources in topological order. Independent
branches **MAY** start in parallel. A resource **MUST** be considered
ready before any dependent resource starts. Readiness is decided by:

- A successful healthcheck if one is defined.
- The container reporting a `running` state if no healthcheck is defined.

Shutdown follows the reverse order.

## Variable interpolation

The manifest supports string interpolation in resource configuration
values. Three sources are recognised:

| Syntax | Meaning |
|---|---|
| `${resources.<name>.<property>}` | Exposed property of another resource. Creates an implicit dependency on `<name>`. |
| `${env.<NAME>}` | Host environment variable. Resolution failure **MUST** produce a hard error at validate time. |
| `${env.<NAME>:-<default>}` | Host environment variable with default. The default is used when the variable is unset **or** empty. |

To emit a literal `${...}` sequence, use the escape form `${{...}}`.

```yaml
api:
  container:
    image: my-org/api:1.0
    env:
      DATABASE_URL: ${resources.api_db.url}
      LOG_LEVEL: ${env.LOG_LEVEL:-info}
      LITERAL: ${{not.interpolated}}     # emits the string "${not.interpolated}"
```

### Resolution semantics

- Interpolation **MUST** happen at `lightshuttle validate` time for
  syntactic correctness, and again at `lightshuttle up` time for
  runtime values.
- Interpolation **MUST** be performed exactly once per occurrence;
  the result is not re-scanned for further interpolation.
- An unknown resource property in `${resources.<name>.<property>}`
  **MUST** produce a hard error at validate time.
- Cyclic references (`a.url` referencing `b.url` referencing `a.url`)
  follow the same cycle rule as `depends_on` and produce a hard error.

## Lifecycle

The lifecycle of a resource is driven by the orchestrator. The manifest
**MAY** influence it through fields described above; this section
documents the default behaviour.

### Startup policy

For every resource, the default startup policy is `wait_for_health`:

1. The resource is started.
2. If a `healthcheck` is defined, the orchestrator waits for the
   first successful check before considering the resource ready.
3. If no `healthcheck` is defined, the resource is considered ready
   as soon as the container reports a `running` state.

Dependent resources **MUST NOT** start before their dependencies are
ready.

Additional startup policies are planned for v0.4 and are out of scope
of `v0`.

### Shutdown

On `SIGINT` or `SIGTERM`, the CLI initiates a coordinated shutdown:

1. Resources are sent `SIGTERM` in reverse topological order.
2. Each resource has a configurable grace period (default 10 seconds)
   to exit cleanly.
3. After the grace period, `SIGKILL` is sent.

A second `SIGINT` during shutdown **MUST** trigger an immediate `SIGKILL`
to every running resource.

## Validation

The `lightshuttle validate` command parses, type-checks and
interpolates the manifest without starting anything. Its outputs are:

- Exit code `0` and no output: the manifest is valid.
- Exit code `0` with warnings on stderr: the manifest is valid but
  uses defaults or carries fields that may surprise the user.
- Exit code `1` and a diagnostic on stderr: the manifest is invalid.

Under the `--strict` flag, every warning is upgraded to an error.
Continuous integration pipelines **SHOULD** use `--strict`.

The exact list of warnings is implementation-defined. The list of
errors is normative and defined throughout this specification by the
keywords MUST and MUST NOT.

## Conformance

A conformant implementation of `v0`:

- **MUST** accept any manifest that satisfies every MUST and MUST NOT
  rule in this document.
- **MUST** reject any manifest that violates a MUST or MUST NOT rule.
- **MUST** support every resource kind listed in
  [Recognised resource kinds in v0](#recognised-resource-kinds-in-v0).
- **MUST** support the `lightshuttle: "v0"` discriminator and the
  absent-means-v0 default.
- **SHOULD** emit a warning for every SHOULD or SHOULD NOT rule
  violation.
- **MAY** emit additional warnings beyond those defined here.

## Examples

### Hello world

A single Postgres database and a container that reads its connection
string. Six lines.

```yaml
project:
  name: hello
resources:
  db:
    postgres:
      version: "16"
  app:
    container:
      image: alpine
      command: ["sh", "-c", "echo $DATABASE_URL"]
      env:
        DATABASE_URL: ${resources.db.url}
```

### Real-world example

A Postgres database, a Redis cache, an API container and a frontend
built locally from a Dockerfile. Around twenty lines.

```yaml
project:
  name: my-app
  version: "0.1.0"

resources:
  cache:
    redis:
      version: "7"

  api_db:
    postgres:
      version: "16"

  api:
    container:
      image: my-org/api:1.0
      env:
        DATABASE_URL: ${resources.api_db.url}
        REDIS_URL: ${resources.cache.url}
        LOG_LEVEL: ${env.LOG_LEVEL:-info}
      ports:
        - 8080

  frontend:
    dockerfile:
      context: ./apps/frontend
      target: dev
      env:
        API_URL: http://${resources.api.host}:8080
      ports:
        - 3000
```

## Forward compatibility

The following are explicitly **not** part of `v0` but are reserved for
later versions. Until they ship, using their keywords at the top level
or inside a resource produces a warning at validate time.

| Future version | Keyword | Topic |
|---|---|---|
| `v0.2` | `dashboard:` (top-level) | Dashboard configuration. |
| `v0.3` | `export:` (top-level) | Per-target production export overrides. |
| `v0.4` | `hooks:` (top-level) | Global lifecycle hooks. |
| `v0.4` | `${secret.*}` (interpolation) | Secret store integration. |
| `v0.4` | resource kinds `mysql`, `mariadb`, `mongodb`, `static`, `process` | Additional resource kinds. |

Future versions of this specification **MUST NOT** remove or repurpose
any keyword defined in `v0`. They **MAY** extend resource kinds with
new optional fields under existing keywords.
