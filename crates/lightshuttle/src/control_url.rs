//! Discovery file for the local control plane.
//!
//! `lightshuttle up` writes `.lightshuttle/control.url` in the current
//! working directory so future client commands (`restart`, `ps`) can
//! locate the running orchestrator without re-passing the port.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const DIR_NAME: &str = ".lightshuttle";
const FILE_NAME: &str = "control.url";

/// Path to the discovery file, anchored at `cwd`.
fn path_at(cwd: &Path) -> PathBuf {
    cwd.join(DIR_NAME).join(FILE_NAME)
}

/// Write the resolved URL to `<cwd>/.lightshuttle/control.url`.
///
/// Creates the parent directory if missing. Overwrites any previous
/// file at the same location.
pub(crate) fn write(cwd: &Path, url: &str) -> io::Result<PathBuf> {
    let path = path_at(cwd);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut contents = String::with_capacity(url.len() + 1);
    contents.push_str(url);
    contents.push('\n');
    fs::write(&path, contents)?;
    Ok(path)
}

/// Remove the discovery file if it exists. Errors other than
/// `NotFound` are surfaced; missing files are silently treated as a
/// no-op so a partial shutdown stays idempotent.
pub(crate) fn remove(cwd: &Path) -> io::Result<()> {
    let path = path_at(cwd);
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn write_then_remove_round_trip() {
        let dir = tempdir().expect("tempdir");
        let url = "http://127.0.0.1:54321/";

        let written = write(dir.path(), url).expect("write");
        assert!(written.exists());
        let read_back = fs::read_to_string(&written).expect("read");
        assert_eq!(read_back, format!("{url}\n"));

        remove(dir.path()).expect("remove");
        assert!(!written.exists());
    }

    #[test]
    fn remove_is_idempotent_when_file_missing() {
        let dir = tempdir().expect("tempdir");
        remove(dir.path()).expect("remove on empty dir is ok");
    }
}
