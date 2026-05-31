//! Pure planning over the rc file contents.
//!
//! Nothing here touches the filesystem or the environment: every
//! decision is a function of the rc file text and a single boolean
//! describing whether a conflicting `lsh` executable is on the PATH.
//! Keeping the logic pure makes idempotency and conflict handling
//! testable with string literals.

use super::shell::{MARKER_BEGIN, MARKER_END};

/// What an `install` or `uninstall` would do, decided without any IO.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum AliasPlan {
    /// The managed block is already present; nothing to do.
    AlreadyPresent,
    /// The alias is absent and would be appended.
    WillAdd,
    /// A conflicting `lsh` executable is on the PATH; refuse to install.
    Conflict {
        /// Why the install is refused, shown to the user.
        reason: String,
    },
    /// The managed block is present and would be removed.
    WillRemove,
    /// Nothing to remove; the rc file has no managed block.
    NotPresent,
}

/// Decide what `install` would do.
pub(crate) fn plan_install(rc_contents: &str, lsh_on_path: bool) -> AliasPlan {
    if lsh_on_path {
        return AliasPlan::Conflict {
            reason: "an `lsh` executable is already on your PATH (the GNU lsh package); \
                     refusing to shadow it"
                .to_owned(),
        };
    }
    if rc_contents.contains(MARKER_BEGIN) {
        AliasPlan::AlreadyPresent
    } else {
        AliasPlan::WillAdd
    }
}

/// Decide what `uninstall` would do.
pub(crate) fn plan_uninstall(rc_contents: &str) -> AliasPlan {
    if rc_contents.contains(MARKER_BEGIN) {
        AliasPlan::WillRemove
    } else {
        AliasPlan::NotPresent
    }
}

/// Append the managed block to `rc_contents`, returning the new file
/// contents. A trailing newline is ensured before the block so it never
/// fuses with an existing last line.
pub(crate) fn with_block_added(rc_contents: &str, alias_line: &str) -> String {
    let mut out = String::with_capacity(rc_contents.len() + alias_line.len() + 96);
    out.push_str(rc_contents);
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out.push('\n');
    out.push_str(MARKER_BEGIN);
    out.push('\n');
    out.push_str(alias_line);
    out.push('\n');
    out.push_str(MARKER_END);
    out.push('\n');
    out
}

/// Remove the managed block from `rc_contents`, returning the new file
/// contents. The block boundary lines are dropped together with a single
/// blank separator line immediately preceding the block, if any.
pub(crate) fn with_block_removed(rc_contents: &str) -> String {
    let lines: Vec<&str> = rc_contents.lines().collect();
    let Some(begin) = lines.iter().position(|l| l.trim() == MARKER_BEGIN) else {
        return rc_contents.to_owned();
    };
    let end = lines
        .iter()
        .skip(begin)
        .position(|l| l.trim() == MARKER_END)
        .map_or(lines.len(), |offset| begin + offset + 1);

    // Drop a single blank line that separated the block from the body.
    let drop_from = if begin > 0 && lines[begin - 1].trim().is_empty() {
        begin - 1
    } else {
        begin
    };

    let kept: Vec<&str> = lines[..drop_from]
        .iter()
        .chain(lines.get(end..).into_iter().flatten())
        .copied()
        .collect();

    if kept.is_empty() {
        return String::new();
    }
    let mut out = kept.join("\n");
    if rc_contents.ends_with('\n') {
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::shell::Shell;
    use super::*;

    #[test]
    fn install_on_empty_file_adds_block() {
        assert_eq!(plan_install("", false), AliasPlan::WillAdd);
    }

    #[test]
    fn install_is_idempotent_once_present() {
        let rc = with_block_added("export A=1\n", &Shell::Zsh.alias_line());
        assert_eq!(plan_install(&rc, false), AliasPlan::AlreadyPresent);
    }

    #[test]
    fn install_refuses_when_lsh_on_path() {
        assert!(matches!(plan_install("", true), AliasPlan::Conflict { .. }));
    }

    #[test]
    fn uninstall_reports_not_present_on_clean_file() {
        assert_eq!(plan_uninstall("export A=1\n"), AliasPlan::NotPresent);
    }

    #[test]
    fn add_then_remove_round_trips_to_original() {
        let original = "export A=1\nexport B=2\n";
        let added = with_block_added(original, &Shell::Bash.alias_line());
        assert_eq!(plan_uninstall(&added), AliasPlan::WillRemove);
        assert_eq!(with_block_removed(&added), original);
    }

    #[test]
    fn add_block_preserves_trailing_newline_on_unterminated_file() {
        let added = with_block_added("export A=1", &Shell::Zsh.alias_line());
        assert!(added.starts_with("export A=1\n"));
        assert!(added.contains(MARKER_BEGIN));
        assert!(added.ends_with(&format!("{MARKER_END}\n")));
    }

    #[test]
    fn remove_on_file_without_block_is_a_no_op() {
        let rc = "export A=1\n";
        assert_eq!(with_block_removed(rc), rc);
    }

    #[test]
    fn added_block_contains_exactly_one_alias_line() {
        let line = Shell::Fish.alias_line();
        let added = with_block_added("", &line);
        assert_eq!(added.matches(&line).count(), 1);
    }
}
