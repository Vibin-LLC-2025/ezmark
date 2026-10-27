//! The one shared validator for every local reference a document makes.
//!
//! Image rewriting, the asset protocol handler, and (from M1) link resolution
//! and image-header reads all call [`validate_reference`]. The order of steps
//! is part of the security contract (plan section 7):
//!
//! 1. A cheap gate on the raw input: no NUL, no URL scheme.
//! 2. Percent-decode exactly once, strictly. Nothing downstream decodes again,
//!    so `%255C` becomes the literal three characters `%5C`.
//! 3. Inspect the decoded string before touching the filesystem: reject
//!    schemes, UNC and device paths, NUL, and (on Windows) drive-relative
//!    paths, alternate data streams, and reserved device names. Canonicalizing
//!    a network path can itself cause network access, so this must come first.
//! 4. Join to the document's directory, canonicalize (symlinks and junctions
//!    resolved), and require the result to stay under the active root.

use std::io;
use std::path::{Path, PathBuf};

/// Why a reference was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rejection {
    /// The reference is empty.
    Empty,
    /// The reference contains a NUL character, raw or decoded.
    Nul,
    /// The reference names a URL scheme (`file:`, `https:`, ...), raw or decoded.
    Scheme,
    /// A `%` that does not start a two-digit hex escape, or decoded bytes that
    /// are not UTF-8.
    MalformedEscape,
    /// A UNC (`\\server\share`, `//server/share`) or device (`\\?\`, `\\.\`) path.
    NetworkOrDevicePath,
    /// Windows only: a drive-relative path (`C:file`), an alternate data stream
    /// (`a.png:stream`), or a reserved device name (`NUL`, `COM1`, ...).
    UnsupportedWindowsPath,
    /// The target does not exist or could not be canonicalized.
    NotFound,
    /// The canonical target lies outside the active root.
    OutsideRoot,
}

impl std::fmt::Display for Rejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            Rejection::Empty => "empty reference",
            Rejection::Nul => "reference contains NUL",
            Rejection::Scheme => "reference is not a local path",
            Rejection::MalformedEscape => "reference has a malformed percent escape",
            Rejection::NetworkOrDevicePath => "network and device paths are not allowed",
            Rejection::UnsupportedWindowsPath => "unsupported Windows path form",
            Rejection::NotFound => "file not found",
            Rejection::OutsideRoot => "file is outside the document root",
        };
        f.write_str(text)
    }
}

impl std::error::Error for Rejection {}

/// The filesystem calls the validator makes. Tests substitute a recorder to
/// prove that rejected input never reaches the filesystem.
pub trait Fs {
    /// Resolve `path` to an absolute path with symlinks and junctions resolved.
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf>;
}

/// The real filesystem.
#[derive(Debug, Clone, Copy, Default)]
pub struct RealFs;

impl Fs for RealFs {
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        std::fs::canonicalize(path)
    }
}

/// Split a raw reference into its path part and its fragment, dropping any
/// query. Splitting happens before decoding, so `%23` stays part of a filename.
pub fn split_reference(raw: &str) -> (&str, Option<&str>) {
    let (before_fragment, fragment) = match raw.split_once('#') {
        Some((path, fragment)) => (path, Some(fragment)),
        None => (raw, None),
    };
    let path = before_fragment
        .split_once('?')
        .map_or(before_fragment, |(path, _query)| path);
    (path, fragment)
}

/// Validate the path part of a reference made by the document in `doc_dir`.
///
/// `doc_dir` and `root` must already be canonical. Returns the canonical
/// target, which is guaranteed to lie under `root` at the time of the call.
pub fn validate_reference(
    raw: &str,
    doc_dir: &Path,
    root: &Path,
    fs: &dyn Fs,
) -> Result<PathBuf, Rejection> {
    raw_gate(raw)?;
    let decoded = decode_once(raw)?;
    inspect(&decoded)?;
    let canonical = fs
        .canonicalize(&doc_dir.join(&decoded))
        .map_err(|_| Rejection::NotFound)?;
    if canonical.starts_with(root) {
        Ok(canonical)
    } else {
        Err(Rejection::OutsideRoot)
    }
}

/// Step 1: a cheap first gate on the undecoded input.
fn raw_gate(raw: &str) -> Result<(), Rejection> {
    if raw.is_empty() {
        return Err(Rejection::Empty);
    }
    if raw.contains('\0') {
        return Err(Rejection::Nul);
    }
    if names_scheme(raw) {
        return Err(Rejection::Scheme);
    }
    Ok(())
}

/// Step 2: percent-decode exactly once. A `%` must be followed by two hex
/// digits; anything else is refused rather than guessed at.
pub fn decode_once(raw: &str) -> Result<String, Rejection> {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hi = bytes.get(i + 1).and_then(|b| hex_value(*b));
            let lo = bytes.get(i + 2).and_then(|b| hex_value(*b));
            match (hi, lo) {
                (Some(hi), Some(lo)) => out.push(hi << 4 | lo),
                _ => return Err(Rejection::MalformedEscape),
            }
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| Rejection::MalformedEscape)
}

