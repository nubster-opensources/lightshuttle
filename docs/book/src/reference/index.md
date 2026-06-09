# Reference

Reference material is information-oriented: exhaustive and normative. When you
need to know exactly what a field means or what a command does, this is the
section to consult.

## Specifications

The normative specifications live in the repository and are versioned alongside
the manifest:

- [Manifest specification](https://github.com/nubster-opensources/lightshuttle/blob/main/docs/spec/manifest-v0.md):
  every section, resource kind and interpolation rule.
- [Control plane API](https://github.com/nubster-opensources/lightshuttle/blob/main/docs/spec/control-api.md):
  the local HTTP API the dashboard and the client commands speak.
- [Observability](https://github.com/nubster-opensources/lightshuttle/blob/main/docs/spec/observability.md):
  the bundled OpenTelemetry collector and the metrics it exposes.
- [Export](https://github.com/nubster-opensources/lightshuttle/blob/main/docs/spec/export.md):
  how the manifest maps to each deployment target.

A JSON Schema for editor autocompletion ships at
[`docs/spec/manifest-v0.schema.json`](https://github.com/nubster-opensources/lightshuttle/blob/main/docs/spec/manifest-v0.schema.json).

## API documentation

The Rust API of every published crate is documented on docs.rs:

- [`lightshuttle`](https://docs.rs/lightshuttle) (the CLI)
- [`lightshuttle-manifest`](https://docs.rs/lightshuttle-manifest)
- [`lightshuttle-spec`](https://docs.rs/lightshuttle-spec)
- [`lightshuttle-runtime`](https://docs.rs/lightshuttle-runtime)
- [`lightshuttle-otel`](https://docs.rs/lightshuttle-otel)
- [`lightshuttle-control`](https://docs.rs/lightshuttle-control)
- [`lightshuttle-secrets`](https://docs.rs/lightshuttle-secrets)
- [`lightshuttle-export`](https://docs.rs/lightshuttle-export)

The **[manifest reference](manifest/index.md)** is generated from the JSON
Schema and documents every top-level section and resource kind. The **[CLI
reference](cli/index.md)** is generated from the command definitions and
documents every subcommand, its flags and examples.
