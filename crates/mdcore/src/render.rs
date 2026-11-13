//! Markdown to sanitized HTML.
//!
//! Parse with comrak (raw HTML preserved), then sanitize with ammonia while
//! enforcing the URL policy from plan section 7. The allowlist is extended just
//! far enough for GitHub's features to survive: task-list checkboxes, alert
//! and footnote classes, heading and footnote ids, code language classes,
//! `align`, and `<picture>`. Each addition is pinned by a test in this file.
//!
//! Two things leave this module in a form the view layer finishes:
//!
//! - An image that may not load keeps its `<img>` with `src` set to
//!   `ezmark-blocked:<reason>:<hex of the original reference>`. That scheme is
//!   not in the CSP, so it cannot load anything; the view replaces the element
//!   with a text placeholder that says why.
//! - Every `id` (and `<a name>`) is prefixed with `user-content-`, as GitHub
//!   does, so a document cannot shadow the app's own elements. Fragment links
//!   get the same prefix, so in-page jumps keep working without script.
//!
//! Highlighting, front matter, and image dimensions arrive in M1.

use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::Arc;

use crate::boundary::{RealFs, Rejection, split_reference, validate_reference};
use crate::hex;
use crate::nesting::{self, MAX_DEPTH};

/// Turns a validated raw image reference into the URL the webview requests it by.
pub type AssetUrlFn = dyn Fn(&str) -> String + Send + Sync;

/// Prefix on every document-supplied `id`, `name`, and fragment link.
pub const ID_PREFIX: &str = "user-content-";

/// Scheme of the inert `src` an image gets when it may not load.
pub const BLOCKED_IMAGE_SCHEME: &str = "ezmark-blocked";

/// The plain-text fallback shows at most this much of the source.
pub const PLAIN_TEXT_LIMIT: usize = 2 * 1024 * 1024;

/// The longest original reference carried inside a blocked-image marker.
const BLOCKED_DETAIL_LIMIT: usize = 2048;

/// Where a document lives, for resolving the local references it makes.
#[derive(Clone)]
pub struct RenderContext {
    /// Canonical directory of the document being rendered.
    pub doc_dir: PathBuf,
    /// Canonical active root. Local references must resolve under it.
    pub root: PathBuf,
    /// Builds an asset URL for a raw reference that passed validation.
    pub asset_url: Arc<AssetUrlFn>,
}

impl std::fmt::Debug for RenderContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RenderContext")
            .field("doc_dir", &self.doc_dir)
            .field("root", &self.root)
            .finish_non_exhaustive()
    }
}

/// The result of rendering one document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rendered {
    /// Sanitized HTML, safe to place in the viewer's content element.
    pub html: String,
    /// Set when the document was shown as plain text instead of rendered.
    pub fallback: Option<Fallback>,
}

/// Why and how a document was shown as plain text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fallback {
    /// True if the source was longer than [`PLAIN_TEXT_LIMIT`] and was cut.
    pub truncated: bool,
}

/// Render markdown to HTML that is safe to place in the viewer's content element.
///
/// A document nested deeper than [`MAX_DEPTH`] is shown as escaped plain text
/// instead: parsing and sanitizing such input takes time that grows with the
/// square of the depth (see `nesting.rs`).
pub fn render(source: &str, ctx: &RenderContext) -> Rendered {
    if nesting::quote_depth_exceeds(source, MAX_DEPTH) {
        return plain_text(source, ctx);
    }
    let unsafe_html = comrak::markdown_to_html(source, &parser_options());
    if nesting::html_depth_exceeds(&unsafe_html, MAX_DEPTH) {
        return plain_text(source, ctx);
    }
    Rendered {
        html: sanitize(&unsafe_html, ctx),
        fallback: None,
    }
}

