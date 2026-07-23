//! Resolution of relative host volume paths against the manifest directory.
//!
//! This module provides [`Manifest::resolve_host_volume_paths`], which
//! rewrites relative `src` paths in `container` and `dockerfile` volume
//! mappings to absolute paths so the container runtime receives unambiguous
//! paths regardless of the process working directory.
//!
//! Security: a relative `src` containing a `..` component is rejected with
//! [`ManifestError::InvalidVolumePath`], not silently dropped, so a directory
//! traversal attempt fails loudly instead of surviving in the manifest.

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
    /// Call this method after [`Manifest::parse`] and before handing the
    /// manifest to the runtime or export layers. Typically `base_dir` is the
    /// directory containing the `lightshuttle.yml` file.
    ///
    /// # Errors
    ///
    /// Returns [`ManifestError::InvalidVolumePath`] when a relative host mount
    /// tries to escape `base_dir` through a `..` component.
    pub fn resolve_host_volume_paths(&mut self, base_dir: &Path) -> Result<(), ManifestError> {
        for kind in self.resources.values_mut() {
            let volumes = match kind {
                ResourceKind::Container(c) => &mut c.volumes,
                ResourceKind::Dockerfile(c) => &mut c.volumes,
                ResourceKind::Postgres(_) | ResourceKind::Redis(_) => continue,
            };
            for mapping in volumes.iter_mut() {
                if let Some(resolved) = resolve_mapping(mapping, base_dir)? {
                    *mapping = resolved;
                }
            }
        }
        Ok(())
    }
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
    if !src.starts_with('.') {
        return Ok(None);
    }
    let relative = src.strip_prefix("./").unwrap_or(src);
    // Reject any path that contains '..' to prevent directory traversal. The
    // rejection is propagated as an error rather than dropped, so the caller
    // cannot mistake it for a mapping that was deliberately left unchanged.
    if Path::new(relative)
        .components()
        .any(|c| c == Component::ParentDir)
    {
        return Err(ManifestError::InvalidVolumePath {
            mapping: mapping.to_string(),
        });
    }
    let absolute = base_dir.join(relative);
    Ok(Some(format!("{}:{target}", absolute.display())))
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
            .resolve_host_volume_paths(&base())
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
            .resolve_host_volume_paths(&base())
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
