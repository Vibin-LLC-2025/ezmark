//! Syntax highlighting for fenced code blocks.
//!
//! syntect runs here, in Rust, before the HTML reaches the sanitizer, so the
//! view layer has no highlighter and nothing reflows after the first paint.
//! It emits classes, never inline styles: the page's Content Security Policy
//! allows styles only from the bundled stylesheet, and `ui/highlight.css`
//! maps these classes to colours for the light and the dark theme.
//!
//! The output is deliberately small. syntect's stock HTML writer nests one
//! span per grammar scope and names every part of the scope in the class,
//! which measured at 27 bytes of HTML for each byte of code. Here each run of
//! text gets at most one flat span with one class from the fixed list in
//! [`CLASSES`], chosen by the innermost scope that has a colour:
//!
//! ```html
//! <span class="hl-keyword">let</span> x = <span class="hl-string">"a"</span>;
//! ```
//!
//! The sanitizer lets a class through on a `<span>` only if it is on that
//! list, so a document gains nothing by writing such spans itself.
//!
//! Highlighting is bounded. Its cost grows with the amount of code, so one
//! document gets a budget of [`DOCUMENT_BUDGET`] bytes of code; blocks beyond
//! it, and any block with a line longer than [`LINE_LIMIT`], are shown
//! as plain code.

use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

use comrak::adapters::SyntaxHighlighterAdapter;
use syntect::parsing::{ParseState, Scope, ScopeStack, SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;

/// Every class the highlighter can emit. `ui/highlight.css` styles exactly
/// these; a test in this file fails if the two lists drift apart.
pub const CLASSES: &[&str] = &[
    "hl-comment",
    "hl-keyword",
    "hl-string",
    "hl-constant",
    "hl-entity",
    "hl-tag",
    "hl-variable",
    "hl-heading",
    "hl-bold",
    "hl-italic",
    "hl-inserted",
    "hl-deleted",
    "hl-changed",
    "hl-invalid",
];

/// Which grammar scopes get which class. A piece of text takes the class of
/// its innermost scope that appears here, and within one scope the first
/// matching line wins, so the more specific prefixes come first. The grouping
/// follows GitHub's own highlighting, so colours land where a reader expects.
const RULES: &[(&str, &str)] = &[
    ("comment", "hl-comment"),
    ("punctuation.definition.comment", "hl-comment"),
    ("string", "hl-string"),
    ("punctuation.definition.string", "hl-string"),
    ("constant", "hl-constant"),
    ("variable.language", "hl-constant"),
    ("support.constant", "hl-constant"),
    ("support.type", "hl-constant"),
    ("support.class", "hl-constant"),
    ("meta.property-name", "hl-constant"),
    ("markup.raw", "hl-constant"),
    ("keyword", "hl-keyword"),
    ("storage", "hl-keyword"),
    ("entity.name.tag", "hl-tag"),
    ("markup.quote", "hl-tag"),
    ("entity.name.section", "hl-heading"),
    ("entity", "hl-entity"),
    ("support.function", "hl-entity"),
    ("variable.function", "hl-entity"),
    ("meta.diff.range", "hl-entity"),
    ("variable.other.readwrite", "hl-variable"),
    ("variable.other.member", "hl-variable"),
    ("punctuation.definition.list_item", "hl-variable"),
    ("markup.heading", "hl-heading"),
    ("meta.diff.header", "hl-heading"),
    ("markup.bold", "hl-bold"),
    ("markup.italic", "hl-italic"),
    ("markup.inserted", "hl-inserted"),
    ("markup.deleted", "hl-deleted"),
    ("markup.changed", "hl-changed"),
    ("invalid", "hl-invalid"),
];

/// How many bytes of code one document may have highlighted. Measured on
/// 2026-11-14 (docs/measurements.md), so that the worst case stays well under
/// a second.
pub const DOCUMENT_BUDGET: usize = 256 * 1024;

/// A block with a line longer than this is not highlighted. Minified code on
/// one line is unreadable either way, and it is the worst case for the
/// grammar's regular expressions.
pub const LINE_LIMIT: usize = 4096;

/// Names people write after the fence that the grammar set knows under
/// another name. TypeScript and JSX have no grammar of their own here and are
/// read as JavaScript, which gets comments, strings, and keywords right.
const ALIASES: &[(&str, &str)] = &[
    ("c#", "cs"),
    ("c++", "cpp"),
    ("cjs", "js"),
    ("csharp", "cs"),
    ("golang", "go"),
    ("json5", "json"),
    ("jsonc", "json"),
    ("jsx", "js"),
    ("latex", "tex"),
    ("make", "makefile"),
    ("mjs", "js"),
    ("node", "js"),
    ("objc", "m"),
    ("objective-c", "m"),
    ("py3", "py"),
    ("python3", "py"),
    ("shell", "sh"),
    ("svelte", "html"),
    ("ts", "js"),
    ("tsx", "js"),
    ("typescript", "js"),
    ("vue", "html"),
    ("zsh", "sh"),
];

fn syntaxes() -> &'static SyntaxSet {
    static SET: OnceLock<SyntaxSet> = OnceLock::new();
    SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

/// The grammar for the first word of a fence's info string, if there is one.
fn syntax_for(language: &str) -> Option<&'static SyntaxReference> {
    let lower = language.to_ascii_lowercase();
    let token = ALIASES
        .iter()
        .find(|(alias, _)| *alias == lower)
        .map_or(lower.as_str(), |(_, target)| target);
    let syntax = syntaxes().find_syntax_by_token(token)?;
    // "Plain Text" matches `txt`; there is nothing in it to colour.
    (syntax.name != "Plain Text").then_some(syntax)
}

