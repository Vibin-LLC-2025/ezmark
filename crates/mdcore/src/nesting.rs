//! A cheap guard against absurdly deep nesting.
//!
//! Opening a block element makes an HTML parser walk its stack of open
//! elements, so a document that nests N elements costs on the order of N
//! squared. Measured on 2026-11-13 through the full pipeline: 20,000 unclosed
//! `<div>` tags (120 KB) took 2.8 s, and 20,000 nested `>` quotes on one line
//! (20 KB) took 2.9 s, both doubling roughly fourfold with each doubling of
//! depth. At a depth of 500 the same inputs take under 5 ms.
//!
//! No real document nests hundreds deep (comrak itself caps list nesting at
//! 100), so anything deeper than [`MAX_DEPTH`] is shown as plain text instead
//! of being parsed.
//!
//! This is a best-effort guard, not a proof. It is tuned never to reject an
//! ordinary document, so it follows HTML's tag rules only approximately, and
//! input crafted against those approximations can still be slow. The backstop
//! is that rendering runs off the UI thread, the window says when a render is
//! taking long, and opening another file supersedes it.

/// The deepest nesting that is still rendered.
pub const MAX_DEPTH: usize = 512;

/// True if some line opens more than `limit` block quotes.
///
/// Each level of quote nesting needs its own `>` at the start of every line
/// that continues it, so counting the `>` characters in a line's leading run
/// of container markers bounds the depth.
pub fn quote_depth_exceeds(source: &str, limit: usize) -> bool {
    source.split(['\n', '\r']).any(|line| {
        line.bytes()
            .take_while(|byte| {
                matches!(
                    byte,
                    b' ' | b'\t' | b'>' | b'-' | b'+' | b'*' | b'0'..=b'9' | b'.' | b')'
                )
            })
            .filter(|byte| *byte == b'>')
            .count()
            > limit
    })
}

/// Elements that never have content, so never nest.
const VOID: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

/// Elements whose end tag may be left out, with the containers a search for
/// the one being implicitly closed must not cross.
const IMPLICITLY_CLOSED: &[(&str, &[&str])] = &[
    ("li", &["ul", "ol", "menu"]),
    ("dt", &["dl"]),
    ("dd", &["dl"]),
    ("tr", &["table", "thead", "tbody", "tfoot"]),
    ("td", &["table", "tr"]),
    ("th", &["table", "tr"]),
    ("option", &["select"]),
];

/// True if the tags in `html` nest more than `limit` deep.
///
/// Tracks a stack of open element names the way a browser roughly would: an
/// end tag closes the nearest open element of that name and everything above
/// it, a new `<li>` (or `<td>`, `<tr>`, ...) closes the previous one in the
/// same list, a `<p>` closes a `<p>` left open just before it, and void and
/// self-closed elements never open.
pub fn html_depth_exceeds(html: &str, limit: usize) -> bool {
    let mut open: Vec<&str> = Vec::new();
    let mut rest = html;
    while let Some(at) = rest.find('<') {
        rest = &rest[at + 1..];
        let (closing, tag) = match rest.strip_prefix('/') {
            Some(tag) => (true, tag),
            None => (false, rest),
        };
        if !tag.starts_with(|c: char| c.is_ascii_alphabetic()) {
            continue;
        }
        let name_len = tag
            .find(|c: char| c.is_ascii_whitespace() || c == '/' || c == '>')
            .unwrap_or(tag.len());
        let name = &tag[..name_len];
        let tag_end = tag.find('>').unwrap_or(tag.len());
        let self_closed = tag[..tag_end].ends_with('/');
        rest = &tag[tag_end..];

        if closing {
            close(&mut open, name, &[]);
        } else if !self_closed && !is_any(name, VOID) {
            if name.eq_ignore_ascii_case("p") {
                if open.last().is_some_and(|top| top.eq_ignore_ascii_case("p")) {
                    open.pop();
                }
            } else if let Some((_, containers)) = IMPLICITLY_CLOSED
                .iter()
                .find(|(implicit, _)| name.eq_ignore_ascii_case(implicit))
            {
                close(&mut open, name, containers);
            }
            open.push(name);
            if open.len() > limit {
                return true;
            }
        }
    }
    false
}

/// Close the nearest open `name` and everything above it, unless one of
/// `barriers` is open above it.
fn close(open: &mut Vec<&str>, name: &str, barriers: &[&str]) {
    for index in (0..open.len()).rev() {
        if open[index].eq_ignore_ascii_case(name) {
            open.truncate(index);
            return;
        }
        if is_any(open[index], barriers) {
            return;
        }
    }
}

fn is_any(name: &str, names: &[&str]) -> bool {
    names.iter().any(|known| name.eq_ignore_ascii_case(known))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_quote_markers_per_line() {
        assert!(quote_depth_exceeds(&">".repeat(11), 10));
        assert!(quote_depth_exceeds(&"> ".repeat(11), 10));
        assert!(quote_depth_exceeds(
            &format!("ok\r{}", " > -".repeat(11)),
            10
        ));
        assert!(!quote_depth_exceeds(&">".repeat(10), 10));
        // Depth is per line: many shallow quotes are not deep.
        assert!(!quote_depth_exceeds(&"> quote\n".repeat(1000), 10));
        // Only the leading run counts.
        assert!(!quote_depth_exceeds(&format!("a {}", ">".repeat(100)), 10));
    }

    #[test]
    fn deep_html_is_detected() {
        assert!(html_depth_exceeds(&"<div>\n".repeat(11), 10));
        assert!(html_depth_exceeds(&"<blockquote>\n<p>".repeat(6), 10));
        assert!(html_depth_exceeds(&"<DIV class='x'>".repeat(11), 10));
        assert!(html_depth_exceeds(&"<ul><li>".repeat(6), 10));
        assert!(!html_depth_exceeds(&"<div>\n".repeat(10), 10));
    }

    #[test]
    fn ordinary_documents_are_never_deep() {
        for html in [
            // Many siblings.
            "<div><p>a</p></div>\n".repeat(1000),
            // Void and self-closed elements.
            "<p>a<br>b<img src=x><hr><input disabled=\"\" /></p>".repeat(1000),
            "<svg><path d=\"M0 0\"/><circle r=\"1\"/></svg>".repeat(1000),
            // End tags left out, as hand-written HTML often does.
            format!("<ul>{}</ul>", "<li>item\n".repeat(1000)),
            "<p>one\n<p>two\n".repeat(1000),
            format!("<table>{}</table>", "<tr><td>a<td>b\n".repeat(1000)),
            format!("<dl>{}</dl>", "<dt>t<dd>d\n".repeat(1000)),
            // Text that only looks like a tag, closed by its paragraph.
            "<p>a Vec<T> and a Map<K> here</p>\n".repeat(1000),
            // Not tags at all.
            "a < b, c <3, <!-- note --> <?x?> </> 1<2".repeat(1000),
        ] {
            assert!(!html_depth_exceeds(&html, 10), "{}", &html[..60]);
        }
    }

    #[test]
    fn nested_lists_still_count() {
        // A new <li> closes the previous <li> of its own list only.
        let nested = "<ul><li>a<li>b".repeat(6);
        assert!(html_depth_exceeds(&nested, 10));
    }
}
