//! Command-line argument handling (plan section 6), and telling the user
//! things before there is a window to tell them in.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// What the viewer was asked to do on launch.
#[derive(Debug, PartialEq, Eq)]
pub enum Launch {
    /// Open the window, with this file if there is one. The path is absolute
    /// against the invoking process's working directory. It may not exist;
    /// opening reports that clearly.
    Open(Option<PathBuf>),
    /// `--help`: print [`HELP`] and exit.
    Help,
    /// `--version`: print the version and exit.
    Version,
    /// The arguments make no sense; the message says why.
    Invalid(String),
}

pub const HELP: &str = "\
ezmark, a read-only markdown viewer

Usage: ezmark [FILE]

  FILE           the markdown file to open; without one, a window you can
                 drop a file onto
  -h, --help     show this help
  -V, --version  show the version
  --             treat everything after it as a file name
";

pub fn version() -> String {
    format!("ezmark {}", env!("CARGO_PKG_VERSION"))
}

/// Parse arguments (without the program name).
///
/// One file at most. `--` ends options, so a file may start with a dash. An
/// unknown `--option` is an error. An unknown single-dash argument is skipped,
/// because the OS adds its own on some launches (`-psn_...` from older macOS
/// launchers, `-NSDocumentRevisionsDebugMode` from Xcode). Relative paths
/// resolve against `cwd`, which for a forwarded second launch is the second
/// process's directory, not ours.
pub fn parse<I>(args: I, cwd: &Path) -> Launch
where
    I: IntoIterator<Item = OsString>,
{
    let mut options_done = false;
    let mut file: Option<OsString> = None;
    for arg in args {
        let text = arg.to_string_lossy();
        if !options_done && text.starts_with('-') && text.len() > 1 {
            match text.as_ref() {
                "--" => options_done = true,
                "-h" | "--help" => return Launch::Help,
                "-V" | "--version" => return Launch::Version,
                other if other.starts_with("--") => {
                    return Launch::Invalid(format!("unknown option {other}"));
                }
                _ => {}
            }
            continue;
        }
        if arg.is_empty() {
            continue;
        }
        if file.is_some() {
            return Launch::Invalid("ezmark opens one file at a time".to_owned());
        }
        file = Some(arg);
    }
    Launch::Open(file.map(|file| cwd.join(file)))
}

/// Tell the user something when there may be no window and no console.
///
/// A release build is a GUI-subsystem program on Windows, so started from
/// Explorer or a shortcut it has no standard streams, and an app started from
/// Finder has them pointed at `/dev/null`; text written to either goes
/// nowhere. Use the stream when there is one, and a native message box when
/// there is not.
pub fn tell(text: &str, error: bool) {
    if has_stream(error) {
        if error {
            eprintln!("{text}");
        } else {
            println!("{text}");
        }
    } else {
        message_box(text, error);
    }
}

#[cfg(windows)]
fn has_stream(error: bool) -> bool {
    use std::os::windows::io::AsRawHandle;
    let handle = if error {
        std::io::stderr().as_raw_handle()
    } else {
        std::io::stdout().as_raw_handle()
    };
    // No stream is a null handle, or INVALID_HANDLE_VALUE (-1).
    !handle.is_null() && handle.addr() != usize::MAX
}

/// An app started from Finder has its streams pointed at `/dev/null`.
#[cfg(unix)]
fn has_stream(error: bool) -> bool {
    use std::os::fd::AsFd;
    use std::os::unix::fs::MetadataExt;

    let stream = if error {
        std::io::stderr().as_fd().try_clone_to_owned()
    } else {
        std::io::stdout().as_fd().try_clone_to_owned()
    };
    let (Ok(stream), Ok(null)) = (stream, std::fs::metadata("/dev/null")) else {
        return false;
    };
    match std::fs::File::from(stream).metadata() {
        Ok(stream) => (stream.dev(), stream.ino()) != (null.dev(), null.ino()),
        Err(_) => false,
    }
}

#[cfg(not(any(windows, unix)))]
fn has_stream(_error: bool) -> bool {
    true
}

