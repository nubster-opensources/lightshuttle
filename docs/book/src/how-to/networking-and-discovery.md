# Reach one resource from another

Every LightShuttle project runs on its own network where containers find
each other by name. This guide shows how that network works, how to address
a resource, and the two ways a container can discover its dependencies.

For the underlying rules, see the [Networking section of the manifest
specification](https://github.com/nubster-opensources/lightshuttle/blob/main/docs/spec/manifest-v0.md).

## Address a resource by its name

At `up` time LightShuttle creates a dedicated Docker bridge network named
`lightshuttle-<project>` and removes it at `down`. Every container joins it
with a DNS alias equal to its resource name, so containers reach each other
by that name. The `host` property of a resource resolves to exactly this
alias:

```yaml
project:
  name: discovery-demo

resources:
  db:
    postgres:
      version: "16"

  cache:
    redis:
      version: "7"

  app:
    container:
      image: alpine:3.20
      command: ["sh", "-c", "echo db=$DB_HOST cache=$CACHE_URL && sleep 3600"]
      env:
        DB_HOST: ${resources.db.host}
        CACHE_URL: ${resources.cache.url}
```

`DB_HOST` becomes `db`, the resource name, which is the hostname `app`
uses to open a connection on the project network.

## Rely on project isolation

Two projects never collide: each sits on its own
`lightshuttle-<project>` network, so a resource named `db` in one project
is unreachable from another. You can run several stacks side by side
without port or name clashes. The project name is lowercased and any
character outside letters, digits and hyphens is replaced by `-` when the
network name is built.

## Discover a dependency with zero configuration

For every dependency, LightShuttle also injects each property of that
dependency as an environment variable named `LSH_<DEP>_<PROPERTY>`,
upper-cased. With `db` as a dependency, the container receives:

| Variable | Resolves to |
|---|---|
| `LSH_DB_HOST` | `${resources.db.host}` |
| `LSH_DB_PORT` | `${resources.db.port}` |
| `LSH_DB_DATABASE` | `${resources.db.database}` |
| `LSH_DB_USER` | `${resources.db.user}` |
| `LSH_DB_PASSWORD` | `${resources.db.password}` |
| `LSH_DB_URL` | `${resources.db.url}` |

The same pattern applies to every dependency: `LSH_CACHE_HOST`,
`LSH_CACHE_URL`, and so on.

## Choose explicit or automatic wiring

The two styles coexist on purpose:

- **Explicit interpolation** (`DATABASE_URL: ${resources.db.url}`) keeps
  your application portable. It reads the standard `DATABASE_URL` that
  every language ecosystem already understands, with no LightShuttle
  knowledge baked in.
- **Automatic `LSH_<DEP>_<PROP>`** variables are a zero-configuration
  escape hatch for wiring a quick script without editing the manifest.

Prefer explicit interpolation for application code you might run outside
LightShuttle; reach for the `LSH_*` variables for throwaway glue.
