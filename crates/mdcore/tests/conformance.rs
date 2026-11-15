//! Two test sets over the official spec examples (plan sections 3 and 11).
//!
//! 1. **Parser conformance.** Every example of the CommonMark and GitHub
//!    Flavored Markdown specs goes through the parser with spec settings (raw
//!    HTML kept, no sanitizer, no ezmark additions) and must produce the
//!    spec's HTML exactly.
//! 2. **Pipeline snapshots.** The same inputs go through `mdcore::render`,
//!    the full pipeline a reader sees, and the output is compared with the
//!    saved files under `tests/pipeline/`. The pipeline differs from the spec
//!    on purpose (it sanitizes, adds heading ids, blocks remote images, and
//!    more; `tests/pipeline/README.md` lists every kind of difference), so
//!    the gate is "nothing changed since a person last looked", not "equals
//!    the spec".
//!
//! To accept a change in pipeline output, regenerate the snapshots and read
//! the diff before committing it:
//!
//! ```text
//! EZMARK_BLESS=1 cargo test -p mdcore --test conformance
//! ```

use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// One suite: a spec file and the snapshot of its pipeline output.
struct Suite {
    name: &'static str,
    spec: &'static str,
    snapshot: &'static str,
    /// How many examples the spec holds that are not marked `disabled`.
    examples: usize,
    /// cmark-gfm's own quirks, which the GFM spec's expected HTML depends on.
    gfm_quirks: bool,
}

const COMMONMARK: Suite = Suite {
    name: "CommonMark 0.31.2",
    spec: "tests/spec/commonmark-0.31.2.txt",
    snapshot: "tests/pipeline/commonmark-0.31.2.snap",
    examples: 652,
    gfm_quirks: false,
};

const GFM: Suite = Suite {
    name: "GitHub Flavored Markdown 0.29-gfm",
    spec: "tests/spec/gfm.txt",
    snapshot: "tests/pipeline/gfm.snap",
    examples: 670,
    gfm_quirks: true,
};

