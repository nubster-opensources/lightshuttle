# LightShuttle documentation

This folder is the entry point to every piece of LightShuttle documentation. The
full guide is published as a book at
<https://nubster-opensources.github.io/lightshuttle/>; the sources live here so
they version with the code.

## Start here

- [Getting started](book/src/tutorials/getting-started.md): install, write a
  first `lightshuttle.yml`, run a first `lightshuttle up`.
- [Onboarding tutorials](book/src/tutorials/onboarding/index.md): the same arc
  for a Node.js, Python, Go or Rust service.

## Map of the docs

| Area | Where it lives |
| :--- | :--- |
| Tutorials (learning by doing) | [`book/src/tutorials/`](book/src/tutorials/) |
| How-to guides (task recipes) | [`book/src/how-to/`](book/src/how-to/) |
| Reference (manifest, CLI, generated) | [`book/src/reference/`](book/src/reference/) |
| Explanation (architecture, lifecycle, networking) | [`book/src/explanation/`](book/src/explanation/) |
| Feature specifications | [`spec/`](spec/) |

## Design and architecture

- [The crate architecture](book/src/explanation/architecture.md): the layered
  flow from manifest to runtime to CLI, with the dependency rule and the
  `ContainerRuntime` boundary.
- [The resource lifecycle](book/src/explanation/lifecycle.md) and
  [networking and service discovery](book/src/explanation/networking.md).
- Per-feature specifications: [manifest](spec/manifest-v0.md),
  [control plane API](spec/control-api.md), [observability](spec/observability.md),
  [export](spec/export.md).

## Project policies

- [Minimum Supported Rust Version](MSRV_POLICY.md)
- [Semantic Versioning](SEMVER_POLICY.md)
- [Release process](RELEASE_PROCESS.md)
- [Governance](GOVERNANCE.md)
- [Roadmap](explanation/roadmap.md), [changelog](../CHANGELOG.md),
  [contributing](../CONTRIBUTING.md), [code of conduct](../CODE_OF_CONDUCT.md).

## Standards mapping

LightShuttle is the reference repository for archetype C (layer-based CLI tool)
of the workspace standards. Section 1.6 of that standard asks for a structured
`docs/` folder; the table below records where each required piece lives, since
the book supersedes a flat layout without duplicating its content.

| Required by the standard | Satisfied by |
| :--- | :--- |
| `getting-started` | [`book/src/tutorials/getting-started.md`](book/src/tutorials/getting-started.md) |
| MSRV policy | [`MSRV_POLICY.md`](MSRV_POLICY.md) |
| SemVer policy | [`SEMVER_POLICY.md`](SEMVER_POLICY.md) |
| Design docs | [`book/src/explanation/`](book/src/explanation/) and [`spec/`](spec/) |
