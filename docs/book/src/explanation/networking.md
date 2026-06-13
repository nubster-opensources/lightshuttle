# Networking and service discovery

Once the lifecycle has a resource running and ready, its dependents still have
to find it. LightShuttle's answer rests on one decision: every project gets its
own private network, and inside it resources address each other by name. Two
projects never share that world, and dependency values flow in as resolved
strings rather than as anything your code has to look up.

## One private network per project

At `up` time the runtime creates a dedicated Docker bridge network named
`lightshuttle-<project>` and removes it at `down`. The choice to scope a network
per project, rather than dropping every container onto one shared network, buys
two things.

The first is isolation. A resource named `db` in one project is simply
unreachable from another project's `db`, so you can run several stacks side by
side without name or port collisions. The second is a clean teardown boundary:
when the stack stops, its whole network goes with it, which is part of why a
stopped stack leaves nothing behind.

Network creation is idempotent on purpose. If two resources start concurrently
and both try to create the network, the second one's `409 Conflict` is treated
as success, because the only thing that matters is that the network exists.

## Resources are addressed by name, not by port

Each container joins the project network with a DNS alias equal to its resource
name. That alias is the hostname its peers use. A resource's `host` output
resolves to exactly this alias, which is why you address a dependency by the
name you gave it in the manifest and never by an IP address or a guessed port.

This is the deliberate difference from talking to a service through a published
host port. Published ports exist so *you*, on the host, can reach Postgres or
your app from a database GUI or a browser. Inside the network, peers skip the
host entirely and connect to the container directly by name. The name is stable
across restarts in a way an assigned port is not.

## Dependency values arrive already resolved

A resolved resource exposes a set of outputs to its dependents: `host`, `port`,
`url`, and for the managed kinds also `database`, `user` and `password`. The
runtime delivers those outputs into a dependent in two forms, on purpose.

The first form is explicit interpolation. When you write
`DATABASE_URL: ${resources.db.url}`, the dependency is recorded and the variable
is substituted with the resolved value at boot. Your application reads a plain
`DATABASE_URL`, the variable every ecosystem already understands, with no
LightShuttle knowledge baked into the code. That keeps the application portable:
it runs the same outside LightShuttle.

The second form is automatic. For every dependency, each output is also injected
as an environment variable named `LSH_<DEP>_<PROPERTY>`, upper-cased, for
example `LSH_DB_URL` or `LSH_DB_HOST`. This is a zero-configuration escape hatch
for wiring a quick script without editing the manifest. The two coexist by
design: reach for explicit interpolation in application code you might run
elsewhere, and for the `LSH_*` variables for throwaway glue.

## The model in one manifest

The manifest below is the whole model in miniature. `api` reads the database URL
by interpolation, which both creates a dependency on `db` and hands `api` the
resolved `${resources.db.url}` at boot. On the project network, that URL points
at the host `db`, the resource's network alias.

```yaml
project:
  name: discovery-model

resources:
  db:
    postgres:
      version: "16"

  api:
    container:
      image: alpine:3.20
      command: ["sh", "-c", "echo using $DATABASE_URL && sleep 3600"]
      env:
        DATABASE_URL: ${resources.db.url}
```

## Where to go next

- For the step-by-step task, including the full `LSH_*` table, see
  [Reach one resource from another](../how-to/networking-and-discovery.md).
- To understand when a dependency is considered ready to be reached, read
  [The resource lifecycle](lifecycle.md).
- For the exact outputs each resource kind exposes, see the
  [manifest reference](../reference/manifest/index.md).
