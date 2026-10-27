//! The `ezasset` protocol: serves a document's local images (plan section 7).
//!
//! URLs carry the document generation and the document's raw reference,
//! hex-encoded so the webview cannot normalise `..` segments or add a second
//! layer of percent-encoding. The handler hex-decodes that transport layer and
//! then validates the raw reference itself through mdcore's shared validator,
//! independently of the check made at render time.

use std::path::Path;

use tauri::http::{Response, StatusCode, header};
use tauri::{AppHandle, Manager, Runtime, UriSchemeContext, UriSchemeResponder};

use crate::MAIN_WINDOW;
use crate::documents::{Documents, MAX_DOCUMENT_BYTES};

pub const SCHEME: &str = "ezasset";

/// How the webview addresses the scheme. Windows serves custom schemes over
/// `http://<scheme>.localhost`; macOS uses the scheme directly. The CSP
/// `img-src` lists both.
#[cfg(windows)]
const BASE: &str = "http://ezasset.localhost";
#[cfg(not(windows))]
const BASE: &str = "ezasset://localhost";

/// The URL for a raw reference made by the document of `generation`.
pub fn url(generation: u64, raw: &str) -> String {
    let mut out = format!("{BASE}/{generation}/");
    for byte in raw.bytes() {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

pub fn handle<R: Runtime>(
    ctx: UriSchemeContext<'_, R>,
    request: tauri::http::Request<Vec<u8>>,
    responder: UriSchemeResponder,
) {
    let app = ctx.app_handle().clone();
    let from_main = ctx.webview_label() == MAIN_WINDOW;
    let path = request.uri().path().to_owned();
    tauri::async_runtime::spawn_blocking(move || {
        let response = if from_main {
            respond(&app, &path)
        } else {
            status(StatusCode::FORBIDDEN)
        };
        responder.respond(response);
    });
}

fn respond<R: Runtime>(app: &AppHandle<R>, path: &str) -> Response<Vec<u8>> {
    let Some((generation, raw)) = parse_path(path) else {
        return status(StatusCode::BAD_REQUEST);
    };
    let Some(scope) = app.state::<Documents>().asset_scope(generation) else {
        // Stale generation, or no document open: refuse.
        return status(StatusCode::GONE);
    };
    let target =
        match mdcore::validate_reference(&raw, &scope.doc_dir, &scope.root, &mdcore::RealFs) {
            Ok(target) => target,
            Err(mdcore::Rejection::NotFound) => return status(StatusCode::NOT_FOUND),
            Err(_) => return status(StatusCode::FORBIDDEN),
        };
    let Some(mime) = image_mime(&target) else {
        return status(StatusCode::UNSUPPORTED_MEDIA_TYPE);
    };
    match std::fs::metadata(&target) {
        Ok(meta) if !meta.is_file() => return status(StatusCode::NOT_FOUND),
        Ok(meta) if meta.len() > MAX_DOCUMENT_BYTES => {
            return status(StatusCode::PAYLOAD_TOO_LARGE);
        }
        Ok(_) => {}
        Err(_) => return status(StatusCode::NOT_FOUND),
    }
    let Ok(bytes) = std::fs::read(&target) else {
        return status(StatusCode::NOT_FOUND);
    };
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime)
        .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
        .header(header::CACHE_CONTROL, "no-store")
        .body(bytes)
        .unwrap_or_else(|_| status(StatusCode::INTERNAL_SERVER_ERROR))
}

/// `/<generation>/<hex>` to `(generation, raw reference)`.
fn parse_path(path: &str) -> Option<(u64, String)> {
    let (generation, hex) = path.strip_prefix('/')?.split_once('/')?;
    let generation = generation.parse().ok()?;
    if hex.is_empty() || hex.len() % 2 != 0 {
        return None;
    }
    let bytes = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok())
        .collect::<Option<Vec<u8>>>()?;
    Some((generation, String::from_utf8(bytes).ok()?))
}

/// Only image types are ever served.
fn image_mime(path: &Path) -> Option<&'static str> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    Some(match extension.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        _ => return None,
    })
}

fn status(code: StatusCode) -> Response<Vec<u8>> {
    let mut response = Response::new(Vec::new());
    *response.status_mut() = code;
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_round_trips_through_the_path_parser() {
        for raw in [
            "img/a.png",
            "../x.png",
            "my%20pic.png",
            "ünï ✓.png",
            "%5C%5Cserver",
        ] {
            let url = url(7, raw);
            let path = url.strip_prefix(BASE).unwrap();
            assert_eq!(parse_path(path), Some((7, raw.to_owned())), "{raw}");
        }
    }

    #[test]
    fn malformed_paths_are_refused() {
        for path in ["", "/", "/7", "/7/", "/x/6162", "/7/616", "/7/zz", "/7/ff"] {
            assert_eq!(parse_path(path), None, "{path}");
        }
    }

    #[test]
    fn only_images_are_served() {
        assert_eq!(image_mime(Path::new("a.PNG")), Some("image/png"));
        assert_eq!(image_mime(Path::new("a.md")), None);
        assert_eq!(image_mime(Path::new("a.html")), None);
        assert_eq!(image_mime(Path::new("noext")), None);
    }
}
