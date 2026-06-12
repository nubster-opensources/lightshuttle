# Wire dependencies and gate on readiness

LightShuttle starts resources in dependency order and waits for each one
to be ready before starting the resources that need it. This guide shows
how to declare those dependencies, how readiness is decided, and how to
tighten it with a custom healthcheck.

It assumes you have booted a stack before; if not, start with the
[getting started tutorial](../tutorials/getting-started.md). For the field
grammar, see the [`container`](../reference/manifest/container.md) and
[Common types](../reference/manifest/common-types.md) reference pages.

## Declare a dependency implicitly

Most dependencies are declared for free: any `${resources.<name>.*}`
interpolation creates a dependency on `<name>`. Reading the database URL
into the application's environment is enough to make `app` start after
`db`:

```yaml
project:
  name: web-and-db

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
      # Custom healthcheck: dependents wait until this command succeeds.
      healthcheck:
        test: ["CMD-SHELL", "test -f /tmp/ready"]
        interval: "1s"
        timeout: "1s"
        retries: 10
        start_period: "2s"
```

`app` will not start until `db` is healthy, and the interpolation also
gives `app` the resolved value of `DATABASE_URL` at boot.

## Declare a dependency explicitly

When you need ordering but no value flows between the resources, use
`depends_on`. It accepts a list of resource names and is available on every
resource kind:

```yaml
      depends_on:
        - db
        - cache
```

Explicit and implicit dependencies are merged and de-duplicated, so adding
`depends_on: [db]` next to a `${resources.db.url}` reference is harmless.

## Understand how readiness is decided

A resource is *ready*, and therefore unblocks its dependents, when:

- its healthcheck succeeds, if one is defined; or
- the container reports a `running` state, if no healthcheck is defined.

The managed resource kinds (`postgres`, `redis`) ship with a built-in
healthcheck, so a dependent waits for a real connection, not just a started
process. Independent resources start in parallel; only dependency edges
force serialisation.

## Add a custom healthcheck

For a plain `container`, "running" is often too weak: the process is up but
not yet serving. Add a `healthcheck` so dependents wait for genuine
readiness. The fields mirror Docker Compose:

```yaml
      healthcheck:
        test: ["CMD-SHELL", "curl -fsS http://localhost:8080/health || exit 1"]
        interval: "5s"
        timeout: "3s"
        retries: 5
        start_period: "5s"
```

- `test` is required; its first element should be `CMD` (exec form) or
  `CMD-SHELL` (run through a shell).
- `interval`, `timeout` and `start_period` are Go duration strings
  (`"5s"`, `"500ms"`, `"2m"`); they default to `5s`, `3s` and `5s`.
- `retries` is the number of consecutive failures before the resource is
  marked unhealthy; it defaults to `5`.
- `start_period` is a grace window after start during which failures are
  not counted, so a slow boot does not trip the check.

## Let `validate` catch wiring mistakes

`lightshuttle validate` checks the dependency graph without touching
Docker. Two mistakes are hard errors at validate time:

- a dependency on a resource that does not exist; and
- a dependency cycle, with every resource in the cycle named in the
  message.

Run it in CI with `--strict` to fail the build on either:

```sh
$ lightshuttle validate --strict
```

Shutdown follows the reverse order: dependents stop before the resources
they relied on.
