# Export specification (v0.3.0)

`lightshuttle export <target>` turns a `lightshuttle.yml` manifest into
deployment artifacts. The same typed manifest that boots the stack
locally becomes the single source of truth for a `docker-compose.yml`
file, plain Kubernetes manifests or a Helm chart.

The pipeline has a compiler shape: the manifest is lowered into a neutral
model by resolving every resource exactly as `lightshuttle up` does, then
a per-target emitter renders that model. Because the lowering reuses the
runtime resolution, the image, port, environment and healthcheck a target
receives never drift from what `up` runs.

## CLI

```sh
lightshuttle export <target> [--output <dir>] [--force]
```

- `<target>` is one of `compose`, `kubernetes` or `helm`.
- `--output <dir>` chooses the output directory. The default is
  `./export/<target>`.
- `--force` is required to write into a non-empty directory.

Relative host volume paths in the manifest are resolved against the
manifest directory before export, so a generated artifact never carries a
path that only made sense relative to the manifest file.

## The `export:` section

The optional top-level `export:` section carries per-target overrides.
Every field is optional; an absent section resolves to the defaults
below. A resource named in any `resources` map must be a declared
resource, otherwise the manifest is rejected.

```yaml
export:
  compose:
    resources:
      <resource>:
        enabled: false        # omit this resource from the compose output
  kubernetes:
    namespace: <string>       # default: the project name
    replicas: <uint>          # default: 1
    image_pull_policy: Always | IfNotPresent | Never   # default: IfNotPresent
    resources:
      <resource>:
        enabled: false
        replicas: <uint>
        image_pull_policy: Always | IfNotPresent | Never
  helm:
    chart_name: <string>      # default: the project name
    chart_version: <string>   # default: the project version, else 0.1.0
    replicas: <uint>          # default: 1
    resources:
      <resource>:
        enabled: false
        replicas: <uint>
```

A per-resource value wins over the per-target value, which falls back to
the documented default. Exclusion is expressed once, with
`resources.<name>.enabled: false`; there is no separate exclude list.

## Target matrix

| Target       | Output                                                        | Validated by            |
| ------------ | ------------------------------------------------------------- | ----------------------- |
| `compose`    | a single `docker-compose.yml`                                 | `docker compose config` |
| `kubernetes` | `namespace.yaml` plus one multi-document `<resource>.yaml`    | `kubeconform -strict`   |
| `helm`       | `Chart.yaml`, `values.yaml` and `templates/<resource>.yaml`   | `helm lint`             |

## Cross-cutting rules

These hold for every target.

- **Resource names.** Names are sanitised to DNS-1123 for Kubernetes and
  Helm: `_` becomes `-`.
- **Secret split.** Values declared under a resource's `secrets:` map are
  always treated as sensitive. For compatibility, environment keys are also
  treated as sensitive when their name contains `PASSWORD`, `PASSWD`, `PASS`,
  `SECRET`, `TOKEN`, `KEY`, `CREDENTIAL`, `AUTH`, `CERT` or `PWD`,
  case-insensitively. Kubernetes and Helm route them to a `Secret` using a
  placeholder value; Compose emits a `${KEY}` reference. Exported artifacts
  therefore require those values to be provisioned separately.
- **Built images.** A `dockerfile` resource has no registry image, so the
  generated build tag is emitted as the image reference. You are
  responsible for building and pushing that tag before applying a
  Kubernetes or Helm artifact.
- **Reproducibility.** A `postgres` or `redis` resource with no explicit
  `password` resolves to a freshly generated one on every run. Exporters do
  not embed that password; provision the corresponding environment or Secret
  value before deploying the generated artifact.

## Compose mapping

One service per enabled resource, keyed by the resource name.

- Pulled images map to `image:`; `dockerfile` resources map to a `build:`
  block plus the tagged `image:`.
- Published ports default to a `127.0.0.1` host bind, preserving the
  not-exposed-by-default posture of `lightshuttle up`. A manifest port
  with an explicit `address:host:container` form keeps that address.
- `depends_on` uses the long form with `condition: service_healthy`.
- Named volumes are declared in the top-level `volumes:` block.
- Sensitive environment keys use `${KEY}` references and must be supplied to
  Compose by the caller.
- Healthchecks carry `test`, `interval`, `timeout`, `retries` and
  `start_period`.

## Kubernetes mapping

Per enabled resource: a `Deployment`, a `ClusterIP` `Service`, a
`ConfigMap` and a `Secret` for the environment split, and a
`PersistentVolumeClaim` per named volume. A `Namespace` is emitted once.

- `replicas` and `imagePullPolicy` come from the resolved `export:`
  values.
- Environment is referenced with `envFrom` against the `ConfigMap` and
  `Secret`.
- The manifest healthcheck maps to a `readinessProbe` and a
  `livenessProbe`: `interval` to `periodSeconds`, `timeout` to
  `timeoutSeconds`, `retries` to `failureThreshold` and `start_period` to
  `initialDelaySeconds`.
- Named volumes become a `PersistentVolumeClaim` (`ReadWriteOnce`, `1Gi`);
  anonymous volumes become an `emptyDir`; host paths become a `hostPath`.

## Helm mapping

A chart whose `templates/` reach parity with the Kubernetes target while
the knobs surface in `values.yaml`.

- `values.yaml` carries, per service, `replicas`, `image`
  (`repository`, `tag`, `pullPolicy`) and the `env` and `secrets` maps.
- Templates reference `{{ $svc.replicas }}`,
  `{{ $svc.image.repository }}:{{ $svc.image.tag }}` and render the
  environment with `{{- range $k, $v := $svc.env }}`.
- Services are accessed with `index .Values.services "<name>"` so
  DNS-sanitised names containing a dash resolve.

## Validation

Each target carries an offline validation in the test suite, exercised in
CI:

```sh
cargo test -p lightshuttle-export -- --ignored
```

The checks run `docker compose config`, `kubeconform -strict` and
`helm lint` against generated output, skipping any tool that is not
installed.

See the [export tutorial](../tutorial/export.md) for a step-by-step
walkthrough.
