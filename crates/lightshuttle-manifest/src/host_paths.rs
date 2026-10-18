//! Resolution of relative host paths against the manifest directory.
//!
//! This module provides [`Manifest::resolve_host_paths`], which rewrites
//! every path a manifest declares on the developer's machine to an absolute
//! one, so the runtime receives unambiguous paths regardless of the directory
//! the command was invoked from. Two kinds of field qualify: the `src` half
//! of a `container` or `dockerfile` volume mapping, and the `working_dir` of
//! a `process`.
//!
//! Security: a relative path containing a `..` component is rejected, not
//! silently dropped, so a directory traversal attempt fails loudly instead of
//! surviving in the manifest. See [`ManifestError::InvalidVolumePath`] and
//! [`ManifestError::InvalidWorkingDirectory`].

use std::path::{Component, Path};

use crate::error::ManifestError;
use crate::model::{Manifest, ResourceKind};

impl Manifest {
    /// Resolve relative host paths in volume mappings against `base_dir`.
    ///
    /// Volume mappings are strings of the form `"src:container_path"`. When
    /// `src` starts with `.` it is treated as a path relative to the manifest
    /// file and is expanded to an absolute path by joining it onto `base_dir`.
    /// Absolute host paths and named volumes (e.g. `"dbdata:/var/lib/data"`)
    /// are left unchanged.
    ///
    /// A mapping whose relative `src` contains a `..` component is rejected
    /// with [`ManifestError::InvalidVolumePath`]: it could escape `base_dir`
    /// and mount an arbitrary host path into the container.
    ///
    /// Only `container` and `dockerfile` resources carry volume mappings.
    /// `postgres` and `redis` use the typed [`crate::Volume`] enum instead and are
    /// not touched by this method.
    ///
    /// A `process` resource carries no volumes, but its `working_dir` is a
    /// path on the developer's machine and is resolved here too. That is what
    /// distinguishes it from a container's `working_dir`, which names a
    /// directory inside the image and must be left exactly as written.
    ///
    /// Call this method after [`Manifest::parse`] and before handing the
    /// manifest to the runtime or export layers. Typically `base_dir` is the
    /// directory containing the `lightshuttle.yml` file.
    ///
    /// # Errors
    ///
    /// Returns [`ManifestError::InvalidVolumePath`] when a relative host mount
    /// tries to escape `base_dir` through a `..` component, or
    /// [`ManifestError::InvalidWorkingDirectory`] when a process working
    /// directory does.
    pub fn resolve_host_paths(&mut self, base_dir: &Path) -> Result<(), ManifestError> {
        for (name, kind) in &mut self.resources {
            let volumes = match kind {
                ResourceKind::Container(c) => &mut c.volumes,
                ResourceKind::Dockerfile(c) => &mut c.volumes,
                ResourceKind::Postgres(_) | ResourceKind::Redis(_) => continue,
                ResourceKind::Process(c) => {
                    if let Some(working_dir) = c.working_dir.as_mut() {
                        match classify_host_path(working_dir, base_dir) {
                            HostPath::Unchanged => {}
                            HostPath::Resolved(absolute) => *working_dir = absolute,
                            HostPath::Escaping => {
                                return Err(ManifestError::InvalidWorkingDirectory {
                                    resource: name.clone(),
                                    path: working_dir.clone(),
                                });
                            }
                        }
                    }
                    continue;
                }
            };
            for mapping in volumes.iter_mut() {
                if let Some(resolved) = resolve_mapping(mapping, base_dir)? {
                    *mapping = resolved;
                }
            }
        }
        Ok(())
    }

    /// Resolve relative host paths in volume mappings against `base_dir`.
    ///
    /// # Errors
    ///
    /// See [`Manifest::resolve_host_paths`].
    #[deprecated(
        since = "0.6.0",
        note = "renamed to `resolve_host_paths`: the pass also resolves a `process` working directory, which is not a volume"
    )]
    pub fn resolve_host_volume_paths(&mut self, base_dir: &Path) -> Result<(), ManifestError> {
        self.resolve_host_paths(base_dir)
    }
}

/// What examining one declared host path found.
///
/// Three outcomes, not two, and the third is the reason this is an enum and
/// not an `Option`: a path that escapes the base directory must be
/// distinguishable from one that simply needed no change, or a traversal
/// attempt would be indistinguishable from an absolute path and survive.
enum HostPath {
    /// Nothing to rewrite: an absolute host path, or a named volume.
    Unchanged,
    /// The relative path, expanded against the manifest directory.
    Resolved(String),
    /// The relative path leaves the manifest directory through `..`.
    Escaping,
}

/// Classify `path` as declared in the manifest, against `base_dir`.
///
/// Shared by volume mappings and by a `process` working directory so the two
/// cannot disagree on what counts as relative, or on what counts as an
/// escape. They differ only in the error each one reports.
fn classify_host_path(path: &str, base_dir: &Path) -> HostPath {
    if !path.starts_with('.') {
        return HostPath::Unchanged;
    }
    let relative = path.strip_prefix("./").unwrap_or(path);
    if Path::new(relative)
        .components()
        .any(|c| c == Component::ParentDir)
    {
        return HostPath::Escaping;
    }
    HostPath::Resolved(base_dir.join(relative).display().to_string())
}

