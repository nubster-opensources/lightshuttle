# Example 04: export to deployment artifacts

The same `lightshuttle.yml` you run locally, this time used as the source
of truth for deployment artifacts. It carries an `export:` section that
tailors each target.

It demonstrates:

- Generating `docker-compose.yml`, plain Kubernetes manifests and a Helm
  chart from one manifest with `lightshuttle export <target>`.
- Per-target overrides: a Kubernetes namespace and replica counts, a Helm
  chart name and version, and a resource excluded from the compose output.
- The split between plain environment (a `ConfigMap`) and secret-looking
  keys (a `Secret`), driven by the key name.

## Try it

```sh
# From this directory:
lightshuttle export compose      # writes ./export/compose/docker-compose.yml
lightshuttle export kubernetes   # writes ./export/kubernetes/*.yaml
lightshuttle export helm         # writes ./export/helm/ (Chart.yaml, values.yaml, templates/)
```

The output lands under `./export/<target>/` (git-ignored). Pass
`--output <dir>` to choose another location and `--force` to overwrite a
non-empty one.

See the [export tutorial](https://nubster-opensources.github.io/lightshuttle/tutorials/export.html) for a full
walkthrough and the [export specification](../../docs/spec/export.md) for
the mapping rules.

## Note on secrets

`db.password` and `api.API_TOKEN` are placeholders. Real deployments
should supply secrets from a vault rather than committing them to the
manifest. A `postgres` resource with no `password` resolves to a freshly
generated one on every run, which is convenient for `up` but makes the
export non-reproducible, so set an explicit value when you intend to
export.