/// The class for text inside `stack`: that of the innermost scope with a rule.
fn class_of(stack: &ScopeStack) -> Option<&'static str> {
    static RULE_SCOPES: OnceLock<Vec<(Scope, &'static str)>> = OnceLock::new();
    let rules = RULE_SCOPES.get_or_init(|| {
        RULES
            .iter()
            .map(|(prefix, class)| (Scope::new(prefix).expect("a valid scope"), *class))
            .collect()
    });
    stack.as_slice().iter().rev().find_map(|scope| {
        rules
            .iter()
            .find(|(prefix, _)| prefix.is_prefix_of(*scope))
            .map(|(_, class)| *class)
    })
}

/// Highlights the code blocks of one document.
#[derive(Debug)]
pub struct Highlighter {
    remaining: AtomicUsize,
}

impl Highlighter {
    /// A highlighter with a fresh [`DOCUMENT_BUDGET`].
    pub fn new() -> Self {
        Self::with_budget(DOCUMENT_BUDGET)
    }

    /// A highlighter that will colour at most `bytes` bytes of code.
    pub fn with_budget(bytes: usize) -> Self {
        Self {
            remaining: AtomicUsize::new(bytes),
        }
    }

    /// Take `bytes` from the budget, or leave it untouched if it does not fit.
    fn spend(&self, bytes: usize) -> bool {
        self.remaining
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |left| {
                left.checked_sub(bytes)
            })
            .is_ok()
    }

    /// `code` as classed spans, or `None` if it should be shown plain.
    fn highlight(&self, language: &str, code: &str) -> Option<String> {
        let syntax = syntax_for(language)?;
        if code.lines().any(|line| line.len() > LINE_LIMIT) || !self.spend(code.len()) {
            return None;
        }
        let mut state = ParseState::new(syntax);
        let mut stack = ScopeStack::new();
        let mut spans = Spans::new(code.len());
        for line in LinesWithEndings::from(code) {
            // A grammar that fails on some input costs the colours, not the block.
            let ops = state.parse_line(line, syntaxes()).ok()?;
            let mut done = 0;
            for (at, op) in &ops {
                spans.push(line.get(done..*at)?, class_of(&stack));
                stack.apply(op).ok()?;
                done = *at;
            }
            spans.push(line.get(done..)?, class_of(&stack));
        }
        Some(spans.finish())
    }
}

impl Default for Highlighter {
    fn default() -> Self {
        Self::new()
    }
}

/// Escaped code with flat, never nested, spans. Neighbouring pieces with the
/// same class share one span.
struct Spans {
    html: String,
    open: Option<&'static str>,
}

impl Spans {
    fn new(code_len: usize) -> Self {
        Self {
            html: String::with_capacity(code_len * 2),
            open: None,
        }
    }

    fn push(&mut self, text: &str, class: Option<&'static str>) {
        if text.is_empty() {
            return;
        }
        // Whitespace has no colour, so it never opens or closes a span.
        if class != self.open && !text.trim().is_empty() {
            self.close();
            if let Some(class) = class {
                self.html.push_str("<span class=\"");
                self.html.push_str(class);
                self.html.push_str("\">");
            }
            self.open = class;
        }
        // Writing to a String cannot fail.
        let _ = write_escaped(&mut self.html, text);
    }

