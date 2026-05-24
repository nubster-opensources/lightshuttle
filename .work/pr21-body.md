feat(cli): top-level commands for the lightshuttle binary

Wires the four foundational layers (manifest, runtime, lifecycle,
schema) behind a clap-derived CLI with six subcommands. This is the
PR that turns LightShuttle from a Rust workspace into something a
developer can actually invoke.

Closes #9.

## Subcommands

- `lightshuttle up` boots the stack and supervises it until SIGINT
  or SIGTERM, then stops everything cleanly with the configured
  grace window.
- `lightshuttle down` queries Docker by label and stops every
  container managed by the project, even after a hard kill of the
  manager.
- `lightshuttle ps` prints a tabular view of the managed resources
  and their status.
- `lightshuttle logs <resource>` streams logs of a single resource,
  with an optional follow mode.
- `lightshuttle validate` parses and validates the manifest without
  touching Docker.
- `lightshuttle manifest` dumps the resolved manifest to stdout as
  YAML.

A global `--file <path>` (short `-f`) overrides the manifest
discovery; otherwise the CLI walks parent directories from the cwd
looking for `lightshuttle.yml`, in the spirit of `cargo` looking for
`Cargo.toml`.

## Architecture

The crate `lightshuttle` is split into five modules:

- `cli.rs` — clap derive types.
- `discovery.rs` — manifest path resolution.
- `output.rs` — `ps` tabular formatter and log chunk writer.
- `commands/` — one module per subcommand plus a shared
  `ExitOutcome` mapper.
- `main.rs` — tokio entry point that dispatches to the right
  command and translates the outcome into a POSIX exit code
  (0 success, 1 user error, 2 runtime error).

## Runtime changes

To support `ps`/`down`/`logs` without relying on in-memory state,
`DockerRuntime::start` now writes two Docker labels on every
container it creates:

- `lightshuttle.project` carries the project name.
- `lightshuttle.resource` carries the resource name.

`DockerRuntime::list_managed(project)` queries Docker by the
project label and returns a sorted list of `ManagedContainer` values
(id, resource, status).

The `ContainerSpec` struct gains two new public fields, `project`
and `resource`, so the runtime can populate the labels at create
time. `from_resource` fills them automatically; existing tests that
construct `ContainerSpec` literally were updated.

## Dependencies

Workspace-level: `humantime = "2"`.

Crate `lightshuttle`:

- `lightshuttle-manifest`, `lightshuttle-runtime` (path).
- `clap`, `tokio`, `tracing`, `tracing-subscriber`, `anyhow`,
  `humantime`, `futures` (all from `[workspace.dependencies]`).
- Dev-deps: `assert_cmd`, `predicates`, `tempfile`.

## Tests

- `tests/cli.rs` (4 tests) exercises `validate` and `manifest`
  end-to-end against the compiled binary via `assert_cmd`. No
  Docker required.
- `ps`, `down`, `logs` and `up` are exercised on a developer
  machine via the existing `#[ignore]` integration tests of
  `lightshuttle-runtime`; the manager event stream already gives
  CLI-level coverage indirectly.

## Pre-commit verification

- cargo fmt --all -- --check exits 0.
- cargo build --workspace --all-features exits 0.
- cargo clippy --workspace --all-targets --all-features -- -D warnings exits 0.
- cargo test --workspace --all-features reports 42 tests passing
  plus 3 ignored Docker-dependent integration tests.

## What this PR is not

- It does not implement env-var auto-injection between resources
  (`LSH_<SERVICE>_<PROP>`). That is the scope of #8.
- It does not ship a user guide; the documentation PR (#10) follows
  this one to capture the now-stable CLI surface.
