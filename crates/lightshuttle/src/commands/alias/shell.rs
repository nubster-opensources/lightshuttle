//! The shell as a domain type: detection and per-shell syntax.

use clap::ValueEnum;

/// A shell whose startup file can carry the `lsh` alias.
///
/// `PowerShell` legitimately ends with the enum name, so the
/// `enum_variant_names` lint is silenced for this type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[allow(clippy::enum_variant_names)]
pub(crate) enum Shell {
    /// GNU Bash.
    Bash,
    /// Z shell.
    Zsh,
    /// Fish shell.
    Fish,
    /// Windows `PowerShell` or `PowerShell` 7+.
    #[value(name = "powershell", alias = "pwsh")]
    PowerShell,
}

/// Opening sentinel of the block LightShuttle manages in the rc file.
pub(crate) const MARKER_BEGIN: &str = "# >>> lightshuttle alias >>>";

/// Closing sentinel of the managed block.
pub(crate) const MARKER_END: &str = "# <<< lightshuttle alias <<<";

impl Shell {
    /// Best-effort detection from the environment.
    ///
    /// On Unix the basename of `$SHELL` selects the shell. On Windows
    /// `PowerShell` is assumed. Returns `None` when the shell cannot be
    /// determined, leaving the caller to ask for an explicit `--shell`.
    pub(crate) fn detect() -> Option<Self> {
        if cfg!(windows) {
            return Some(Self::PowerShell);
        }
        let shell = std::env::var("SHELL").ok()?;
        let name = shell.rsplit(['/', '\\']).next().unwrap_or(&shell);
        match name {
            n if n.contains("bash") => Some(Self::Bash),
            n if n.contains("zsh") => Some(Self::Zsh),
            n if n.contains("fish") => Some(Self::Fish),
            _ => None,
        }
    }

    /// The alias definition line written in this shell's own syntax.
    pub(crate) fn alias_line(self) -> String {
        match self {
            Self::Bash | Self::Zsh => "alias lsh='lightshuttle'".to_owned(),
            Self::Fish => "alias lsh 'lightshuttle'".to_owned(),
            Self::PowerShell => "Set-Alias lsh lightshuttle".to_owned(),
        }
    }

    /// Human-readable label, e.g. `"zsh"`.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Bash => "bash",
            Self::Zsh => "zsh",
            Self::Fish => "fish",
            Self::PowerShell => "PowerShell",
        }
    }
}
