# Example 03: full polyglot stack

The kind of `lightshuttle.yml` a small startup would commit to its
repository: a Postgres database, a Redis cache, a third-party worker
container pulled from a registry, and an API service built from a
local Dockerfile.

It demonstrates:

- All four resource kinds of `manifest-v0` (`postgres`, `redis`,
  `container`, `dockerfile`) in one file.
- Multiple consumers sharing a single dependency (`api_db` and `cache`
  are read by both `worker` and `api`).
- Mixed explicit (`depends_on`) and implicit (`${resources.*}`)
  dependencies.
- Environment defaulting through `${env.LOG_LEVEL:-info}`.

## Caveat: this example is a shape, not a runnable thing

The `api` resource references a `./apps/api/Dockerfile` that does not
exist in this repository. The example is intentionally a *template*:
it shows what a realistic manifest looks like for a service team, but
the actual application code is out of scope.

`lightshuttle validate` will still pass on this manifest because the
build context is only resolved at `lightshuttle up` time.

```sh
cd examples/03-full-stack
lightshuttle validate     # passes
lightshuttle up           # will fail at the api build step until you provide apps/api/Dockerfile
```

To make it runnable, drop a minimal `apps/api/Dockerfile` next to this
file and rebuild.