/// The source as escaped text in one `<pre>`, cut at [`PLAIN_TEXT_LIMIT`].
fn plain_text(source: &str, ctx: &RenderContext) -> Rendered {
    let mut end = source.len().min(PLAIN_TEXT_LIMIT);
    while !source.is_char_boundary(end) {
        end -= 1;
    }
    let mut html = String::with_capacity(end + end / 8 + 64);
    html.push_str("<pre class=\"plain-text\">");
    for c in source[..end].chars() {
        match c {
            '&' => html.push_str("&amp;"),
            '<' => html.push_str("&lt;"),
            '>' => html.push_str("&gt;"),
            _ => html.push(c),
        }
    }
    html.push_str("</pre>");
    Rendered {
        // Escaped text is safe by construction; it still goes through the
        // sanitizer so that nothing leaves this crate without doing so.
        html: sanitize(&html, ctx),
        fallback: Some(Fallback {
            truncated: end < source.len(),
        }),
    }
}

/// GitHub-flavoured parsing with raw HTML kept, because the sanitizer, not the
/// parser, decides what survives.
fn parser_options() -> comrak::Options<'static> {
    let mut options = comrak::Options::default();
    options.extension.strikethrough = true;
    options.extension.table = true;
    options.extension.autolink = true;
    options.extension.tasklist = true;
    options.extension.footnotes = true;
    options.extension.alerts = true;
    options.extension.front_matter_delimiter = Some("---".to_owned());
    // Slug ids on headings. The sanitizer adds the `user-content-` prefix, so
    // the parser adds none.
    options.extension.header_id_prefix = Some(String::new());
    options.render.tasklist_classes = true;
    options.render.r#unsafe = true;
    options
}

/// Sanitize parser output.
///
/// ammonia first drops every element and attribute outside the allowlist and
/// every absolute URL whose scheme is not `http`, `https`, or `mailto`. What
/// survives then passes through [`attribute_policy`], one attribute at a time.
pub fn sanitize(html: &str, ctx: &RenderContext) -> String {
    let ctx = ctx.clone();
    let mut builder = ammonia::Builder::default();
    builder
        .add_tags(["input", "picture", "section", "source", "tfoot"])
        .add_generic_attributes(["id", "class", "aria-label", "aria-hidden"])
        .add_tag_attributes("a", ["name"])
        .add_tag_attributes("img", ["srcset"])
        .add_tag_attributes("input", ["type", "checked", "disabled"])
        .add_tag_attributes("source", ["srcset", "media", "type"])
        // The only input a document can produce is a disabled checkbox,
        // whatever attributes it asked for.
        .set_tag_attribute_value("input", "type", "checkbox")
        .set_tag_attribute_value("input", "disabled", "")
        .id_prefix(Some(ID_PREFIX))
        .url_schemes(["http", "https", "mailto"].into_iter().collect())
        .url_relative(ammonia::UrlRelative::PassThrough)
        .attribute_filter(move |element, attribute, value| {
            attribute_policy(&ctx, element, attribute, value)
        });
    for tag in ["p", "div", "h1", "h2", "h3", "h4", "h5", "h6"] {
        builder.add_tag_attributes(tag, ["align"]);
    }
    builder.clean(html).to_string()
}

/// Every attribute that survives the allowlist passes through here.
fn attribute_policy<'u>(
    ctx: &RenderContext,
    element: &str,
    attribute: &str,
    value: &'u str,
) -> Option<Cow<'u, str>> {
    match (element, attribute) {
        // Local images are the only resources a document may load, and only
        // through the asset protocol. Anything else becomes an inert marker.
        ("img", "src") => Some(Cow::Owned(image_url(ctx, value))),
        ("img" | "source", "srcset") => local_srcset(ctx, value).map(Cow::Owned),
        // Nothing else may make the webview fetch anything.
        (
            _,
            "src" | "srcset" | "poster" | "data" | "background" | "action" | "formaction" | "ping"
            | "longdesc" | "xlink:href",
        ) => None,
        // A citation URL is never shown and never fetched; it has no use here.
        (_, "cite") => None,
        // Links never navigate the webview: the frontend intercepts every click.
        (_, "href") => link_target(value),
        ("a", "name") => Some(prefixed(value)),
        (_, "class") => allowed_classes(element, value),
        (_, "align") => is_alignment(value).then_some(Cow::Borrowed(value)),
        _ => Some(Cow::Borrowed(value)),
    }
}

/// The asset URL for a local image inside the root, or a blocked marker.
fn image_url(ctx: &RenderContext, value: &str) -> String {
    let reference = value.trim();
    let (path, _fragment) = split_reference(reference);
    match validate_reference(path, &ctx.doc_dir, &ctx.root, &RealFs) {
        Ok(_) => (ctx.asset_url)(path),
        Err(rejection) => blocked_image(blocked_reason(reference, rejection), reference),
    }
}