fn hex_value(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Step 3: inspect the decoded string. No filesystem access happens here.
fn inspect(decoded: &str) -> Result<(), Rejection> {
    if decoded.is_empty() {
        return Err(Rejection::Empty);
    }
    if decoded.contains('\0') {
        return Err(Rejection::Nul);
    }
    let mut chars = decoded.chars();
    // `\\server\share`, `//server/share`, `\\?\C:\`, `\\.\pipe\x`, mixed forms.
    if let (Some(a), Some(b)) = (chars.next(), chars.next())
        && is_separator(a)
        && is_separator(b)
    {
        return Err(Rejection::NetworkOrDevicePath);
    }
    if names_scheme(decoded) {
        return Err(Rejection::Scheme);
    }
    inspect_platform(decoded)
}

fn is_separator(c: char) -> bool {
    c == '/' || c == '\\'
}

/// True if `s` starts with an RFC 3986 scheme (`alpha *(alpha / digit / + - .) :`).
///
/// A single letter followed by `:` is a Windows drive, which
/// [`inspect_platform`] handles, so it is not treated as a scheme here.
fn names_scheme(s: &str) -> bool {
    let Some((candidate, _)) = s.split_once(':') else {
        return false;
    };
    let mut chars = candidate.chars();
    let starts_alpha = chars.next().is_some_and(|c| c.is_ascii_alphabetic());
    starts_alpha
        && candidate.len() > 1
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

#[cfg(windows)]
fn inspect_platform(decoded: &str) -> Result<(), Rejection> {
    let bytes = decoded.as_bytes();
    let rest = if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        // `C:\x` and `C:/x` are absolute local paths; `C:x` is drive-relative.
        match bytes.get(2) {
            Some(b'\\' | b'/') => &decoded[2..],
            _ => return Err(Rejection::UnsupportedWindowsPath),
        }
    } else {
        decoded
    };
    if rest.contains(':') {
        // Alternate data streams and anything else Win32 treats specially.
        return Err(Rejection::UnsupportedWindowsPath);
    }
    if rest.split(['/', '\\']).any(is_reserved_device_name) {
        return Err(Rejection::UnsupportedWindowsPath);
    }
    Ok(())
}

#[cfg(not(windows))]
fn inspect_platform(decoded: &str) -> Result<(), Rejection> {
    let bytes = decoded.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        // A one-letter scheme as far as a browser is concerned.
        return Err(Rejection::Scheme);
    }
    Ok(())
}

