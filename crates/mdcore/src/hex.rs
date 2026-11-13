//! Lowercase hex, the transport encoding for anything a document supplied
//! that has to travel inside a URL (asset references, blocked-image markers).
//! Hex has no characters a webview would normalise, re-encode, or split on.

/// `text` as lowercase hex, two digits per UTF-8 byte.
pub fn encode(text: &str) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(text.len() * 2);
    for byte in text.bytes() {
        out.push(char::from(DIGITS[usize::from(byte >> 4)]));
        out.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    out
}

/// The text `hex` encodes, or `None` unless it is a non-empty, even-length run
/// of hex digits that decodes to UTF-8.
pub fn decode(hex: &str) -> Option<String> {
    let (pairs, odd) = hex.as_bytes().as_chunks::<2>();
    if pairs.is_empty() || !odd.is_empty() {
        return None;
    }
    let decoded = pairs
        .iter()
        .map(|[hi, lo]| Some(digit(*hi)? << 4 | digit(*lo)?))
        .collect::<Option<Vec<u8>>>()?;
    String::from_utf8(decoded).ok()
}

/// The value of one hex digit, either case.
pub(crate) fn digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        for text in [
            "img/a.png",
            "../x.png",
            "my%20pic.png",
            "ünï ✓.png",
            "%5C%5Cserver",
        ] {
            assert_eq!(decode(&encode(text)).as_deref(), Some(text));
        }
        assert_eq!(encode("a/"), "612f");
    }

    #[test]
    fn refuses_anything_that_is_not_plain_hex() {
        for hex in ["", "6", "616", "zz", "ff", "+f", "6 ", "0x61"] {
            assert_eq!(decode(hex), None, "{hex}");
        }
        assert_eq!(decode("4A").as_deref(), Some("J"));
    }
}
