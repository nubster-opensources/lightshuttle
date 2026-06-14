//! Resolution of relative host volume paths against the manifest directory.
//!
//! This module provides [`Manifest::resolve_host_volume_paths`], which
//! rewrites relative `src` paths in `container` and `dockerfile` volume
//! mappings to absolute paths so the container runtime receives unambiguous
//! paths regardless of the process working directory.
//!
//! Security: paths containing `..` components are silently dropped to
//! prevent directory traversal outside `base_dir`.

use std::path::{Component, Path};

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
    /// Paths that contain `..` components are silently dropped because they
    /// could escape `base_dir` and create unexpected host mounts.
    ///
    /// Only `container` and `dockerfile` resources carry volume mappings.
    /// `postgres` and `redis` use the typed [`crate::Volume`] enum instead and are
    /// not touched by this method.
    ///
    /// Call this method after [`Manifest::parse`] and before handing the
    /// manifest to the runtime or export layers. Typically `base_dir` is the
    /// directory containing the `lightshuttle.yml` file.
    pub fn resolve_host_volume_paths(&mut self, base_dir: &Path) {
        for kind in self.resources.values_mut() {
            let volumes = match kind {
                ResourceKind::Container(c) => &mut c.volumes,
                ResourceKind::Dockerfile(c) => &mut c.volumes,
                ResourceKind::Postgres(_) | ResourceKind::Redis(_) => continue,
            };
            for mapping in volumes.iter_mut() {
                if let Some(resolved) = resolve_mapping(mapping, base_dir) {
                    *mapping = resolved;
                }
            }
        }
    }
}

/// Rewrite a `src:target` mapping whose `src` is a relative host path,
/// returning the absolute form. Returns `None` when nothing changes
/// (named volume, absolute host path, or malformed mapping) or when the
/// resolved path would escape `base_dir` via `..` components.
fn resolve_mapping(mapping: &str, base_dir: &Path) -> Option<String> {
    let (src, target) = mapping.split_once(':')?;
    if !src.starts_with('.') {
        return None;
    }
    let relative = src.strip_prefix("./").unwrap_or(src);
    // Reject any path that contains '..' to prevent directory traversal.
    if Path::new(relative)
        .components()
        .any(|c| c == Component::ParentDir)
    {
        return None;
    }
    let absolute = base_dir.join(relative);
    Some(format!("{}:{target}", absolute.display()))
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
            resolve_mapping("./config/demo.conf:/etc/demo.conf", &base()).as_deref(),
            Some(expected.as_str())
        );
    }

    #[test]
    fn parent_relative_source_is_rejected() {
        assert_eq!(
            resolve_mapping("../shared/x:/etc/x", &base()),
            None,
            "paths escaping the base directory via '..' must be rejected"
        );
    }

    #[test]
    fn embedded_traversal_is_rejected() {
        assert_eq!(
            resolve_mapping("./foo/../../etc/passwd:/etc/passwd", &base()),
            None,
            "embedded '..' traversal must be rejected"
        );
    }

    #[test]
    fn absolute_source_is_unchanged() {
        assert_eq!(resolve_mapping("/data/x:/etc/x", &base()), None);
    }

    #[test]
    fn named_source_is_unchanged() {
        assert_eq!(resolve_mapping("dbdata:/var/lib/data", &base()), None);
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
        manifest.resolve_host_volume_paths(&base());

        let ResourceKind::Container(svc) = &manifest.resources["svc"] else {
            panic!("svc is a container");
        };
        let expected = format!("{}:/etc/config", base().join("config").display());
        assert_eq!(svc.volumes[0], expected, "relative host mount resolved");
        assert_eq!(svc.volumes[1], "cache:/var/cache", "named volume untouched");
    }
}
