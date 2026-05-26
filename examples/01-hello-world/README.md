# Example 01: hello world

The smallest possible manifest: a single Postgres 16 instance.

It demonstrates that:

- A resource can be declared with one line of configuration
  (`version: "16"`), every other field has a sensible default.
- The runtime auto-generates a password, picks a persistent volume and
  configures a `pg_isready` healthcheck on your behalf.

## Run it

```sh
cd examples/01-hello-world
lightshuttle up
```

Press `Ctrl+C` to stop. The volume persists between runs; use
`lightshuttle down` followed by `docker volume rm` to wipe state.