/// The word the view layer keys its placeholder text on.
fn blocked_reason(reference: &str, rejection: Rejection) -> &'static str {
    let lower = reference.to_ascii_lowercase();
    if lower.starts_with("http:") || lower.starts_with("https:") || lower.starts_with("//") {
        return "remote";
    }
    match rejection {
        Rejection::OutsideRoot => "outside",
        Rejection::NotFound => "missing",
        _ => "refused",
    }
}

fn blocked_image(reason: &str, reference: &str) -> String {
    let mut end = reference.len().min(BLOCKED_DETAIL_LIMIT);
    while !reference.is_char_boundary(end) {
        end -= 1;
    }
    format!(
        "{BLOCKED_IMAGE_SCHEME}:{reason}:{}",
        hex::encode(&reference[..end])
    )
}

/// Rewrite a `srcset`, keeping only candidates that are local images inside
/// the root. `None` if no candidate survives.
fn local_srcset(ctx: &RenderContext, value: &str) -> Option<String> {
    let mut kept = Vec::new();
    for candidate in value.split(',') {
        let mut parts = candidate.split_ascii_whitespace();
        let Some(url) = parts.next() else { continue };
        let descriptor = parts.next();
        if parts.next().is_some() || !descriptor.is_none_or(is_srcset_descriptor) {
            continue;
        }
        let (path, _fragment) = split_reference(url);
        if validate_reference(path, &ctx.doc_dir, &ctx.root, &RealFs).is_err() {
            continue;
        }
        let url = (ctx.asset_url)(path);
        kept.push(match descriptor {
            Some(descriptor) => format!("{url} {descriptor}"),
            None => url,
        });
    }
    (!kept.is_empty()).then(|| kept.join(", "))
}

/// `2x`, `1.5x`, `640w`.
fn is_srcset_descriptor(descriptor: &str) -> bool {
    let Some(number) = descriptor
        .strip_suffix('x')
        .or_else(|| descriptor.strip_suffix('w'))
    else {
        return false;
    };
    !number.is_empty()
        && number.chars().all(|c| c.is_ascii_digit() || c == '.')
        && number.parse::<f64>().is_ok()
}

/// What a link may point at: a fragment (prefixed like the ids it targets),
/// an `http`, `https`, or `mailto` URL, or a relative path for the M2 resolver.
///
/// ammonia has already dropped other schemes. This repeats that check on the
/// string a browser would actually parse, and drops the network-path forms
/// (`//host/x`, `\\host\x`) that a browser would resolve to another host.
fn link_target(value: &str) -> Option<Cow<'_, str>> {
    let cleaned = as_a_browser_reads_it(value);
    if let Some(fragment) = cleaned.strip_prefix('#') {
        return Some(if fragment.is_empty() || fragment.starts_with(ID_PREFIX) {
            cleaned
        } else {
            Cow::Owned(format!("#{ID_PREFIX}{fragment}"))
        });
    }
    let mut chars = cleaned.chars();
    if let (Some(a), Some(b)) = (chars.next(), chars.next())
        && matches!(a, '/' | '\\')
        && matches!(b, '/' | '\\')
    {
        return None;
    }
    match scheme_of(&cleaned) {
        Some(scheme) => ["http", "https", "mailto"]
            .iter()
            .any(|allowed| scheme.eq_ignore_ascii_case(allowed))
            .then_some(cleaned),
        None => Some(cleaned),
    }
}

/// Browsers strip leading and trailing spaces and control characters from a
/// URL, and remove tabs and newlines anywhere in it, before parsing.
fn as_a_browser_reads_it(value: &str) -> Cow<'_, str> {
    let trimmed = value.trim_matches(|c: char| c <= ' ');
    if trimmed.contains(['\t', '\n', '\r']) {
        Cow::Owned(trimmed.replace(['\t', '\n', '\r'], ""))
    } else {
        Cow::Borrowed(trimmed)
    }
}

