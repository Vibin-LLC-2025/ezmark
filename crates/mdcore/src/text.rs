//! Turning a file's bytes into text.
//!
//! Markdown is UTF-8 almost everywhere, but Windows tools still write UTF-16
//! (Windows PowerShell does it for every `>` redirect). A UTF-16 file read as
//! UTF-8 is unreadable, so both byte orders are detected and decoded here.

use std::borrow::Cow;

/// A file's text, and whether any of it could not be decoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded<'a> {
    /// The text, without a byte-order mark.
    pub text: Cow<'a, str>,
    /// True if some bytes were not valid in the detected encoding and were
    /// replaced with U+FFFD.
    pub lossy: bool,
}

/// Decode `bytes` as UTF-8, or as UTF-16 if it starts with a UTF-16
/// byte-order mark or plainly looks like UTF-16 without one.
pub fn decode(bytes: &[u8]) -> Decoded<'_> {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return utf8(rest);
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return utf16(rest, u16::from_le_bytes);
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        return utf16(rest, u16::from_be_bytes);
    }
    match guess_utf16_without_mark(bytes) {
        Some(to_unit) => utf16(bytes, to_unit),
        None => utf8(bytes),
    }
}

fn utf8(bytes: &[u8]) -> Decoded<'_> {
    let text = String::from_utf8_lossy(bytes);
    let lossy = matches!(text, Cow::Owned(_));
    Decoded { text, lossy }
}

fn utf16(bytes: &[u8], to_unit: fn([u8; 2]) -> u16) -> Decoded<'static> {
    let (pairs, odd) = bytes.as_chunks::<2>();
    let mut lossy = !odd.is_empty();
    let units = pairs.iter().map(|pair| to_unit(*pair));
    let text = char::decode_utf16(units)
        .map(|unit| {
            unit.unwrap_or_else(|_| {
                lossy = true;
                char::REPLACEMENT_CHARACTER
            })
        })
        .collect::<String>();
    Decoded {
        text: Cow::Owned(text),
        lossy,
    }
}

/// UTF-16 text that is mostly ASCII has a zero in every other byte. If at
/// least 40% of the first 512 bytes are zero and nearly all of those sit on
/// the same side of each pair, that side names the byte order. Real UTF-8
/// text contains no zero bytes at all, so this cannot misread it.
fn guess_utf16_without_mark(bytes: &[u8]) -> Option<fn([u8; 2]) -> u16> {
    let sample = &bytes[..bytes.len().min(512) & !1];
    if sample.len() < 4 {
        return None;
    }
    let zeros_at = |offset: usize| {
        sample
            .iter()
            .skip(offset)
            .step_by(2)
            .filter(|byte| **byte == 0)
            .count()
    };
    let (even, odd) = (zeros_at(0), zeros_at(1));
    if (even + odd) * 5 < sample.len() * 2 {
        return None;
    }
    if odd >= even * 10 {
        Some(u16::from_le_bytes)
    } else if even >= odd * 10 {
        Some(u16::from_be_bytes)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utf16_bytes(text: &str, to_bytes: fn(u16) -> [u8; 2]) -> Vec<u8> {
        text.encode_utf16().flat_map(to_bytes).collect()
    }

    #[test]
    fn utf8_with_and_without_a_mark() {
        let plain = decode("# Héllo ✓".as_bytes());
        assert_eq!((plain.text.as_ref(), plain.lossy), ("# Héllo ✓", false));
        assert!(
            matches!(plain.text, Cow::Borrowed(_)),
            "no copy for valid UTF-8"
        );

        let marked = decode(b"\xEF\xBB\xBF# Title");
        assert_eq!((marked.text.as_ref(), marked.lossy), ("# Title", false));

        let empty = decode(b"");
        assert_eq!((empty.text.as_ref(), empty.lossy), ("", false));
    }

    #[test]
    fn invalid_utf8_is_replaced_and_reported() {
        let decoded = decode(b"caf\xE9 au lait");
        assert_eq!(decoded.text, "caf\u{FFFD} au lait");
        assert!(decoded.lossy);
    }

    #[test]
    fn utf16_with_a_mark_in_either_byte_order() {
        let text = "# Tïtle ✓ 😀\r\nbody";
        let mut le = vec![0xFF, 0xFE];
        le.extend(utf16_bytes(text, u16::to_le_bytes));
        let mut be = vec![0xFE, 0xFF];
        be.extend(utf16_bytes(text, u16::to_be_bytes));
        for bytes in [le, be] {
            let decoded = decode(&bytes);
            assert_eq!((decoded.text.as_ref(), decoded.lossy), (text, false));
        }
    }

    #[test]
    fn utf16_without_a_mark_is_recognised_when_mostly_ascii() {
        let text = "# Notes\n\nPlain ASCII text, as a script would write it.\n";
        for to_bytes in [u16::to_le_bytes, u16::to_be_bytes] {
            let bytes = utf16_bytes(text, to_bytes);
            let decoded = decode(&bytes);
            assert_eq!((decoded.text.as_ref(), decoded.lossy), (text, false));
        }
    }

    #[test]
    fn broken_utf16_is_replaced_and_reported() {
        // A lone high surrogate, then a trailing odd byte.
        let decoded = decode(&[0xFF, 0xFE, b'a', 0, 0x00, 0xD8, b'b', 0, b'c']);
        assert_eq!(decoded.text, "a\u{FFFD}b");
        assert!(decoded.lossy);
    }

    #[test]
    fn binary_noise_is_not_mistaken_for_utf16() {
        let noise: Vec<u8> = (0..=255).collect();
        let decoded = decode(&noise);
        assert!(decoded.lossy, "decoded as lossy UTF-8, not as UTF-16");
        assert!(decoded.text.starts_with('\0'));
    }
}
