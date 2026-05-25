# Semantic Versioning policy

LightShuttle follows [Semantic Versioning 2.0.0](https://semver.org/) with explicit conventions for the 0.x phase.

## 0.x phase (pre-1.0)

While the major version is 0, breaking changes are allowed on a minor version bump:

- `0.1.x` -> `0.1.y` (patch): bug fixes, performance improvements, internal refactors, additive non-breaking changes. No public API change observable by a downstream user.
- `0.x.y` -> `0.X.0` (minor): may introduce breaking changes. Removed items must have been deprecated for at least one previous minor release whenever feasible.

Reasoning: every component of LightShuttle is shipped early to gather feedback. Locking ourselves into Semver-strict semantics before the API surface is stable would prevent the changes we know we still need.

## 1.0 and beyond

Once 1.0 is reached, LightShuttle commits to strict Semver:

- Major (`X.0.0`): breaking changes to the public API.
- Minor (`1.Y.0`): backwards-compatible additions.
- Patch (`1.x.Z`): backwards-compatible bug fixes.

## Public API definition

The public API consists of every item that is reachable from a crate's root through `pub` re-exports, except items marked `#[doc(hidden)]`. This includes:

- Public types, traits, functions, constants and modules.
- Trait method signatures and associated types.
- Public re-exports from sibling crates (the facade crate `lightshuttle` re-exports curated items).
- The on-disk manifest schema produced by `lightshuttle-manifest` and exposed via `cargo xtask schema`. Schema changes that break previously valid manifests are treated as breaking changes.
- The `lsh` CLI surface: command names, flags, exit codes, and the documented shape of human-readable and machine-readable output.

Items that are explicitly NOT part of the public API:

- Anything under a module annotated `#[doc(hidden)]`.
- Test-only helpers under `#[cfg(test)]`.
- The exact wording of log lines, progress output, or error diagnostic formatting, provided that error categories and machine-readable fields remain stable.
- Internal runtime details (container naming, intermediate file layout under the LightShuttle work directory) that are not part of the documented contract.

## Deprecation cycle

When an item is to be removed:

1. The item is marked `#[deprecated(since = "0.X.0", note = "use Y instead")]` in the release that introduces the replacement.
2. The deprecated item continues to compile and run unchanged for the entire next minor cycle.
3. The item is removed in the minor release after that, at the earliest. Removal is documented in CHANGELOG.md under `Removed` for that version.

## Breaking change documentation

Every breaking change is announced in CHANGELOG.md under `Changed` or `Removed`, with:

- The new signature or replacement.
- A migration snippet when the change is non-mechanical.
- A link to the relevant pull request or discussion when context is useful.

## MSRV

The MSRV (Minimum Supported Rust Version) is governed by [MSRV_POLICY.md](MSRV_POLICY.md). An MSRV bump is treated as a minor version bump.
