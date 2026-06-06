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

- A multi-stage local build: `apps/api/Dockerfile` exposes `dev` and
  `release` stages, and the manifest selects `dev` through `target:`.

## Run it

```sh
cd examples/03-full-stack
lightshuttle up
```

The `api` image builds from `apps/api/Dockerfile` on first boot. The
shipped Dockerfile is a stand-in that logs and sleeps; replace it with
your real service to turn this template into your own stack.

`Ctrl+C` for a clean shutdown.
