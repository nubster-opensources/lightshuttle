# The resource lifecycle

A LightShuttle stack is not a flat list of containers to start. It is a graph,
and the runtime treats it as one. Understanding that single idea explains why
`up` boots in the order it does, why a dependent never starts too early, and why
`down` and Ctrl+C leave nothing behind.

## A stack is a dependency graph

Every dependency edge, whether you wrote it as an explicit `depends_on` or
created it implicitly with a `${resources.<name>.*}` interpolation, becomes an
edge in a graph. The two kinds are merged and de-duplicated, so the graph is the
single source of truth about ordering. From it the runtime builds a lifecycle
plan: a topological walk where a resource is only reached once everything it
depends on has been reached.

The payoff of modelling it as a graph rather than a sequence is parallelism with
correctness. Resources that share no edge have no reason to wait for each other,
so they start concurrently. Only real dependency edges force serialisation. A
stack with one database and three unrelated services boots the three services in
parallel, each gated only on the database.

## Startup is gated on readiness, not on "started"

Starting a container is not the same as the thing inside it being usable. A
Postgres process accepts the start command long before it accepts connections.
If a dependent started the moment its dependency's container existed, it would
race a half-ready service and fail intermittently. That class of flake is
exactly what the lifecycle is designed to remove.

So the runtime gates on readiness, not on existence. A resource is ready, and
only then unblocks its dependents, when:

- its healthcheck succeeds, if one is defined; or
- its container reports a `running` state, if none is defined.

The managed kinds (`postgres`, `redis`) ship a built-in healthcheck, so a
dependent waits for a real connection rather than a started process. For a plain
`container`, "running" is the default, which is why adding your own healthcheck
matters when the process needs warm-up time. The how-to guide
[Wire dependencies and gate on readiness](../how-to/dependencies-and-health.md)
covers how to declare those edges and checks.

## Lifecycle events are the spine

As it walks the plan, the manager emits a stream of events for each resource:
it has *started*, it has become *healthy*, or it has *failed*. These events are
not just logging. They are the contract the rest of the system observes: the
control plane consumes them to drive the dashboard, and the metrics pump uses
the started-to-healthy transition to record how long each resource took to come
up. Modelling progress as an event stream, rather than as polled state, is what
lets several observers watch the same boot without coupling to the runtime's
internals.

## Shutdown is the reverse walk, with a grace window

Once the stack is up, the manager supervises it and waits for a signal. On
SIGINT (Ctrl+C) or SIGTERM it tears the stack down in the reverse of the startup
order: dependents stop before the resources they relied on, so nothing is pulled
out from under a still-running consumer.

Each resource is given a grace window to stop cleanly. The runtime sends SIGTERM
and waits; only if the resource overruns the window does it escalate to SIGKILL.
The window defaults to 30 seconds and is tunable with `--grace`, which is the
knob you reach for when a service needs longer to flush in-flight work. After
the containers are gone, the per-project network is removed too, so a stopped
stack leaves no orphaned Docker resources behind. The teardown is written to
stay idempotent: a partial shutdown can be re-run safely.

## Why this shape

A development orchestrator earns trust by being boring in two moments: boot and
teardown. Boot must be reproducible, so a teammate running `up` on a fresh
checkout sees the same ordering you do, with dependents never observing a
half-ready dependency. Teardown must be complete, so iterating dozens of times a
day does not silently accumulate dead containers and networks. The graph model,
readiness gating and reverse-order shutdown with a grace window are the three
mechanisms that together make those two moments dependable.

## Where to go next

- To see how a ready resource is actually reached over the network, read
  [Networking and service discovery](networking.md).
- For the crate that owns this logic, read
  [The crate architecture](architecture.md).
- For the task-level steps, see
  [Wire dependencies and gate on readiness](../how-to/dependencies-and-health.md).