    fn close(&mut self) {
        if self.open.take().is_some() {
            self.html.push_str("</span>");
        }
    }

    fn finish(mut self) -> String {
        self.close();
        self.html
    }
}

impl SyntaxHighlighterAdapter for Highlighter {
    fn write_highlighted(
        &self,
        output: &mut dyn fmt::Write,
        lang: Option<&str>,
        code: &str,
    ) -> fmt::Result {
        match lang.and_then(|lang| self.highlight(lang, code)) {
            Some(html) => output.write_str(&html),
            None => write_escaped(output, code),
        }
    }

    fn write_pre_tag(
        &self,
        output: &mut dyn fmt::Write,
        attributes: HashMap<&'static str, Cow<'_, str>>,
    ) -> fmt::Result {
        write_tag(output, "pre", attributes)
    }

    fn write_code_tag(
        &self,
        output: &mut dyn fmt::Write,
        attributes: HashMap<&'static str, Cow<'_, str>>,
    ) -> fmt::Result {
        write_tag(output, "code", attributes)
    }
}

/// An opening tag, with its attributes in a fixed order so that the same
/// document always renders to the same bytes.
fn write_tag(
    output: &mut dyn fmt::Write,
    tag: &str,
    attributes: HashMap<&'static str, Cow<'_, str>>,
) -> fmt::Result {
    let mut attributes: Vec<_> = attributes.into_iter().collect();
    attributes.sort();
    write!(output, "<{tag}")?;
    for (name, value) in attributes {
        write!(output, " {name}=\"")?;
        write_escaped(output, &value)?;
        output.write_char('"')?;
    }
    output.write_char('>')
}

/// Text escaped for an element's content or a double-quoted attribute.
pub(crate) fn write_escaped(output: &mut dyn fmt::Write, text: &str) -> fmt::Result {
    let mut rest = text;
    while let Some(at) = rest.find(['&', '<', '>', '"']) {
        output.write_str(&rest[..at])?;
        output.write_str(match rest.as_bytes()[at] {
            b'&' => "&amp;",
            b'<' => "&lt;",
            b'>' => "&gt;",
            _ => "&quot;",
        })?;
        rest = &rest[at + 1..];
    }
    output.write_str(rest)
}

