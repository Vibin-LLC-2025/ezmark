//! Command-line argument handling (plan section 6).

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// What the viewer was asked to open on launch.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Launch {
    /// The file to open, made absolute against the invoking process's working
    /// directory. It may not exist; opening reports that clearly.
    pub path: Option<PathBuf>,
}

/// Parse arguments (without the program name).
///
/// The first argument that is not a flag is the file. Flags are skipped so a
/// stray `-psn_...` from older macOS launchers is harmless; `--` ends flags.
/// Relative paths resolve against `cwd`, which for a forwarded second launch is
/// the second process's directory, not ours.
pub fn parse<I>(args: I, cwd: &Path) -> Launch
where
    I: IntoIterator<Item = OsString>,
{
    let mut flags_done = false;
    for arg in args {
        if !flags_done && arg == "--" {
            flags_done = true;
            continue;
        }
        if !flags_done && arg.to_string_lossy().starts_with('-') {
            continue;
        }
        if arg.is_empty() {
            continue;
        }
        return Launch {
            path: Some(cwd.join(arg)),
        };
    }
    Launch::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<OsString> {
        list.iter().map(OsString::from).collect()
    }

    #[test]
    fn no_argument_means_empty_state() {
        assert_eq!(parse(args(&[]), Path::new("/w")), Launch::default());
        assert_eq!(
            parse(args(&["-psn_0_123"]), Path::new("/w")),
            Launch::default()
        );
    }

    #[test]
    fn relative_paths_resolve_against_the_invoking_directory() {
        let cwd = Path::new("work dir");
        let launch = parse(args(&["docs/read me.md"]), cwd);
        assert_eq!(launch.path, Some(cwd.join("docs/read me.md")));
    }

    #[test]
    fn absolute_paths_are_kept() {
        let absolute = std::env::temp_dir().join("ünïcödé ✓.md");
        let launch = parse(
            vec![absolute.clone().into_os_string()],
            Path::new("elsewhere"),
        );
        assert_eq!(launch.path, Some(absolute));
    }

    #[test]
    fn double_dash_allows_a_leading_dash() {
        let launch = parse(args(&["--", "-notes.md"]), Path::new("w"));
        assert_eq!(launch.path, Some(Path::new("w").join("-notes.md")));
    }
}
