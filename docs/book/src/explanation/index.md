# Explanation

Explanation is understanding-oriented: it steps back from the day-to-day tasks
to discuss how LightShuttle works and why it is built the way it is. Read this
section when you want the reasoning behind a design rather than instructions for
a task.

This section currently covers:

- [The crate architecture](architecture.md): the workspace layout, the
  dependency rule, and the control plane versus the runtime.
- [The resource lifecycle](lifecycle.md): startup ordering, readiness gating,
  supervision, and graceful shutdown.
- [Networking and service discovery](networking.md): the per-project network,
  hostname-by-name addressing, and how `${resources.*}` and `LSH_*` interact.
