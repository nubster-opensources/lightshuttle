# Observability

LightShuttle ships two complementary observability surfaces, both
local-only and both fed by the same `lightshuttle up` process:

1. **Self-tracing** — the orchestrator emits OTLP spans for every
   lifecycle operation, pushed over gRPC to the bundled collector.
2. **Prometheus metrics** — the control plane exposes a pull-based
   `/metrics` endpoint on the dashboard port.

The two are independent: tracing is push (OTLP gRPC to the collector),
metrics are pull (Prometheus scrape of `/metrics`). Disabling OTel with
`--no-otel` or `observability.otel.enabled: false` removes the spans
but keeps `/metrics` available.

## Self-tracing (OTLP)

On `lightshuttle up` with OTel enabled,
`lightshuttle_otel::init_orchestrator_tracer(endpoint, "lightshuttle")`
installs a `tracing` subscriber wired to an OTLP gRPC span exporter
pointed at `http://127.0.0.1:4317` (the loopback port the bundled
collector publishes). It returns a `TracerGuard`; dropping the guard on
shutdown flushes any pending spans.

Spans emitted (resource name carried as the `resource` attribute):

| Span           | Emitted from                       | Attributes          |
| -------------- | ---------------------------------- | ------------------- |
| `start`        | per-resource startup               | `resource`          |
| `wait_healthy` | healthcheck wait inside `start`    | `resource`          |
| `stop`         | per-resource stop in `stop_all`    | `resource`          |
| `restart_one`  | `LifecycleManager::restart_one`    | `resource`          |

The `start` and `wait_healthy` spans are nested: `wait_healthy` is a
child of `start`, so a trace shows the full pull → create → wait chain
per resource.

## Prometheus metrics (`GET /metrics`)

The control plane mounts `/metrics` on the same axum server as the
dashboard, in the Prometheus text exposition format
(`text/plain; version=0.0.4`). Scrape it from the dashboard URL printed
on boot.

| Metric                                          | Type      | Labels   | Use |
| ----------------------------------------------- | --------- | -------- | --- |
| `lightshuttle_resources`                        | gauge     | `status` | Number of managed resources per status (`pending`, `starting`, `running`, `failed`, `stopped`). Recomputed at scrape time from the lifecycle handle. |
| `lightshuttle_restart_total`                    | counter   | —        | Total accepted restart requests. Incremented when `POST /api/resources/{name}/restart` is accepted. |
| `lightshuttle_lifecycle_event_duration_seconds` | histogram | —        | Seconds a resource takes to go from `ResourceStarted` to `ResourceHealthy`. Observed by the in-process event pump that follows the lifecycle broadcast. |
| `lightshuttle_uptime_seconds`                   | gauge     | —        | Orchestrator uptime in seconds. Set at scrape time from the process start instant. |

### Scrape-time vs event-time metrics

- `lightshuttle_resources` and `lightshuttle_uptime_seconds` are
  **scrape-time** gauges: they are refreshed inside the `/metrics`
  handler immediately before rendering, so they always reflect the
  current plan state.
- `lightshuttle_restart_total` and
  `lightshuttle_lifecycle_event_duration_seconds` are **event-time**:
  the counter is bumped at the REST boundary, and the histogram is fed
  by a task subscribed to the lifecycle event broadcast.

### Example scrape

```text
# HELP lightshuttle_resources Number of managed resources, labelled by status
# TYPE lightshuttle_resources gauge
lightshuttle_resources{status="running"} 2
lightshuttle_resources{status="failed"} 1
# HELP lightshuttle_restart_total Total number of accepted restart requests
# TYPE lightshuttle_restart_total counter
lightshuttle_restart_total 3
# HELP lightshuttle_uptime_seconds Orchestrator uptime in seconds
# TYPE lightshuttle_uptime_seconds gauge
lightshuttle_uptime_seconds 142.7
```

## Wiring summary

- `lightshuttle-otel` owns tracer init (`init_orchestrator_tracer`) and
  the collector wiring (`augment_manifest`, `CollectorConfig`).
- `lightshuttle-control` owns the Prometheus recorder (`Metrics`), the
  `/metrics` route and the restart counter.
- `lightshuttle up` is the composition root: it installs telemetry,
  installs the metrics recorder, spawns the event pump and serves the
  dashboard.
