//! Content keys for the top-level blocks of a document.
//!
//! Live reload has to put the reader back where they were after the file
//! changes (plan section 9). It does that by content, not by position: every
//! top-level block carries a key, the hash of its source text, and the view
//! looks for the block the reader was on among the new document's keys.
//!
//! The key is the hash of the *source*, not of the rendered HTML, because two
//! identical headings render with different ids (`intro`, `intro-1`) and two
//! identical paragraphs with a footnote render with different reference ids.
//! Hashing the HTML would make such copies look unique, and the view could
//! then land on the wrong one.
//!
//! A key travels as an empty marker element written just before its block:
//!
//! ```html
//! <ezmark-block key="9f3a…"></ezmark-block>
//! <p>The block.</p>
//! ```
//!
//! Markers go through the sanitizer like everything else. The view reads
//! them, attaches each key to the top-level element that follows, and removes
//! them before the page sees the document. A document can write a marker of
//! its own; all that does is change where its own reader lands after a reload.

use comrak::Arena;
use comrak::nodes::{Ast, AstNode, LineColumn, NodeHtmlBlock, NodeValue};

use crate::frontmatter;

/// The marker element's name.
pub const MARKER_TAG: &str = "ezmark-block";

/// The marker attribute that holds the key.
pub const MARKER_ATTRIBUTE: &str = "key";

/// Put a marker before every top-level block of `root`, and replace front
/// matter, which the parser renders as nothing, with its collapsed block.
pub fn mark<'a>(arena: &'a Arena<'a>, root: &'a AstNode<'a>, source: &str) {
    let lines = LineIndex::new(source);
    // Collected first: inserting while iterating would visit the new nodes.
    let blocks: Vec<&AstNode<'a>> = root.children().collect();
    for block in blocks {
        let (start, end, front_matter) = {
            let ast = block.data();
            let front_matter = match &ast.value {
                // Footnote definitions are gathered into one section at the
                // end, wherever they were written, so no block follows here.
                NodeValue::FootnoteDefinition(_) => continue,
                NodeValue::FrontMatter(raw) => Some(frontmatter::html(raw)),
                _ => None,
            };
            (
                first_line(block, &ast),
                ast.sourcepos.end.line,
                front_matter,
            )
        };
        let Some(text) = lines.slice(source, start, end) else {
            continue;
        };
        let mut literal = format!(
            "<{MARKER_TAG} {MARKER_ATTRIBUTE}=\"{:016x}\"></{MARKER_TAG}>\n",
            content_hash(text.trim_end().as_bytes())
        );
        match front_matter {
            Some(html) if html.is_empty() => continue,
            Some(html) => literal.push_str(&html),
            None => {}
        }
        block.insert_before(html_block(arena, literal));
    }
}

/// The line a block's own text starts on.
///
/// Link reference definitions written directly above a paragraph are parsed
/// as part of it and then taken out of the tree, but the paragraph's source
/// range still covers them. They are not part of what the block shows, and
/// keying on them would give two identical paragraphs different keys, which
/// trade places if the definition is moved from above one to above the other:
/// the reader would be sent to the other copy.
///
/// The parser (comrak, written against 0.59 and still passing the tests below
/// on 1.2) does not say where the text starts: it numbers
/// the lines of a paragraph's inline nodes from the paragraph's first line,
/// definitions included, so they come out too early by however many lines
/// the definitions took. What those numbers do give is how many lines the
/// text spans, and the paragraph's last line is right, so the first line of
/// the text is counted back from there. Should the parser start numbering
/// inlines correctly, the span stays the same and this gives the wrong
/// answer; the test for this function is what would say so.
fn first_line<'a>(block: &'a AstNode<'a>, ast: &Ast) -> usize {
    let (start, end) = (ast.sourcepos.start.line, ast.sourcepos.end.line);
    if !matches!(ast.value, NodeValue::Paragraph) {
        return start;
    }
    let last_inline_line = block
        .descendants()
        .skip(1)
        .map(|inline| inline.data().sourcepos.end.line)
        .max();
    match last_inline_line {
        // `last - start + 1` lines of text, ending on line `end`.
        Some(last) if (start..=end).contains(&last) => end - (last - start),
        _ => start,
    }
}

fn html_block<'a>(arena: &'a Arena<'a>, literal: String) -> &'a AstNode<'a> {
    let mut block = NodeHtmlBlock::new(literal);
    block.block_type = 6;
    let value = NodeValue::HtmlBlock(block);
    arena.alloc(Ast::new(value, LineColumn { line: 0, column: 0 }).into())
}

/// True if `value` is a key this module could have written.
pub(crate) fn is_key(value: &str) -> bool {
    value.len() == 16
        && value
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Where each line of the source starts, for turning the parser's line
/// numbers back into text.
struct LineIndex {
    starts: Vec<usize>,
}

impl LineIndex {
    /// Lines end at `\n`, `\r\n`, or a lone `\r`, as they do for the parser.
    fn new(source: &str) -> Self {
        let bytes = source.as_bytes();
        let mut starts = vec![0];
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'\r' if bytes.get(i + 1) == Some(&b'\n') => {}
                b'\r' | b'\n' => starts.push(i + 1),
                _ => {}
            }
            i += 1;
        }
        Self { starts }
    }

    /// The text of lines `first..=last`, numbered from 1.
    fn slice<'s>(&self, source: &'s str, first: usize, last: usize) -> Option<&'s str> {
        if first == 0 || last < first {
            return None;
        }
        let start = *self.starts.get(first - 1)?;
        let end = self.starts.get(last).copied().unwrap_or(source.len());
        source.get(start..end)
    }
}

