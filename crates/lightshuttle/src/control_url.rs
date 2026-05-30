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

/// Read the URL recorded in `<cwd>/.lightshuttle/control.url`.
///
/// Trims trailing whitespace (typically the newline added by `write`)
/// and surfaces an error when the file is missing, empty, or does not
/// point at a loopback address. The loopback check stops a forged
/// discovery file from redirecting client requests to an arbitrary
/// host.
pub(crate) fn read(cwd: &Path) -> io::Result<String> {
    let path = path_at(cwd);
    let raw = fs::read_to_string(&path)?;
    let trimmed = raw.trim().to_owned();
    if trimmed.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{} is empty", path.display()),
        ));
    }
    if !is_loopback_url(&trimmed) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{} points at a non-loopback address: {trimmed}",
                path.display()
            ),
        ));
    }
    Ok(trimmed)
}

/// Whether `raw` is an `http` URL targeting the IPv4 or IPv6 loopback
/// with no embedded credentials.
///
/// The URL is parsed with the same crate `reqwest` uses, then the host
/// is checked structurally. String prefix matching is unsafe here: a
/// value like `http://127.0.0.1:80@evil.example/` would pass a prefix
/// check while resolving to `evil.example`.
fn is_loopback_url(raw: &str) -> bool {
    use std::net::{Ipv4Addr, Ipv6Addr};

    let Ok(parsed) = url::Url::parse(raw) else {
        return false;
    };
    if parsed.scheme() != "http" {
        return false;
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return false;
    }
    match parsed.host() {
        Some(url::Host::Ipv4(ip)) => ip == Ipv4Addr::LOCALHOST,
        Some(url::Host::Ipv6(ip)) => ip == Ipv6Addr::LOCALHOST,
        _ => false,
    }
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

    #[test]
    fn read_returns_the_recorded_url_without_trailing_newline() {
        let dir = tempdir().expect("tempdir");
        let url = "http://127.0.0.1:54321/";
        write(dir.path(), url).expect("write");

        let recovered = read(dir.path()).expect("read");
        assert_eq!(recovered, url);
    }

    #[test]
    fn read_errors_when_file_missing() {
        let dir = tempdir().expect("tempdir");
        let err = read(dir.path()).expect_err("missing file is an error");
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn read_rejects_a_non_loopback_url() {
        let dir = tempdir().expect("tempdir");
        write(dir.path(), "http://evil.example.com:80/").expect("write");

        let err = read(dir.path()).expect_err("non-loopback url is rejected");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn read_accepts_ipv6_loopback() {
        let dir = tempdir().expect("tempdir");
        let url = "http://[::1]:54321/";
        write(dir.path(), url).expect("write");

        assert_eq!(read(dir.path()).expect("read"), url);
    }

    #[test]
    fn loopback_check_resists_userinfo_smuggling() {
        // The host is actually `evil.example`; the loopback string is in
        // the userinfo. A prefix check would wrongly accept this.
        assert!(!is_loopback_url("http://127.0.0.1:80@evil.example/"));
        assert!(!is_loopback_url("http://127.0.0.1:@evil.example/"));
        // Credentials on a real loopback host are rejected too.
        assert!(!is_loopback_url("http://user:pass@127.0.0.1:8080/"));
        // Non-http schemes are rejected.
        assert!(!is_loopback_url("https://127.0.0.1:8080/"));
        assert!(!is_loopback_url("file:///etc/passwd"));
        // Genuine loopback URLs still pass.
        assert!(is_loopback_url("http://127.0.0.1:8080/"));
        assert!(is_loopback_url("http://[::1]:8080/"));
    }
}