/// The scheme of `url`, if it starts with one (`alpha *(alpha / digit / + - .) :`).
fn scheme_of(url: &str) -> Option<&str> {
    let (candidate, _) = url.split_once(':')?;
    let mut chars = candidate.chars();
    let valid = chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    valid.then_some(candidate)
}

fn prefixed(value: &str) -> Cow<'_, str> {
    if value.starts_with(ID_PREFIX) {
        Cow::Borrowed(value)
    } else {
        Cow::Owned(format!("{ID_PREFIX}{value}"))
    }
}

/// Keep only the class names the stylesheet knows, per element.
fn allowed_classes<'u>(element: &str, value: &'u str) -> Option<Cow<'u, str>> {
    let kept: Vec<&str> = value
        .split_ascii_whitespace()
        .filter(|class| is_allowed_class(element, class))
        .collect();
    if kept.is_empty() {
        None
    } else if kept.len() == value.split_ascii_whitespace().count() {
        Some(Cow::Borrowed(value))
    } else {
        Some(Cow::Owned(kept.join(" ")))
    }
}

fn is_allowed_class(element: &str, class: &str) -> bool {
    match element {
        "div" => matches!(
            class,
            "markdown-alert"
                | "markdown-alert-note"
                | "markdown-alert-tip"
                | "markdown-alert-important"
                | "markdown-alert-warning"
                | "markdown-alert-caution"
        ),
        "p" => class == "markdown-alert-title",
        "ul" => class == "contains-task-list",
        "li" => class == "task-list-item",
        "input" => class == "task-list-item-checkbox",
        "sup" => class == "footnote-ref",
        "section" => class == "footnotes",
        "a" => matches!(class, "anchor" | "footnote-backref"),
        "pre" => class == "plain-text",
        // The language of a fenced code block, for highlighting in M1.
        "code" => class.strip_prefix("language-").is_some_and(|language| {
            (1..=40).contains(&language.len())
                && language
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '+' | '#' | '.' | '-'))
        }),
        _ => false,
    }
}

