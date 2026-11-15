//! Front matter, shown as a collapsed block instead of being dropped.
//!
//! The parser recognises a leading `---` block and keeps it out of the
//! document. Raw YAML at the top of a page looks broken, and hiding it
//! entirely hides things a reader may want (a title, a date, tags), so it
//! becomes a closed `<details>` holding a table of the top-level keys.
//!
//! This is not a YAML parser, on purpose: no dependency, and nothing here
//! interprets the values. It only splits `key: value` lines at the top level.
//! Whatever is nested under a key is shown as written, and a block that does
//! not look like top-level keys at all is shown whole, as text.

use std::fmt::Write;

use crate::highlight::write_escaped;

/// The class on the `<details>` element, for the stylesheet.
pub const CLASS: &str = "front-matter";

/// HTML for a front matter block as the parser hands it over, delimiters
/// included. Empty if there is nothing between the delimiters.
pub fn html(raw: &str) -> String {
    let body = body(raw);
    if body.trim().is_empty() {
        return String::new();
    }
    let mut out = format!("<details class=\"{CLASS}\"><summary>Front matter</summary>");
    // Writing to a String cannot fail.
    let _ = match entries(body) {
        Some(entries) => write_table(&mut out, &entries),
        None => write_pre(&mut out, body.trim_end()),
    };
    out.push_str("</details>\n");
    out
}

/// What lies between the opening and the closing delimiter line.
fn body(raw: &str) -> &str {
    let raw = raw.trim_end();
    let after_open = raw.split_once('\n').map_or("", |(_, rest)| rest);
    match after_open.rfind('\n') {
        Some(last_line) => &after_open[..last_line + 1],
        // `---\n---`: the only line left is the closing delimiter.
        None => "",
    }
}

/// The top-level `key: value` pairs, or `None` if the block is not made of them.
fn entries(body: &str) -> Option<Vec<(&str, String)>> {
    let mut entries: Vec<(&str, String)> = Vec::new();
    for line in body.lines() {
        let line = line.trim_end();
        let nested = line.starts_with([' ', '\t']) || line == "-" || line.starts_with("- ");
        if nested || line.is_empty() {
            // Part of the value above it: a list, a mapping, a block of text.
            let (_, value) = entries.last_mut()?;
            if !(line.is_empty() && value.is_empty()) {
                value.push('\n');
                value.push_str(line);
            }
        } else if !line.starts_with('#') {
            let (key, value) = split_key(line)?;
            // `- x` after a key is not a value YAML allows; show the block whole.
            if value.starts_with('-') && value.parse::<f64>().is_err() {
                return None;
            }
            entries.push((key, scalar(value).to_owned()));
        }
    }
    for (_, value) in &mut entries {
        *value = value.trim_matches('\n').to_owned();
    }
    (!entries.is_empty()).then_some(entries)
}

/// `key: value` or `key:`. The colon must end the line or be followed by a
/// space, so `http://x` on its own line is not a key.
fn split_key(line: &str) -> Option<(&str, &str)> {
    let (key, value) = match line.split_once(": ") {
        Some(pair) => pair,
        None => (line.strip_suffix(':')?, ""),
    };
    let key = unquoted(key.trim());
    (!key.is_empty()).then_some((key, value.trim()))
}

/// A value as a reader would write it: without the quotes around a string,
/// and without the marker that only says "a block of text follows".
fn scalar(value: &str) -> &str {
    if matches!(value, "|" | ">" | "|-" | ">-" | "|+" | ">+") {
        return "";
    }
    unquoted(value)
}

fn unquoted(text: &str) -> &str {
    for quote in ['"', '\''] {
        if let Some(inner) = text
            .strip_prefix(quote)
            .and_then(|rest| rest.strip_suffix(quote))
            && !inner.contains(quote)
        {
            return inner;
        }
    }
    text
}

fn write_table(out: &mut String, entries: &[(&str, String)]) -> std::fmt::Result {
    out.push_str("<table><tbody>");
    for (key, value) in entries {
        out.push_str("<tr><th scope=\"row\">");
        write_escaped(out, key)?;
        out.push_str("</th><td>");
        // Lines written under the key keep their layout, even a single one.
        // A value on the key's own line was trimmed, so it never starts so.
        let nested = value.contains('\n') || value.starts_with([' ', '\t', '-']);
        if nested {
            write_pre(out, value)?;
        } else {
            write_escaped(out, value)?;
        }
        out.push_str("</td></tr>");
    }
    out.push_str("</tbody></table>");
    Ok(())
}

fn write_pre(out: &mut String, text: &str) -> std::fmt::Result {
    out.push_str("<pre>");
    write_escaped(out, text)?;
    out.write_str("</pre>")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn top_level_keys_become_table_rows() {
        let html = html("---\ntitle: \"Notes & <ideas>\"\ndate: 2026-11-14\ndraft: true\n---\n\n");
        assert_eq!(
            html,
            "<details class=\"front-matter\"><summary>Front matter</summary><table><tbody>\
             <tr><th scope=\"row\">title</th><td>Notes &amp; &lt;ideas&gt;</td></tr>\
             <tr><th scope=\"row\">date</th><td>2026-11-14</td></tr>\
             <tr><th scope=\"row\">draft</th><td>true</td></tr>\
             </tbody></table></details>\n"
        );
    }

    #[test]
    fn nested_values_are_shown_as_written() {
        let html = html(
            "---\ntags:\n  - rust\n  - markdown\n# a comment\nsummary: |\n  Two\n  lines\n\
             author:\n  name: A\nurl: https://example.com/a:b\n---\n",
        );
        assert!(
            html.contains("<th scope=\"row\">tags</th><td><pre>  - rust\n  - markdown</pre></td>"),
            "{html}"
        );
        assert!(
            html.contains("<th scope=\"row\">summary</th><td><pre>  Two\n  lines</pre></td>"),
            "{html}"
        );
        assert!(html.contains("<td><pre>  name: A</pre></td>"), "{html}");
        assert!(
            html.contains("<th scope=\"row\">url</th><td>https://example.com/a:b</td>"),
            "{html}"
        );
        assert!(!html.contains("comment"), "{html}");
    }

    #[test]
    fn a_block_that_is_not_top_level_keys_is_shown_whole() {
        let html = html("---\njust some <text>\nover two lines\n---\n");
        assert_eq!(
            html,
            "<details class=\"front-matter\"><summary>Front matter</summary>\
             <pre>just some &lt;text&gt;\nover two lines</pre></details>\n"
        );
        assert!(super::html("---\n- a\n- b\n---\n").contains("<pre>- a\n- b</pre>"));
    }

    #[test]
    fn an_empty_block_shows_nothing() {
        assert_eq!(html("---\n---\n"), "");
        assert_eq!(html("---\n\n---\n"), "");
        assert_eq!(html("---\r\n---\r\n"), "");
    }

    #[test]
    fn windows_line_endings_are_handled() {
        let html = html("---\r\ntitle: A\r\nlist:\r\n  - x\r\n---\r\n");
        assert!(
            html.contains("<th scope=\"row\">title</th><td>A</td>"),
            "{html}"
        );
        assert!(html.contains("<pre>  - x</pre>"), "{html}");
        assert!(!html.contains('\r'), "{html:?}");
    }
}
