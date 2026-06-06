# Tutorial: export to deployment artifacts

`lightshuttle up` runs your stack locally. When it is time to ship,
`lightshuttle export` turns the same manifest into the deployment format
your platform expects, with no second source of truth to keep in sync.

This walkthrough uses [`examples/04-export`](../../examples/04-export),
whose manifest carries an `export:` section that tailors each target.

## The manifest

```yaml
project:
  name: shop
  version: "1.2.0"

export:
  compose:
    resources:
      worker:
        enabled: false        # a dev-only helper, left out of compose
  kubernetes:
    namespace: shop-staging
    replicas: 2
    resources:
      api:
        replicas: 3           # the API scales wider than the default
  helm:
    chart_name: shop
    chart_version: "1.2.0"

resources:
  db:
    postgres:
      version: "16"
      database: shop
      password: ${env.SHOP_DB_PASSWORD:-change-me-in-your-vault}
  api:
    container:
      image: nginx:1.27-alpine
      ports:
        - 8080:80
      env:
        LOG_LEVEL: info
        API_TOKEN: ${env.SHOP_API_TOKEN:-change-me-in-your-vault}
      depends_on: [db]
  worker:
    container:
      image: alpine:3.20
      command: ["sh", "-c", "echo worker booting && sleep 3600"]
```

The `export:` section is optional. Without it every target still
generates valid output using the defaults described in the
[export specification](../spec/export.md).

## Docker Compose

```sh
lightshuttle export compose
```

Writes `./export/compose/docker-compose.yml`. The `db` and `api` services
appear; `worker` is omitted because the manifest disables it for compose.
Published ports bind to `127.0.0.1` and `api` waits on `db` becoming
healthy.

Validate it with the Compose CLI:

```sh
docker compose -f export/compose/docker-compose.yml config
```

## Kubernetes

```sh
lightshuttle export kubernetes
```

Writes one file per resource plus a namespace:

```
export/kubernetes/
  namespace.yaml
  db.yaml      # Deployment, Service, ConfigMap, Secret, PersistentVolumeClaim
  api.yaml     # Deployment (replicas: 3), Service, ConfigMap, Secret
  worker.yaml
```

The namespace is `shop-staging`, `api` runs three replicas (its
per-resource override) and the rest run two (the target default).
`API_TOKEN` lands in a `Secret`, `LOG_LEVEL` in a `ConfigMap`.

Validate the manifests offline with
[`kubeconform`](https://github.com/yannh/kubeconform):

```sh
kubeconform -strict export/kubernetes/*.yaml
```

`kubectl apply --dry-run=client` is avoided here because it contacts the
cluster to download the schema, which makes it depend on your kubeconfig.

## Helm

```sh
lightshuttle export helm
```

Writes a chart:

```
export/helm/
  Chart.yaml          # name: shop, version: 1.2.0
  values.yaml         # per-service replicas, image and env/secrets
  templates/
    db.yaml
    api.yaml
    worker.yaml
```

The knobs live in `values.yaml`, so a downstream operator can override
replicas or the image tag without editing templates. Validate the chart:

```sh
helm lint export/helm
```

## Output options

- `--output <dir>` writes elsewhere than `./export/<target>`.
- `--force` overwrites a non-empty output directory.

## A note on secrets

The manifest resolves its secrets through `${env.*}` references with
explicit defaults: the export stays reproducible out of the box, and
real values override the placeholders from a `.env` file or the
process environment (the file wins). Audit the resolution before
exporting:

```sh
lightshuttle secrets check
```

A `postgres` or `redis` resource without any `password` resolves to a
freshly generated one on every run, which is handy for `up` but means
each export bakes a different value: keep an explicit password or an
`${env.*}` reference with a default when you export. Real deployments
should still source production secrets from a vault, not from the
exported files.

For the full mapping rules see the
[export specification](../spec/export.md).