/// `CON`, `NUL`, `COM1`, `LPT1.txt`, `conin$` and friends open devices, not files.
#[cfg(windows)]
fn is_reserved_device_name(component: &str) -> bool {
    let trimmed = component.trim_end_matches([' ', '.']);
    let stem = trimmed.split('.').next().unwrap_or(trimmed).trim_end();
    let upper = stem.to_ascii_uppercase();
    match upper.as_str() {
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$" => true,
        _ => {
            // `get` rather than slicing: byte 3 may fall inside a multibyte char.
            let Some(prefix) = upper.get(..3) else {
                return false;
            };
            matches!(prefix, "COM" | "LPT")
                && matches!(
                    &upper[3..],
                    "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// Records every path the validator asks the filesystem about.
    #[derive(Default)]
    struct RecordingFs {
        calls: RefCell<Vec<PathBuf>>,
    }

    impl Fs for RecordingFs {
        fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
            self.calls.borrow_mut().push(path.to_path_buf());
            Err(io::Error::from(io::ErrorKind::NotFound))
        }
    }

    fn rejected_before_fs(raw: &str) -> Rejection {
        let fs = RecordingFs::default();
        let doc = Path::new("doc");
        let err = validate_reference(raw, doc, doc, &fs).expect_err(raw);
        assert!(
            fs.calls.borrow().is_empty(),
            "{raw:?} reached the filesystem: {:?}",
            fs.calls.borrow()
        );
        err
    }

    #[test]
    fn decodes_exactly_once() {
        assert_eq!(decode_once("my%20pic.png").unwrap(), "my pic.png");
        assert_eq!(decode_once("%255C").unwrap(), "%5C");
        assert_eq!(decode_once("caf%C3%A9.md").unwrap(), "café.md");
        assert_eq!(decode_once("100%"), Err(Rejection::MalformedEscape));
        assert_eq!(decode_once("%zz"), Err(Rejection::MalformedEscape));
        assert_eq!(decode_once("%FF"), Err(Rejection::MalformedEscape));
    }

    #[test]
    fn splits_fragment_and_query_before_decoding() {
        assert_eq!(split_reference("a.md#intro"), ("a.md", Some("intro")));
        assert_eq!(split_reference("a.png?raw=true"), ("a.png", None));
        assert_eq!(split_reference("a%23b.md"), ("a%23b.md", None));
    }

    #[test]
    fn network_and_device_paths_never_reach_the_filesystem() {
        for raw in [
            r"\\server\share\x.png",
            "//server/share/x.png",
            r"\\?\C:\x.png",
            r"\\.\pipe\x",
            r"\/server/x.png",
            "%5C%5Cserver%5Cshare%5Cx.png",
            "%2F%2Fserver%2Fshare",
            "%5C%5C%3F%5CC:%5Cx.png",
            "%5c%5c.%5cpipe%5cx",
        ] {
            assert_eq!(
                rejected_before_fs(raw),
                Rejection::NetworkOrDevicePath,
                "{raw}"
            );
        }
    }

    #[test]
    fn schemes_never_reach_the_filesystem() {
        for raw in [
            "file:///etc/passwd",
            "https://example.com/x.png",
            "javascript:alert(1)",
            "file%3A%2F%2F%2Fetc%2Fpasswd",
            "data:image/png;base64,AAAA",
        ] {
            assert_eq!(rejected_before_fs(raw), Rejection::Scheme, "{raw}");
        }
    }

    #[test]
    fn nul_and_malformed_input_never_reach_the_filesystem() {
        assert_eq!(rejected_before_fs("a\0.png"), Rejection::Nul);
        assert_eq!(rejected_before_fs("a%00.png"), Rejection::Nul);
        assert_eq!(rejected_before_fs("50%.png"), Rejection::MalformedEscape);
        assert_eq!(rejected_before_fs(""), Rejection::Empty);
    }

    #[cfg(windows)]
    #[test]
    fn windows_special_forms_never_reach_the_filesystem() {
        for raw in [
            "C:x.png",
            "img/a.png:Zone.Identifier",
            "NUL",
            "img/con.png",
            "COM1.md",
            "lpt9",
            "CONIN$",
            "nul%2Etxt",
        ] {
            assert_eq!(
                rejected_before_fs(raw),
                Rejection::UnsupportedWindowsPath,
                "{raw}"
            );
        }
        // Multibyte names that merely resemble device names are ordinary files.
        for raw in ["éé.png", "COMé.png"] {
            let fs = RecordingFs::default();
            let doc = Path::new("doc");
            let err = validate_reference(raw, doc, doc, &fs).unwrap_err();
            assert_eq!(err, Rejection::NotFound, "{raw}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn windows_absolute_drive_paths_are_local() {
        for raw in [r"C:\nowhere\x.png", "C:/nowhere/x.png"] {
            let fs = RecordingFs::default();
            let doc = Path::new("doc");
            let err = validate_reference(raw, doc, doc, &fs).unwrap_err();
            assert_eq!(err, Rejection::NotFound, "{raw}");
            assert_eq!(fs.calls.borrow().len(), 1, "{raw}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn windows_special_forms_are_case_insensitive() {
        for raw in ["Nul", "coM3.txt", "Lpt1 .md"] {
            assert_eq!(
                rejected_before_fs(raw),
                Rejection::UnsupportedWindowsPath,
                "{raw}"
            );
        }
    }

    #[test]
    fn double_encoded_backslash_is_an_ordinary_filename() {
        // `%255C` decodes once to the literal `%5C`, which is then just a name
        // and goes through the normal root check.
        let fs = RecordingFs::default();
        let doc = Path::new("doc");
        let err = validate_reference("%255C%255Cserver", doc, doc, &fs).unwrap_err();
        assert_eq!(err, Rejection::NotFound);
        assert_eq!(*fs.calls.borrow(), vec![doc.join("%5C%5Cserver")]);
    }

    #[test]
    fn real_filesystem_enforces_the_root() {
        let base = std::env::temp_dir().join(format!("mdcore-boundary-{}", std::process::id()));
        let root = base.join("root");
        std::fs::create_dir_all(root.join("img")).unwrap();
        std::fs::write(root.join("img").join("a b.png"), b"x").unwrap();
        std::fs::write(base.join("secret.png"), b"x").unwrap();
        let root = std::fs::canonicalize(&root).unwrap();

        let inside = validate_reference("img/a%20b.png", &root, &root, &RealFs).unwrap();
        assert!(inside.starts_with(&root));
        assert_eq!(
            validate_reference("../secret.png", &root, &root, &RealFs),
            Err(Rejection::OutsideRoot)
        );
        assert_eq!(
            validate_reference("img/..%2F..%2Fsecret.png", &root, &root, &RealFs),
            Err(Rejection::OutsideRoot)
        );
        assert_eq!(
            validate_reference("missing.png", &root, &root, &RealFs),
            Err(Rejection::NotFound)
        );
        std::fs::remove_dir_all(&base).unwrap();
    }
}
