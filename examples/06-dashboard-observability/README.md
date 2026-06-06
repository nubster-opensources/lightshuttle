# Example 06: dashboard, observability and a custom healthcheck

A Postgres database and a container with a hand-written readiness
probe, plus the two optional top-level sections that drive the local
developer experience:

- `dashboard.port` pins the web dashboard to a fixed port instead of a
  random free one.
- `observability.otel.enabled` controls the bundled OpenTelemetry
  collector (`true` is the default; the manifest spells it out for
  visibility).
- A custom `healthcheck` on a plain container: the resource reports
  ready only once `/tmp/ready` exists, about two seconds after start.

## Run it

```sh
cd examples/06-dashboard-observability
lightshuttle up
```

Then open the dashboard:

```
http://127.0.0.1:7878
```

Watch the `app` resource move from `starting` to `running`: the custom
healthcheck gates readiness until the probe file appears. Logs stream
live per resource, and the restart button mirrors
`lightshuttle restart app`.

Every managed container receives `OTEL_EXPORTER_OTLP_ENDPOINT` and
`OTEL_SERVICE_NAME` automatically while the collector is enabled. Set
`observability.otel.enabled: false` (or pass `--no-otel` to `up`) to
skip the collector entirely.

`Ctrl+C` for a clean shutdown.
