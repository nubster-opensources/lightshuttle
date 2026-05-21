# Contributing to LightShuttle

LightShuttle is currently in **pre-alpha**. The project lives in public so the design phase happens transparently, not because external contributions can be accepted yet. The information below describes the conventions that will be applied once the project opens for contributions.

## Conventions

LightShuttle follows the Encelade Technologies general coding standards documented in [nubster-docs](https://github.com/nubster-opensources/nubster-docs/tree/main/docs/reference/coding-standards). In short:

- **Trunk-Based Development**, feature branches `feature/<issue>-<slug>` from `main`, never commit directly on `main`.
- **Conventional Commits**, all commit messages follow the `type(scope): description` format, enforced by `cog verify` in the commit-msg hook.
- **Rust style**, workspace lints `clippy::all` and `clippy::pedantic` set to `deny`, MSRV pinned in `rust-toolchain.toml` and `Cargo.toml`.
- **No competitor mentions**, the source code, commit messages, pull requests and documentation never name competing tools.
- **English on the public API, French on internal artifacts**, rustdoc comments and the `lightshuttle.yml` schema are written in English; commit messages, issues and project documentation are written in French.

## Local setup (when the project opens)

```bash
# Pin the Rust toolchain via rustup
rustup show

# Install local git hooks
lefthook install

# Run tests
cargo test --workspace --all-features
```

## Discussion before code

Until v0.1.0, all design decisions go through a `discussion/` thread on the repository before any pull request is opened. This includes the YAML schema, the resource catalog, the dashboard surface and the prod export formats.

## License

By contributing, you agree that your contributions are licensed under the [MIT license](./LICENSE).

Copyright © Encelade Technologies.
