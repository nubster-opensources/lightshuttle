//! Resolution of relative host volume paths against the manifest
//! directory.

use std::path::Path;

use crate::model::{Manifest, ResourceKind};

impl Manifest {
    /// Resolve relative host volume paths against `base_dir`.
    ///
    /// `container` and `dockerfile` resources may mount a host path with
    /// a `src:target` volume mapping. A `src` that starts with `.` (such
    /// as `./config` or `../shared`) is a path relative to the manifest
    /// file; it is rewritten to an absolute path joined onto `base_dir`,
    /// so the runtime and the exporters receive a path Docker accepts.
    /// Absolute host paths and named volumes are left unchanged.
    ///
    /// This mirrors the rule documented in `docs/spec/manifest-v0.md`:
    /// relative host paths are resolved against the manifest directory.
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
/// (named volume, absolute host path, or malformed mapping).
fn resolve_mapping(mapping: &str, base_dir: &Path) -> Option<String> {
    let (src, target) = mapping.split_once(':')?;
    if !src.starts_with('.') {
        return None;
    }
    let relative = src.strip_prefix("./").unwrap_or(src);
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
    fn parent_relative_source_is_resolved() {
        let expected = format!("{}:/etc/x", base().join("../shared/x").display());
        assert_eq!(
            resolve_mapping("../shared/x:/etc/x", &base()).as_deref(),
            Some(expected.as_str())
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
