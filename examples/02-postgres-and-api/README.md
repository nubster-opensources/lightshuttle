# Example 02: Postgres and a tiny API container

A Postgres database and a container that reads its connection string.
Mirrors the example used throughout the
[getting-started tutorial](../../docs/tutorial/getting-started.md).

It demonstrates:

- Variable interpolation with `${resources.db.url}`.
- The implicit dependency it creates: `app` is gated on `db` becoming
  healthy.
- The dual environment-variable contract: the explicit `DATABASE_URL`
  declared in `env`, and the automatic `LSH_DB_*` family injected by
  the runtime.

## Run it

```sh
cd examples/02-postgres-and-api
lightshuttle up
```

In another terminal, observe and inspect:

```sh
lightshuttle ps
lightshuttle logs app
```

`Ctrl+C` for a clean shutdown.