struct Example {
    number: usize,
    section: String,
    /// Extensions named on the example's opening line.
    extensions: Vec<String>,
    markdown: String,
    html: String,
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(relative: &str) -> String {
    let path = repo_root().join(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        .replace("\r\n", "\n")
}

/// Read a spec file the way the specs' own `spec_tests.py` does: an example
/// opens with 32 backticks and the word `example`, a line holding only `.`
/// separates the markdown from its HTML, and `→` stands for a tab.
fn examples(spec: &str) -> Vec<Example> {
    #[derive(PartialEq)]
    enum State {
        Prose,
        Markdown,
        Html,
    }
    let fence = "`".repeat(32);
    let opening = format!("{fence} example");
    let mut state = State::Prose;
    let mut examples = Vec::new();
    let mut number = 0;
    let mut section = String::new();
    let (mut extensions, mut markdown, mut html) = (Vec::new(), String::new(), String::new());

    for line in spec.split_inclusive('\n') {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(&opening) {
            state = State::Markdown;
            extensions = rest.split_whitespace().map(str::to_owned).collect();
        } else if trimmed == fence {
            state = State::Prose;
            number += 1;
            let (markdown, html) = (std::mem::take(&mut markdown), std::mem::take(&mut html));
            if !extensions.iter().any(|extension| extension == "disabled") {
                examples.push(Example {
                    number,
                    section: section.clone(),
                    extensions: std::mem::take(&mut extensions),
                    markdown: markdown.replace('→', "\t"),
                    html: html.replace('→', "\t"),
                });
            }
        } else if trimmed == "." && state != State::Prose {
            state = State::Html;
        } else if state == State::Markdown {
            markdown.push_str(line);
        } else if state == State::Html {
            html.push_str(line);
        } else if line.starts_with('#') && line.trim_start_matches('#').starts_with(' ') {
            section = line.trim_start_matches('#').trim().to_owned();
        }
    }
    examples
}

/// The parser as the specs' own test runs configure it: defaults, raw HTML
/// kept, and only the extensions an example asks for.
fn spec_html(example: &Example, suite: &Suite) -> String {
    let mut options = comrak::Options::default();
    options.render.r#unsafe = true;
    options.render.gfm_quirks = suite.gfm_quirks;
    for extension in &example.extensions {
        match extension.as_str() {
            "table" => options.extension.table = true,
            "strikethrough" => options.extension.strikethrough = true,
            "autolink" => options.extension.autolink = true,
            "tasklist" => options.extension.tasklist = true,
            "footnotes" => options.extension.footnotes = true,
            other => panic!("example {}: unknown extension {other}", example.number),
        }
    }
    comrak::markdown_to_html(&example.markdown, &options)
}

fn check_conformance(suite: &Suite) {
    let examples = examples(&read(suite.spec));
    assert_eq!(
        examples.len(),
        suite.examples,
        "{}: the spec file does not hold the expected number of examples",
        suite.name
    );
    let mut failures = String::new();
    let mut failed = 0;
    let mut ignored = 0;
    for example in &examples {
        let actual = spec_html(example, suite);
        // The spec's own runner passes an example whose expected HTML is this
        // word. GFM has one: the tag filter, which here is the sanitizer's job
        // and is pinned by the pipeline snapshot instead.
        if example.html.trim() == "<IGNORE>" {
            ignored += 1;
        } else if actual != example.html {
            failed += 1;
            let _ = write!(
                failures,
                "\n--- example {} ({})\n{}--- expected\n{}--- actual\n{actual}",
                example.number, example.section, example.markdown, example.html
            );
        }
    }
    assert!(
        failed == 0,
        "{}: {failed} of {} examples failed{failures}",
        suite.name,
        examples.len()
    );
    assert!(
        ignored <= 1,
        "{}: {ignored} examples have no expected HTML",
        suite.name
    );
    println!(
        "{}: {} of {} examples pass ({ignored} of them with no expected HTML in the spec)",
        suite.name,
        examples.len(),
        examples.len()
    );
}

/// A folder made for one snapshot run and removed after it: empty, canonical
/// (as `RenderContext` requires), and with an empty parent of its own, so
/// that neither a relative reference nor a `../` one can find a file that
/// happens to exist on this machine. Every local image is then "missing".
///
/// References that start at the filesystem root (`/url`) are the one thing
/// this cannot fence in; the examples use names that no system has there.
struct EmptyFolder {
    outer: PathBuf,
    inner: PathBuf,
}

impl EmptyFolder {
    fn new(name: &str) -> Self {
        let outer =
            std::env::temp_dir().join(format!("ezmark-conformance-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&outer);
        std::fs::create_dir_all(outer.join("document")).unwrap();
        let outer = std::fs::canonicalize(outer).unwrap();
        let inner = outer.join("document");
        Self { outer, inner }
    }

    fn context(&self) -> mdcore::RenderContext {
        mdcore::RenderContext {
            root: self.inner.clone(),
            doc_dir: self.inner.clone(),
            asset_url: Arc::new(|raw: &str| format!("asset://test/{raw}")),
        }
    }
}

impl Drop for EmptyFolder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.outer);
    }
}

/// Pipeline output without the block markers, for telling whether the
/// pipeline changed anything the spec defines.
fn without_markers(html: &str) -> String {
    let opening = format!("<{} {}=\"", mdcore::MARKER_TAG, mdcore::MARKER_ATTRIBUTE);
    let closing = format!("\"></{}>\n", mdcore::MARKER_TAG);
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(at) = rest.find(&opening) {
        out.push_str(&rest[..at]);
        let after = &rest[at + opening.len()..];
        rest = after
            .find(&closing)
            .map_or(after, |end| &after[end + closing.len()..]);
    }
    out.push_str(rest);
    out
}

fn snapshot(suite: &Suite) -> String {
    let folder = EmptyFolder::new(if suite.gfm_quirks {
        "gfm"
    } else {
        "commonmark"
    });
    let ctx = folder.context();
    let examples = examples(&read(suite.spec));
    let mut body = String::new();
    let mut same = 0;
    for example in &examples {
        let html = mdcore::render(&example.markdown, &ctx).html;
        let verdict = if without_markers(&html) == example.html {
            same += 1;
            "same as the spec"
        } else {
            "differs from the spec"
        };
        let _ = write!(
            body,
            "\n======== example {}: {} ({verdict})\n{}-------- pipeline output\n{html}",
            example.number,
            example.section,
            example.markdown.replace('\t', "→"),
        );
        if !html.ends_with('\n') {
            body.push('\n');
        }
    }
    format!(
        "Pipeline output for the examples of {}.\n\
         Written by crates/mdcore/tests/conformance.rs; do not edit by hand.\n\
         Regenerate with EZMARK_BLESS=1 cargo test -p mdcore --test conformance\n\
         and read the diff. tests/pipeline/README.md explains the differences.\n\
         A tab is shown as → in the markdown, as in the spec.\n\n\
         {} examples: {same} same as the spec once block markers are set aside, {} differ.\n{body}",
        suite.name,
        examples.len(),
        examples.len() - same,
    )
}

fn check_snapshot(suite: &Suite) {
    let actual = snapshot(suite);
    let path = repo_root().join(suite.snapshot);
    if std::env::var_os("EZMARK_BLESS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &actual).unwrap();
        println!("wrote {}", path.display());
        return;
    }
    let saved = read(suite.snapshot);
    if saved == actual {
        return;
    }
    let (line, saved_line, actual_line) = saved
        .lines()
        .zip(actual.lines())
        .enumerate()
        .find(|(_, (saved, actual))| saved != actual)
        .map(|(index, (saved, actual))| (index + 1, saved, actual))
        .unwrap_or((
            saved.lines().count().min(actual.lines().count()) + 1,
            "",
            "",
        ));
    panic!(
        "{}: pipeline output no longer matches {}.\n\
         First difference at line {line}:\n  saved:  {saved_line}\n  actual: {actual_line}\n\
         If the change is intended, run\n  EZMARK_BLESS=1 cargo test -p mdcore --test conformance\n\
         and review the diff of the snapshot before committing it.",
        suite.name, suite.snapshot,
    );
}

#[test]
fn the_parser_passes_the_commonmark_spec() {
    check_conformance(&COMMONMARK);
}

#[test]
fn the_parser_passes_the_gfm_spec() {
    check_conformance(&GFM);
}

#[test]
fn the_pipeline_output_for_the_commonmark_spec_matches_its_snapshot() {
    check_snapshot(&COMMONMARK);
}

#[test]
fn the_pipeline_output_for_the_gfm_spec_matches_its_snapshot() {
    check_snapshot(&GFM);
}

#[test]
fn spec_files_are_read_the_way_their_own_runner_reads_them() {
    let fence = "`".repeat(32);
    let spec = format!(
        "# Tabs\n\n{fence} example\n→foo\n.\n<pre>x</pre>\n{fence}\n\n## Tables (extension)\n\n\
         {fence} example table\n| a |\n.\n<table>\n{fence}\n\n{fence} example disabled\nx\n.\ny\n{fence}\n\n\
         {fence} example\n.\n{fence}\n"
    );
    let examples = examples(&spec);
    assert_eq!(examples.len(), 3, "the disabled example is skipped");
    assert_eq!(examples[0].markdown, "\tfoo\n");
    assert_eq!(examples[0].html, "<pre>x</pre>\n");
    assert_eq!(examples[0].section, "Tabs");
    assert_eq!(examples[1].extensions, ["table"]);
    assert_eq!(examples[1].section, "Tables (extension)");
    assert_eq!(
        examples[2].number, 4,
        "numbering counts the disabled example"
    );
    assert_eq!(
        (examples[2].markdown.as_str(), examples[2].html.as_str()),
        ("", "")
    );
}

#[test]
fn markers_are_set_aside_when_comparing_with_the_spec() {
    let html = "<ezmark-block key=\"0123456789abcdef\"></ezmark-block>\n<p>a</p>\n\
                <ezmark-block key=\"fedcba9876543210\"></ezmark-block>\n<p>b</p>\n";
    assert_eq!(without_markers(html), "<p>a</p>\n<p>b</p>\n");
}