/// True if `class` is one the highlighter could have written.
pub(crate) fn is_highlight_class(class: &str) -> bool {
    CLASSES.contains(&class)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn highlighted(highlighter: &Highlighter, language: &str, code: &str) -> String {
        let mut out = String::new();
        highlighter
            .write_highlighted(&mut out, Some(language), code)
            .unwrap();
        out
    }

    #[test]
    fn code_becomes_flat_classed_spans_and_never_inline_styles() {
        let html = highlighted(
            &Highlighter::new(),
            "rust",
            "fn main() { let x = \"<&>\"; } // done\n",
        );
        assert!(
            html.starts_with(
                "<span class=\"hl-keyword\">fn </span><span class=\"hl-entity\">main</span>"
            ),
            "{html}"
        );
        assert!(
            html.contains("<span class=\"hl-string\">&quot;&lt;&amp;&gt;&quot;</span>"),
            "code is escaped: {html}"
        );
        assert!(
            html.contains("<span class=\"hl-comment\">// done\n</span>"),
            "{html}"
        );
        assert!(!html.contains("style="), "{html}");
        // Flat: every span closes before the next one opens.
        for piece in html.split("<span").skip(1) {
            assert!(piece.contains("</span>"), "{html}");
        }
        assert_eq!(
            html.matches("<span").count(),
            html.matches("</span>").count()
        );
    }

    #[test]
    fn only_listed_classes_are_ever_emitted() {
        let samples = [
            ("rust", "#[derive(Debug)]\nstruct A<'a>(&'a str); // c\n"),
            (
                "cpp",
                "#include <a.h>\ntemplate<class T> int f() { return 1; }\n",
            ),
            ("cs", "class A { string s = $\"{1}\"; }\n"),
            (
                "html",
                "<a href=\"x\" onclick=\"y()\">t</a><style>a{}</style>\n",
            ),
            ("sh", "for f in *.md; do echo \"$f\"; done\n"),
            (
                "py",
                "@decorator\ndef f(x: int) -> str:\n    return f\"{x}\"\n",
            ),
            ("diff", "--- a\n+++ b\n@@ -1 +1 @@\n-old\n+new\n"),
            ("md", "# Title\n\n**bold** *it* `code`\n\n> quote\n"),
            ("json", "{\"a\": [1, true, null]}\n"),
        ];
        for (language, code) in samples {
            let html = highlighted(&Highlighter::new(), language, code);
            assert!(html.contains("<span"), "{language} was not highlighted");
            for attribute in html.split("class=\"").skip(1) {
                let class = attribute.split('"').next().unwrap();
                assert!(is_highlight_class(class), "{language}: {class:?}");
            }
        }
    }

    #[test]
    fn every_rule_names_a_listed_class_and_a_valid_scope() {
        for (prefix, class) in RULES {
            assert!(CLASSES.contains(class), "{class} is not in CLASSES");
            assert!(Scope::new(prefix).is_ok(), "{prefix}");
        }
        for class in CLASSES {
            assert!(
                RULES.iter().any(|(_, used)| used == class),
                "{class} is never emitted"
            );
        }
    }

    /// The stylesheet and the class list are kept in step by hand; this is
    /// what notices when they are not.
    #[test]
    fn the_stylesheet_styles_exactly_the_listed_classes() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../ui/highlight.css");
        let css = std::fs::read_to_string(path).expect("ui/highlight.css");
        for class in CLASSES {
            assert!(
                css.contains(&format!(".{class} {{")),
                "ui/highlight.css has no rule for .{class}"
            );
        }
        for rule in css.split(".hl-").skip(1) {
            let name: String = rule
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
                .collect();
            let class = format!("hl-{name}");
            assert!(
                CLASSES.contains(&class.as_str()),
                "ui/highlight.css styles .{class}, which is never emitted"
            );
        }
    }

    #[test]
    fn languages_are_found_by_the_names_people_write() {
        for language in [
            "rust",
            "rs",
            "Rust",
            "js",
            "javascript",
            "ts",
            "TypeScript",
            "tsx",
            "py",
            "python",
            "sh",
            "bash",
            "shell",
            "zsh",
            "json",
            "jsonc",
            "yaml",
            "yml",
            "html",
            "css",
            "c",
            "cpp",
            "c++",
            "cs",
            "csharp",
            "c#",
            "go",
            "golang",
            "java",
            "rb",
            "ruby",
            "sql",
            "diff",
            "md",
            "markdown",
            "xml",
            "php",
            "lua",
            "makefile",
        ] {
            assert!(syntax_for(language).is_some(), "no grammar for {language}");
        }
        for language in ["", "text", "txt", "nonsense", "mermaid", "language-\"x"] {
            assert!(syntax_for(language).is_none(), "a grammar for {language:?}");
        }
    }

    #[test]
    fn unknown_languages_and_long_lines_are_shown_plain() {
        let highlighter = Highlighter::new();
        assert_eq!(
            highlighted(&highlighter, "nonsense", "a < b\n"),
            "a &lt; b\n"
        );
        let minified = format!("var a = \"{}\";\n", "x".repeat(LINE_LIMIT));
        assert!(!highlighted(&highlighter, "js", &minified).contains("<span"));
        assert!(highlighted(&highlighter, "js", "var a = 1;\n").contains("<span"));
    }

    #[test]
    fn the_budget_is_spent_per_document_and_a_block_that_does_not_fit_stays_plain() {
        let code = "let x = 1;\n";
        let highlighter = Highlighter::with_budget(code.len() * 2);
        assert!(highlighted(&highlighter, "rust", code).contains("<span"));
        let big = code.repeat(2);
        assert!(
            !highlighted(&highlighter, "rust", &big).contains("<span"),
            "over budget"
        );
        assert!(
            highlighted(&highlighter, "rust", code).contains("<span"),
            "a smaller one still fits"
        );
        assert!(
            !highlighted(&highlighter, "rust", code).contains("<span"),
            "budget is gone"
        );
    }

    #[test]
    fn tags_are_written_with_escaped_attributes_in_a_fixed_order() {
        let mut out = String::new();
        let attributes = HashMap::from([
            ("data-b", Cow::Borrowed("2")),
            ("class", Cow::Borrowed("language-\"x")),
        ]);
        Highlighter::new()
            .write_code_tag(&mut out, attributes)
            .unwrap();
        assert_eq!(out, "<code class=\"language-&quot;x\" data-b=\"2\">");
    }
}
