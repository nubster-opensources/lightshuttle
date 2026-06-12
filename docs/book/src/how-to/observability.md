# Collect traces and metrics locally

LightShuttle ships a bundled OpenTelemetry collector and wires your
containers to it automatically, so you can see traces and metrics without
standing up an observability stack by hand. This guide shows how to control
the collector, what it injects, and where to read metrics.

For the field grammar, see the
[`observability`](../reference/manifest/observability.md) and
[`dashboard`](../reference/manifest/dashboard.md) reference pages. For a
tour of the web dashboard itself, see the
[dashboard walkthrough](../tutorials/dashboard.md).

## Enable the bundled collector

The collector is on by default. The `observability.otel` block makes the
choice explicit and is where you turn it off:

```yaml
project:
  name: observability-demo

dashboard:
  port: 7878

observability:
  otel:
    enabled: true

resources:
  db:
    postgres:
      version: "16"

  app:
    container:
      image: alpine:3.20
      command: ["sh", "-c", "echo OTLP=$OTEL_EXPORTER_OTLP_ENDPOINT && sleep 3600"]
      env:
        DATABASE_URL: ${resources.db.url}
```

On `up`, LightShuttle prepends a collector container
(`otel/opentelemetry-collector:0.108.0`) to the stack and makes every other
container depend on it, so the collector is ready before your services
start emitting.

## Know what gets injected

Into every `container` and `dockerfile` resource, LightShuttle adds three
standard OpenTelemetry variables:

| Variable | Value |
|---|---|
| `OTEL_EXPORTER_OTLP_ENDPOINT` | `http://<project>_lightshuttle_otel:4317` |
| `OTEL_SERVICE_NAME` | the resource name |
| `OTEL_RESOURCE_ATTRIBUTES` | `service.name=<resource>,deployment.environment=local` |

The collector receives OTLP on `4317` (gRPC) and `4318` (HTTP). Managed
kinds (`postgres`, `redis`) use canned commands and are not injected.

Injection never overwrites a value you set yourself: if your manifest
already defines `OTEL_SERVICE_NAME`, LightShuttle keeps yours. The collector
is also a normal resource named `lightshuttle_otel`, so a service can target
it directly with `${resources.lightshuttle_otel.host}` when needed.

## Turn it off

Skip the collector and the env injection entirely either per manifest or
per run:

```yaml
observability:
  otel:
    enabled: false
```

```sh
$ lightshuttle up --no-otel
```

Use the manifest form when a project never wants the collector, and the
flag for a one-off run.

## Override the collector image

To pin a different collector build or configuration, declare a resource
with the reserved name `lightshuttle_otel` yourself. LightShuttle detects
it, skips its own augmentation and leaves your definition in place:

```yaml
  lightshuttle_otel:
    container:
      image: otel/opentelemetry-collector-contrib:0.108.0
```

## Read metrics from the dashboard

The local control plane exposes a Prometheus endpoint at `/metrics` in the
standard text exposition format, alongside the web dashboard. Pin the
dashboard to a known port to scrape it:

```yaml
dashboard:
  port: 7878
```

With the stack up, `http://localhost:7878/metrics` returns the current
metrics; point a local Prometheus or `curl` at it. Without a `port`,
`lightshuttle up` picks a free one and prints it at startup.