/// FNV-1a, 64 bits. Written out so that a key is the same on every platform
/// and toolchain; the standard library's hasher makes no such promise.
pub fn content_hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The keys of a document's markers, in order.
    fn keys(source: &str) -> Vec<String> {
        let arena = Arena::new();
        let mut options = comrak::Options::default();
        options.extension.footnotes = true;
        options.extension.front_matter_delimiter = Some("---".to_owned());
        options.render.r#unsafe = true;
        let root = comrak::parse_document(&arena, source, &options);
        mark(&arena, root, source);
        let mut html = String::new();
        comrak::format_html(root, &options, &mut html).unwrap();
        html.split("<ezmark-block key=\"")
            .skip(1)
            .map(|rest| rest[..16].to_owned())
            .collect()
    }

    #[test]
    fn the_hash_matches_fnv1a_test_vectors() {
        assert_eq!(content_hash(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(content_hash(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(content_hash(b"foobar"), 0x8594_4171_f739_67e8);
    }

    #[test]
    fn every_top_level_block_gets_a_key_and_nested_blocks_do_not() {
        let keys = keys(
            "# Title\n\nA paragraph.\n\n- one\n- two\n\n> quote\n>\n> more\n\n```\ncode\n```\n",
        );
        assert_eq!(keys.len(), 5, "{keys:?}");
        assert!(keys.iter().all(|key| is_key(key)), "{keys:?}");
    }

    #[test]
    fn identical_source_gives_identical_keys() {
        let keys = keys("# Same\n\nSame text.\n\nOther text.\n\nSame text.\n\n# Same\n");
        assert_eq!(keys[0], keys[4], "headings, whatever ids they render with");
        assert_eq!(keys[1], keys[3]);
        assert_ne!(keys[1], keys[2]);
        assert_ne!(keys[0], keys[1]);
    }

    #[test]
    fn a_key_depends_on_the_block_alone() {
        let before = keys("First.\n\nSecond.\n");
        let after = keys("Inserted.\n\nFirst.\n\n\n\nSecond.   \n");
        assert_eq!(before[0], after[1]);
        assert_eq!(
            before[1], after[2],
            "surrounding blank lines and trailing spaces"
        );
        let crlf = keys("First.\r\n\r\nSecond.\r\n");
        assert_eq!(before, crlf, "line endings between blocks");
    }

    /// Found in review on 2026-11-15: the definition used to be hashed into
    /// the paragraph below it, so moving it swapped the keys of two
    /// identical paragraphs.
    #[test]
    fn a_link_definition_above_a_paragraph_is_not_part_of_its_key() {
        let above_first = keys("A\n\n[x]: /u\nCopy [x].\n\nMiddle\n\nCopy [x].\n\nC\n");
        let above_second = keys("A\n\nCopy [x].\n\nMiddle\n\n[x]: /u\nCopy [x].\n\nC\n");
        assert_eq!(above_first.len(), 5, "{above_first:?}");
        assert_eq!(above_first[1], above_first[3], "identical paragraphs");
        assert_eq!(above_first, above_second, "wherever the definition sits");

        // Several definitions, one with a title on its own line.
        let several = keys("[a]: /a\n[b]: /b\n  \"title\"\nCopy [x].\n\n[x]: /u\n");
        assert_eq!(several, [above_first[1].clone()]);
    }

    /// Found on 2026-11-24 when comrak 1.x changed the pipeline snapshots:
    /// under 0.59 a two-line paragraph was keyed on its last line only, and
    /// a setext heading below a definition had the definition in its key.
    #[test]
    fn a_key_covers_all_of_a_blocks_own_lines_and_nothing_else() {
        let paragraph = "First line\nsecond line.\n";
        assert_eq!(
            keys(paragraph),
            [format!(
                "{:016x}",
                content_hash(paragraph.trim_end().as_bytes())
            )]
        );
        assert_eq!(keys(&format!("[x]: /u\n{paragraph}")), keys(paragraph));

        let heading = "Title\n===\n";
        assert_eq!(keys(&format!("[x]: /u\n{heading}")), keys(heading));
    }

    #[test]
    fn footnote_definitions_get_no_marker() {
        let keys = keys("Text[^a].\n\n[^a]: The note.\n\nAfter.\n");
        assert_eq!(keys.len(), 2, "{keys:?}");
    }

    #[test]
    fn front_matter_becomes_a_keyed_block() {
        let source = "---\ntitle: T\n---\n\nBody.\n";
        assert_eq!(keys(source).len(), 2);
    }

    #[test]
    fn line_slices_follow_the_parsers_idea_of_a_line() {
        let source = "a\nb\r\nc\rd";
        let lines = LineIndex::new(source);
        assert_eq!(lines.slice(source, 1, 1), Some("a\n"));
        assert_eq!(lines.slice(source, 2, 3), Some("b\r\nc\r"));
        assert_eq!(lines.slice(source, 4, 4), Some("d"));
        assert_eq!(lines.slice(source, 4, 9), Some("d"));
        assert_eq!(lines.slice(source, 5, 5), None);
        assert_eq!(lines.slice(source, 0, 1), None);
        assert_eq!(lines.slice(source, 3, 2), None);
    }
}