/// Rewrite a `src:target` mapping whose `src` is a relative host path,
/// returning the absolute form.
///
/// - `Ok(Some(resolved))`: the relative `src` was rewritten to an absolute path.
/// - `Ok(None)`: nothing to change (named volume, absolute host path, or
///   malformed mapping).
/// - `Err(InvalidVolumePath)`: the relative `src` contains a `..` component and
///   is rejected to prevent directory traversal outside `base_dir`.
fn resolve_mapping(mapping: &str, base_dir: &Path) -> Result<Option<String>, ManifestError> {
    let Some((src, target)) = mapping.split_once(':') else {
        return Ok(None);
    };
    match classify_host_path(src, base_dir) {
        HostPath::Unchanged => Ok(None),
        HostPath::Resolved(absolute) => Ok(Some(format!("{absolute}:{target}"))),
        // Propagated as an error rather than dropped, so the caller cannot
        // mistake it for a mapping that was deliberately left unchanged.
        HostPath::Escaping => Err(ManifestError::InvalidVolumePath {
            mapping: mapping.to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn base() -> PathBuf {
        // An absolute base so the result is platform-correct on every OS.
        PathBuf::from(if cfg!(windows) {
            r"C:\project"
        } else {
            "/project"
        })
    }

    #[test]
    fn relative_source_is_resolved_against_base() {
        let expected = format!(
            "{}:/etc/demo.conf",
            base().join("config/demo.conf").display()
        );
        assert_eq!(
            resolve_mapping("./config/demo.conf:/etc/demo.conf", &base())
                .expect("a plain relative mount resolves without error")
                .as_deref(),
            Some(expected.as_str())
        );
    }

    #[test]
    fn parent_relative_source_is_rejected() {
        let error = resolve_mapping("../shared/x:/etc/x", &base())
            .expect_err("paths escaping the base directory via '..' must be rejected");
        assert!(
            matches!(error, ManifestError::InvalidVolumePath { .. }),
            "expected InvalidVolumePath, got {error:?}"
        );
    }

    #[test]
    fn embedded_traversal_is_rejected() {
        let error = resolve_mapping("./foo/../../etc/passwd:/etc/passwd", &base())
            .expect_err("embedded '..' traversal must be rejected");
        assert!(
            matches!(error, ManifestError::InvalidVolumePath { .. }),
            "expected InvalidVolumePath, got {error:?}"
        );
    }

    #[test]
    fn absolute_source_is_unchanged() {
        assert_eq!(
            resolve_mapping("/data/x:/etc/x", &base()).expect("absolute path is left as-is"),
            None
        );
    }

    #[test]
    fn named_source_is_unchanged() {
        assert_eq!(
            resolve_mapping("dbdata:/var/lib/data", &base()).expect("named volume is left as-is"),
            None
        );
    }

    #[test]
    fn resolve_only_touches_host_mounts() {
        let yaml = r"
project:
  name: app
resources:
  svc:
    container:
      image: alpine
      volumes:
        - ./config:/etc/config
        - cache:/var/cache
  db:
    postgres:
      version: '16'
      volume: dbdata
";
        let mut manifest = Manifest::parse(yaml).expect("parses");
        manifest
            .resolve_host_paths(&base())
            .expect("no traversal in this manifest");

        let ResourceKind::Container(svc) = &manifest.resources["svc"] else {
            panic!("svc is a container");
        };
        let expected = format!("{}:/etc/config", base().join("config").display());
        assert_eq!(svc.volumes[0], expected, "relative host mount resolved");
        assert_eq!(svc.volumes[1], "cache:/var/cache", "named volume untouched");
    }

    #[test]
    fn manifest_rejects_a_traversal_host_mount() {
        let yaml = r"
project:
  name: app
resources:
  svc:
    container:
      image: alpine
      volumes:
        - ./foo/../../etc/passwd:/etc/passwd
";
        let mut manifest = Manifest::parse(yaml).expect("parses");
        let error = manifest
            .resolve_host_paths(&base())
            .expect_err("a traversal mount must be rejected, not left in the manifest");
        assert!(
            matches!(error, ManifestError::InvalidVolumePath { .. }),
            "expected InvalidVolumePath, got {error:?}"
        );
    }

    #[test]
    fn internal_traversal_within_base_is_still_rejected() {
        // `./subdir/../allowed` collapses to a path that stays within the base
        // directory, yet the policy rejects any `..` component outright rather
        // than reasoning about where the path finally lands. A legitimate
        // manifest writes `./allowed`, never `./subdir/../allowed`.
        let error = resolve_mapping("./subdir/../allowed:/etc/allowed", &base())
            .expect_err("any '..' component is rejected, even one that stays within base");
        assert!(
            matches!(error, ManifestError::InvalidVolumePath { .. }),
            "expected InvalidVolumePath, got {error:?}"
        );
    }
}