fn is_alignment(value: &str) -> bool {
    ["left", "right", "center", "justify"]
        .iter()
        .any(|allowed| value.eq_ignore_ascii_case(allowed))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(doc_dir: PathBuf) -> RenderContext {
        RenderContext {
            root: doc_dir.clone(),
            doc_dir,
            asset_url: Arc::new(|raw: &str| format!("asset://test/{raw}")),
        }
    }

    /// A scratch directory that is removed when the test ends.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("mdcore-render-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(dir.join("img")).unwrap();
            Self(std::fs::canonicalize(dir).unwrap())
        }

        fn context(&self) -> RenderContext {
            context(self.0.clone())
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Renders with a context whose directory does not exist: every local
    /// reference is "missing", which is all these tests need.
    fn html(source: &str) -> String {
        let nowhere = std::env::temp_dir().join("mdcore-render-nowhere");
        render(source, &context(nowhere)).html
    }

    fn blocked(reason: &str, reference: &str) -> String {
        format!("ezmark-blocked:{reason}:{}", hex::encode(reference))
    }

    #[test]
    fn renders_gfm() {
        let html = html("# Title\n\n| a |\n|---|\n| b |\n\n~~gone~~\n");
        assert!(html.contains(">Title<"), "{html}");
        assert!(html.contains("<table>"), "{html}");
        assert!(html.contains("<del>gone</del>"), "{html}");
    }

    /// The extended allowlist, feature by feature: what GitHub's features need
    /// survives, and nothing a document could abuse comes with it.
    #[test]
    fn github_features_survive_the_sanitizer() {
        let cases: &[(&str, &str, &[&str])] = &[
            (
                "task list",
                "- [x] done\n- [ ] todo\n",
                &[
                    "<ul class=\"contains-task-list\">",
                    "<li class=\"task-list-item\">",
                    "class=\"task-list-item-checkbox\"",
                    "type=\"checkbox\"",
                    "checked=\"\"",
                    "disabled=\"\"",
                ],
            ),
            (
                "alert",
                "> [!WARNING]\n> careful\n",
                &[
                    "<div class=\"markdown-alert markdown-alert-warning\">",
                    "<p class=\"markdown-alert-title\">Warning</p>",
                ],
            ),
            (
                "footnote",
                "text[^1]\n\n[^1]: note\n",
                &[
                    "<sup class=\"footnote-ref\">",
                    "href=\"#user-content-fn-1\"",
                    "id=\"user-content-fnref-1\"",
                    "<section class=\"footnotes\">",
                    "<li id=\"user-content-fn-1\">",
                    "href=\"#user-content-fnref-1\"",
                    "class=\"footnote-backref\"",
                    "aria-label=\"Back to reference 1\"",
                ],
            ),
            (
                "heading anchor",
                "# Intro\n\n[go](#intro)\n",
                &[
                    "<h1 id=\"user-content-intro\">",
                    "class=\"anchor\"",
                    "<a href=\"#user-content-intro\" rel=\"noopener noreferrer\">go</a>",
                ],
            ),
            (
                "code language",
                "```rust\nfn main() {}\n```\n",
                &["<pre><code class=\"language-rust\">"],
            ),
            (
                "aligned header",
                "<h1 align=\"center\">Name</h1>\n\n<p align=\"center\">tagline</p>\n",
                &["<h1 align=\"center\">", "<p align=\"center\">"],
            ),
            (
                "table alignment",
                "| a |\n|:-:|\n| b |\n",
                &["<th align=\"center\">", "<td align=\"center\">"],
            ),
            (
                "named anchor",
                "<a name=\"install\"></a>\n\n[go](#install)\n",
                &[
                    "name=\"user-content-install\"",
                    "href=\"#user-content-install\"",
                ],
            ),
            (
                "details",
                "<details><summary>More</summary>\n\nbody\n\n</details>\n",
                &["<details>", "<summary>More</summary>"],
            ),
        ];
        for (name, source, expected) in cases {
            let html = html(source);
            for needle in *expected {
                assert!(html.contains(needle), "{name}: no {needle} in {html}");
            }
        }
    }

    #[test]
    fn the_extended_allowlist_cannot_be_abused() {
        let cases: &[(&str, &str, &[&str], &[&str])] = &[
            (
                "a raw input is always a disabled checkbox",
                "<input type=\"text\" name=\"q\" value=\"typed\" onfocus=\"x()\" autofocus>\n",
                &["type=\"checkbox\"", "disabled=\"\""],
                &["text", "name=", "value=", "onfocus", "autofocus"],
            ),
            (
                "ids cannot shadow the app's own",
                "<div id=\"content\">x</div>\n\n<a name=\"status\">y</a>\n",
                &[
                    "id=\"user-content-content\"",
                    "name=\"user-content-status\"",
                ],
                &["id=\"content\"", "name=\"status\""],
            ),
            (
                "only known classes survive",
                "<div class=\"markdown-alert notice evil\">x</div>\n\n<p class=\"notice\">y</p>\n\n\
                 <span class=\"anchor\">z</span>\n",
                &[
                    "<div class=\"markdown-alert\">",
                    "<p>y</p>",
                    "<span>z</span>",
                ],
                &["notice", "evil"],
            ),
            (
                "a code class must look like a language",
                "<code class=\"language-rust language-&quot;x markdown-alert\">c</code>\n",
                &["<code class=\"language-rust\">"],
                &["markdown-alert", "quot"],
            ),
            (
                "align takes only the four keywords",
                "<p align=\"center;background:url(x)\">a</p>\n\n<p align=\"RIGHT\">b</p>\n",
                &["<p>a</p>", "<p align=\"RIGHT\">"],
                &["url("],
            ),
            (
                "style and event attributes never survive",
                "<p style=\"color:red\" onclick=\"x()\" class=\"markdown-alert-title\">a</p>\n",
                &["<p class=\"markdown-alert-title\">a</p>"],
                &["style", "onclick"],
            ),
        ];
        for (name, source, expected, forbidden) in cases {
            let html = html(source);
            for needle in *expected {
                assert!(html.contains(needle), "{name}: no {needle} in {html}");
            }
            for needle in *forbidden {
                assert!(!html.contains(needle), "{name}: {needle} in {html}");
            }
        }
    }

    #[test]
    fn removes_executable_content() {
        let source = concat!(
            "<script>alert(1)</script>\n\n",
            "<img src=x onerror=alert(1)>\n\n",
            "[click](javascript:alert(1))\n\n",
            "<a href=\"vbscript:msgbox(1)\">v</a>\n\n",
            "<iframe src=\"https://example.com\"></iframe>\n\n",
            "<object data=\"x.swf\"></object>\n\n",
            "<svg onload=alert(1)></svg>\n\n",
            "<form action=\"https://example.com\"><input></form>\n\n",
            "<meta http-equiv=refresh content=\"0;url=https://example.com\">\n\n",
            "<base href=\"https://example.com/\">\n\n",
            "<style>body{display:none}</style>\n\n",
            "<link rel=\"preconnect\" href=\"https://example.com\">\n",
        );
        let html = html(source).to_ascii_lowercase();
        for needle in [
            "<script",
            "onerror",
            "javascript:",
            "vbscript:",
            "<iframe",
            "<object",
            "onload",
            "<form",
            "<meta",
            "<base",
            "<style",
            "<svg",
            "<link",
        ] {
            assert!(!html.contains(needle), "{needle} survived in {html}");
        }
    }

    #[test]
    fn images_that_may_not_load_become_inert_markers() {
        let dir = Scratch::new("blocked");
        std::fs::write(dir.0.join("img").join("in.png"), b"png").unwrap();
        let html = render(
            "![a](https://example.com/a.png) ![b](HTTP://example.com/b.png) \
             ![c](//cdn.example.com/c.png) ![d](../outside.png) ![e](img/none.png) \
             <img src=\"\\\\server\\share\\f.png\"> <img src=\"data:image/png;base64,AAAA\"> \
             <img srcset=\"https://example.com/g.png 2x\"> ![ok](img/in.png)",
            &dir.context(),
        )
        .html;

        for (reason, reference) in [
            ("remote", "https://example.com/a.png"),
            ("remote", "HTTP://example.com/b.png"),
            ("remote", "//cdn.example.com/c.png"),
            ("missing", "../outside.png"),
            ("missing", "img/none.png"),
            ("refused", "\\\\server\\share\\f.png"),
        ] {
            let marker = blocked(reason, reference);
            assert!(
                html.contains(&marker),
                "no {reason} marker for {reference}: {html}"
            );
        }
        // Nothing but the marker carries the original reference.
        assert!(!html.contains("example.com"), "{html}");
        assert!(!html.contains("data:"), "{html}");
        assert!(!html.contains("srcset"), "{html}");
        assert!(html.contains("src=\"asset://test/img/in.png\""), "{html}");
    }

    #[test]
    fn an_image_outside_the_root_says_so() {
        let dir = Scratch::new("outside");
        std::fs::create_dir_all(dir.0.join("docs")).unwrap();
        std::fs::write(dir.0.join("above.png"), b"png").unwrap();
        let ctx = context(dir.0.join("docs"));
        let html = render("![up](../above.png)", &ctx).html;
        assert!(html.contains(&blocked("outside", "../above.png")), "{html}");
    }

    #[test]
    fn local_images_are_rewritten_inside_the_root_only() {
        let dir = Scratch::new("local");
        std::fs::write(dir.0.join("img").join("p q.png"), b"png").unwrap();
        std::fs::write(dir.0.join("img").join("100%.png"), b"png").unwrap();
        let html = render(
            "![in](img/p%20q.png) ![raw](<img/p q.png>) <img src=\"img/p%20q.png\"> \
             <img src=\" img/p%20q.png \"> ![percent](img/100%.png) <img src=\"img/100%.png\">",
            &dir.context(),
        )
        .html;
        assert_eq!(
            html.matches("asset://test/img/p%20q.png").count(),
            4,
            "{html}"
        );
        // comrak escapes a bare `%` in markdown syntax; raw HTML passes it as written.
        assert!(html.contains("asset://test/img/100%25.png"), "{html}");
        assert!(html.contains("asset://test/img/100%.png"), "{html}");
        assert!(!html.contains(BLOCKED_IMAGE_SCHEME), "{html}");
    }

    #[test]
    fn picture_sources_keep_only_local_candidates() {
        let dir = Scratch::new("picture");
        for name in ["dark.png", "light.png", "big.png"] {
            std::fs::write(dir.0.join("img").join(name), b"png").unwrap();
        }
        let html = render(
            "<picture>\
             <source media=\"(prefers-color-scheme: dark)\" srcset=\"img/dark.png\">\
             <source srcset=\"https://example.com/remote.png, img/big.png 2x, img/none.png 3x\">\
             <source srcset=\"https://example.com/only-remote.png\" src=\"img/light.png\">\
             <img src=\"img/light.png\" srcset=\"img/big.png 2x, ../x.png 3x, img/big.png nonsense\">\
             </picture>",
            &dir.context(),
        )
        .html;
        assert!(
            html.contains(
                "<source media=\"(prefers-color-scheme: dark)\" srcset=\"asset://test/img/dark.png\">"
            ),
            "{html}"
        );
        assert!(
            html.contains("<source srcset=\"asset://test/img/big.png 2x\">"),
            "{html}"
        );
        assert!(html.contains("<source>"), "{html}");
        assert!(
            html.contains("src=\"asset://test/img/light.png\""),
            "{html}"
        );
        assert_eq!(html.matches("asset://test/img/big.png 2x").count(), 2);
        assert!(!html.contains("example.com"), "{html}");
        assert!(!html.contains("none.png"), "{html}");
        assert!(!html.contains("nonsense"), "{html}");
    }

    #[test]
    fn links_keep_safe_targets_only() {
        let html = html(
            "[a](#intro) [b](https://example.com) [c](mailto:x@example.com) [d](other.md) \
             [e](http://localhost:3000/x) [f](file:///etc/passwd) [g](//evil.example/x) \
             <a href=\"\\\\srv\\share\\x\">h</a> <a href=\"/\\evil.example\">i</a> \
             [j](ftp://example.com) \
             [k](C:/Windows/x) <a href=\" \t//evil.example/y\">l</a> \
             <a href=\"java\tscript:alert(1)\">m</a> <a href=\"#\">n</a> \
             <a href=\"HTTPS://EXAMPLE.COM/z\">o</a>",
        );
        for kept in [
            "href=\"#user-content-intro\"",
            "href=\"https://example.com\"",
            "href=\"mailto:x@example.com\"",
            "href=\"other.md\"",
            "href=\"http://localhost:3000/x\"",
            "href=\"#\"",
            "href=\"HTTPS://EXAMPLE.COM/z\"",
        ] {
            assert!(html.contains(kept), "no {kept} in {html}");
        }
        for dropped in ["file:", "evil.example", "srv", "ftp:", "Windows", "script:"] {
            assert!(!html.contains(dropped), "{dropped} in {html}");
        }
    }

    #[test]
    fn too_deep_documents_fall_back_to_plain_text() {
        for source in [
            format!("{}deep\n", ">".repeat(MAX_DEPTH + 1)),
            format!(
                "{}\n\n<b>bold</b> & more\n",
                "<div>\n".repeat(MAX_DEPTH + 1)
            ),
        ] {
            let rendered = render(&source, &context(PathBuf::from("nowhere")));
            assert_eq!(rendered.fallback, Some(Fallback { truncated: false }));
            assert!(
                rendered.html.starts_with("<pre class=\"plain-text\">"),
                "{}",
                &rendered.html[..80]
            );
            assert!(!rendered.html.contains("<div"), "markup was not escaped");
            assert!(!rendered.html.contains("<b>"), "markup was not escaped");
        }
        let fine = format!("{}fine\n", "> ".repeat(50));
        let rendered = render(&fine, &context(PathBuf::from("nowhere")));
        assert_eq!(rendered.fallback, None);
        assert!(rendered.html.contains("<blockquote>"));
    }

    #[test]
    fn the_plain_text_fallback_is_cut_at_its_limit() {
        let mut source = ">".repeat(MAX_DEPTH + 1);
        source.push_str(&"é".repeat(PLAIN_TEXT_LIMIT));
        let rendered = render(&source, &context(PathBuf::from("nowhere")));
        assert_eq!(rendered.fallback, Some(Fallback { truncated: true }));
        assert!(rendered.html.len() < PLAIN_TEXT_LIMIT + (MAX_DEPTH + 1) * 4 + 64);
        assert!(rendered.html.ends_with("é</pre>"));
    }
}