#[cfg(any(windows, target_os = "macos"))]
fn message_box(text: &str, error: bool) {
    let level = if error {
        rfd::MessageLevel::Error
    } else {
        rfd::MessageLevel::Info
    };
    rfd::MessageDialog::new()
        .set_title("ezmark")
        .set_description(text)
        .set_level(level)
        .show();
}

#[cfg(not(any(windows, target_os = "macos")))]
fn message_box(_text: &str, _error: bool) {}

/// The message for a failed start. A missing WebView2 runtime is the one
/// cause a user can fix, so it gets its own instructions (plan section 5).
pub fn startup_failure(error: &str) -> String {
    let mut message = format!("ezmark could not start.\n\n{error}");
    if error.to_ascii_lowercase().contains("webview2") {
        message.push_str(
            "\n\nezmark needs the Microsoft Edge WebView2 Runtime. Windows 11 includes it; \
             if it was removed, install the Evergreen runtime from \
             https://developer.microsoft.com/microsoft-edge/webview2/ and start ezmark again.",
        );
    }
    message
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(list: &[&str]) -> Launch {
        parse(list.iter().map(OsString::from), Path::new("w"))
    }

    fn opens(file: &str) -> Launch {
        Launch::Open(Some(Path::new("w").join(file)))
    }

    #[test]
    fn no_argument_means_empty_state() {
        assert_eq!(parsed(&[]), Launch::Open(None));
        assert_eq!(parsed(&[""]), Launch::Open(None));
    }

    #[test]
    fn arguments_the_os_adds_are_skipped() {
        assert_eq!(parsed(&["-psn_0_123"]), Launch::Open(None));
        assert_eq!(
            parsed(&["-NSDocumentRevisionsDebugMode", "a.md"]),
            opens("a.md")
        );
    }

    #[test]
    fn relative_paths_resolve_against_the_invoking_directory() {
        let cwd = Path::new("work dir");
        let launch = parse([OsString::from("docs/read me.md")], cwd);
        assert_eq!(launch, Launch::Open(Some(cwd.join("docs/read me.md"))));
    }

    #[test]
    fn absolute_paths_are_kept() {
        let absolute = std::env::temp_dir().join("ünïcödé ✓.md");
        let launch = parse(
            vec![absolute.clone().into_os_string()],
            Path::new("elsewhere"),
        );
        assert_eq!(launch, Launch::Open(Some(absolute)));
    }

    #[test]
    fn double_dash_allows_a_leading_dash() {
        assert_eq!(parsed(&["--", "-notes.md"]), opens("-notes.md"));
        assert_eq!(parsed(&["--", "--help"]), opens("--help"));
        // A lone dash is a file name, not an option.
        assert_eq!(parsed(&["-"]), opens("-"));
    }

    #[test]
    fn help_and_version_are_recognised() {
        for flag in ["-h", "--help"] {
            assert_eq!(parsed(&[flag]), Launch::Help);
            assert_eq!(parsed(&["a.md", flag]), Launch::Help);
        }
        for flag in ["-V", "--version"] {
            assert_eq!(parsed(&[flag]), Launch::Version);
        }
        assert!(HELP.contains("Usage: ezmark [FILE]"));
        assert_eq!(version(), format!("ezmark {}", env!("CARGO_PKG_VERSION")));
    }

    #[test]
    fn mistakes_are_reported_not_ignored() {
        assert_eq!(
            parsed(&["--verison"]),
            Launch::Invalid("unknown option --verison".to_owned())
        );
        assert_eq!(
            parsed(&["a.md", "b.md"]),
            Launch::Invalid("ezmark opens one file at a time".to_owned())
        );
    }

    #[test]
    fn a_missing_webview_runtime_gets_instructions() {
        let message = startup_failure("WebView2 error: runtime not found");
        assert!(message.starts_with("ezmark could not start."));
        assert!(message.contains("developer.microsoft.com/microsoft-edge/webview2"));
        assert!(!startup_failure("out of memory").contains("WebView2"));
    }
}
