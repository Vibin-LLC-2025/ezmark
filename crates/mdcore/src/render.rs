//! Markdown to sanitized HTML.
//!
//! M0 scope: parse with comrak (raw HTML preserved), then sanitize with
//! ammonia while enforcing the URL policy from plan section 7. Highlighting,
//! front matter, image dimensions, the extended allowlist, and remote-image
//! placeholders arrive in M1.

use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::Arc;

use crate::boundary::{RealFs, split_reference, validate_reference};

/// Turns a validated raw image reference into the URL the webview requests it by.
pub type AssetUrlFn = dyn Fn(&str) -> String + Send + Sync;

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

/// Render markdown to HTML that is safe to place in the viewer's content element.
pub fn render(source: &str, ctx: &RenderContext) -> String {
    let unsafe_html = comrak::markdown_to_html(source, &parser_options());
    sanitize(&unsafe_html, ctx)
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
    options.render.r#unsafe = true;
    options
}

/// Sanitize parser output. ammonia removes disallowed elements and attributes
/// and any absolute URL whose scheme is not `https` or `mailto`; the attribute
/// filter then applies the per-attribute policy below.
pub fn sanitize(html: &str, ctx: &RenderContext) -> String {
    let ctx = ctx.clone();
    ammonia::Builder::default()
        .url_schemes(["https", "mailto"].into_iter().collect())
        .url_relative(ammonia::UrlRelative::PassThrough)
        .attribute_filter(move |element, attribute, value| {
            url_policy(&ctx, element, attribute, value)
        })
        .clean(html)
        .to_string()
}

/// Every URL-bearing attribute that survives the allowlist passes through here.
fn url_policy<'u>(
    ctx: &RenderContext,
    element: &str,
    attribute: &str,
    value: &'u str,
) -> Option<Cow<'u, str>> {
    match (element, attribute) {
        // Local images are the only resources a document may load, and only
        // through the asset protocol. Remote images are dropped (v1 blocks all
        // remote loading; the M1 placeholder replaces the bare `<img>`).
        ("img", "src") => local_image_url(ctx, value).map(Cow::Owned),
        // Nothing else may make the webview fetch anything.
        (
            _,
            "src" | "srcset" | "poster" | "data" | "background" | "action" | "formaction" | "ping"
            | "longdesc" | "xlink:href",
        ) => None,
        // Links never navigate the webview: the frontend intercepts every click.
        // Keep fragments, `https`, `mailto` (scheme-checked by ammonia), and
        // relative paths for the M2 resolver.
        (_, "href" | "cite") => Some(Cow::Borrowed(value)),
        _ => Some(Cow::Borrowed(value)),
    }
}

fn local_image_url(ctx: &RenderContext, value: &str) -> Option<String> {
    let (path, _fragment) = split_reference(value);
    validate_reference(path, &ctx.doc_dir, &ctx.root, &RealFs).ok()?;
    Some((ctx.asset_url)(path))
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

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mdcore-render-{name}-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("img")).unwrap();
        std::fs::canonicalize(dir).unwrap()
    }

    #[test]
    fn renders_gfm() {
        let html = render(
            "# Title\n\n| a |\n|---|\n| b |\n\n~~gone~~\n",
            &context(scratch("gfm")),
        );
        assert!(html.contains("<h1>Title</h1>"), "{html}");
        assert!(html.contains("<table>"), "{html}");
        assert!(html.contains("<del>gone</del>"), "{html}");
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
            "<style>body{display:none}</style>\n",
        );
        let html = render(source, &context(scratch("xss"))).to_ascii_lowercase();
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
        ] {
            assert!(!html.contains(needle), "{needle} survived in {html}");
        }
    }

    #[test]
    fn remote_and_data_images_are_dropped() {
        let html = render(
            "![a](https://example.com/a.png) ![b](http://example.com/b.png) \
             <img src=\"data:image/png;base64,AAAA\"> <img srcset=\"https://example.com/c.png 2x\">",
            &context(scratch("remote")),
        );
        assert!(!html.contains("example.com"), "{html}");
        assert!(!html.contains("data:"), "{html}");
    }

    #[test]
    fn local_images_are_rewritten_inside_the_root_only() {
        let dir = scratch("local");
        std::fs::write(dir.join("img").join("p q.png"), b"png").unwrap();
        let html = render(
            "![in](img/p%20q.png) ![raw](<img/p q.png>) <img src=\"img/p%20q.png\"> \
             ![out](../outside.png) ![missing](img/none.png)",
            &context(dir),
        );
        assert_eq!(
            html.matches("asset://test/img/p%20q.png").count(),
            3,
            "{html}"
        );
        assert!(!html.contains("outside.png"), "{html}");
        assert!(!html.contains("none.png"), "{html}");
    }

    #[test]
    fn links_keep_safe_targets_only() {
        let html = render(
            "[a](#intro) [b](https://example.com) [c](mailto:x@example.com) [d](other.md) \
             [e](http://example.com) [f](file:///etc/passwd)",
            &context(scratch("links")),
        );
        assert!(html.contains("href=\"#intro\""), "{html}");
        assert!(html.contains("href=\"https://example.com\""), "{html}");
        assert!(html.contains("href=\"mailto:x@example.com\""), "{html}");
        assert!(html.contains("href=\"other.md\""), "{html}");
        assert!(!html.contains("http://example.com"), "{html}");
        assert!(!html.contains("file:"), "{html}");
    }
}
