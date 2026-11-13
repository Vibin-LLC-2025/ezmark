//! The `ezasset` protocol: serves a document's local images (plan section 7).
//!
//! URLs carry the id of the asset scope (the document's directory and root)
//! and the document's raw reference, hex-encoded so the webview cannot
//! normalise `..` segments or add a second layer of percent-encoding. The
//! handler hex-decodes that transport layer and then validates the raw
//! reference itself through mdcore's shared validator, independently of the
//! check made at render time.

use std::path::Path;
use std::sync::{Condvar, Mutex};

use tauri::http::{Response, StatusCode, header};
use tauri::{AppHandle, Manager, Runtime, UriSchemeContext, UriSchemeResponder};

use crate::MAIN_WINDOW;
use crate::documents::Documents;

pub const SCHEME: &str = "ezasset";

/// How the webview addresses the scheme. Windows serves custom schemes over
/// `http://<scheme>.localhost`; macOS uses the scheme directly. The CSP
/// `img-src` lists both.
#[cfg(windows)]
const BASE: &str = "http://ezasset.localhost";
#[cfg(not(windows))]
const BASE: &str = "ezasset://localhost";

/// Images above this size are refused. Each one is read into memory whole.
pub const MAX_IMAGE_BYTES: u64 = 20 * 1024 * 1024;

/// How many images may be in memory at once on their way to the webview. A
/// folder of large images, or one large image referenced under many
/// spellings, then costs at most this many times [`MAX_IMAGE_BYTES`].
const MAX_CONCURRENT_READS: usize = 4;

static READS: ReadGate = ReadGate::new(MAX_CONCURRENT_READS);

/// The URL for a raw reference made by a document in asset scope `scope`.
pub fn url(scope: u64, raw: &str) -> String {
    format!("{BASE}/{scope}/{}", mdcore::hex::encode(raw))
}

/// A counting gate: `enter` blocks while the limit is reached.
#[derive(Debug)]
struct ReadGate {
    free: Mutex<usize>,
    released: Condvar,
}

/// Held while one image is in memory; dropping it lets the next one in.
#[derive(Debug)]
struct ReadPermit<'a>(&'a ReadGate);

impl ReadGate {
    const fn new(limit: usize) -> Self {
        Self {
            free: Mutex::new(limit),
            released: Condvar::new(),
        }
    }

    fn enter(&self) -> ReadPermit<'_> {
        // The count is plain data, so a poisoned lock is still usable.
        let mut free = self.free.lock().unwrap_or_else(|p| p.into_inner());
        while *free == 0 {
            free = self.released.wait(free).unwrap_or_else(|p| p.into_inner());
        }
        *free -= 1;
        ReadPermit(self)
    }
}

impl Drop for ReadPermit<'_> {
    fn drop(&mut self) {
        *self.0.free.lock().unwrap_or_else(|p| p.into_inner()) += 1;
        self.0.released.notify_one();
    }
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
        if !from_main {
            return responder.respond(status(StatusCode::FORBIDDEN));
        }
        let permit = READS.enter();
        let response = respond(&app, &path);
        // The webview takes responses on the main thread. Hand this one over
        // there, and keep the permit until it has been taken: releasing it
        // here would let any number of images queue up behind a busy UI.
        let _ = app.run_on_main_thread(move || {
            responder.respond(response);
            drop(permit);
        });
    });
}

fn respond<R: Runtime>(app: &AppHandle<R>, path: &str) -> Response<Vec<u8>> {
    let Some((scope, raw)) = parse_path(path) else {
        return status(StatusCode::BAD_REQUEST);
    };
    let Some(scope) = app.state::<Documents>().asset_scope(scope) else {
        // A previous document's scope, or no document open: refuse.
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
        Ok(meta) if meta.len() > MAX_IMAGE_BYTES => {
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

/// `/<scope>/<hex>` to `(scope id, raw reference)`.
fn parse_path(path: &str) -> Option<(u64, String)> {
    let (scope, hex) = path.strip_prefix('/')?.split_once('/')?;
    if scope.is_empty() || !scope.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Some((scope.parse().ok()?, mdcore::hex::decode(hex)?))
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
        for path in [
            "",
            "/",
            "/7",
            "/7/",
            "/x/6162",
            "/+7/6162",
            "/7/616",
            "/7/zz",
            "/7/ff",
            "/7/+a",
            "/99999999999999999999/6162",
        ] {
            assert_eq!(parse_path(path), None, "{path}");
        }
    }

    #[test]
    fn the_read_gate_never_admits_more_than_its_limit() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        static GATE: ReadGate = ReadGate::new(2);
        static INSIDE: AtomicUsize = AtomicUsize::new(0);
        static MOST: AtomicUsize = AtomicUsize::new(0);

        let workers: Vec<_> = (0..8)
            .map(|_| {
                std::thread::spawn(|| {
                    for _ in 0..50 {
                        let _permit = GATE.enter();
                        let inside = INSIDE.fetch_add(1, Ordering::SeqCst) + 1;
                        MOST.fetch_max(inside, Ordering::SeqCst);
                        std::thread::yield_now();
                        INSIDE.fetch_sub(1, Ordering::SeqCst);
                    }
                })
            })
            .collect();
        for worker in workers {
            worker.join().unwrap();
        }
        assert!(
            MOST.load(Ordering::SeqCst) <= 2,
            "more than the limit got in"
        );
        assert_eq!(*GATE.free.lock().unwrap(), 2, "every permit came back");
    }

    #[test]
    fn only_images_are_served() {
        assert_eq!(image_mime(Path::new("a.PNG")), Some("image/png"));
        assert_eq!(image_mime(Path::new("a.md")), None);
        assert_eq!(image_mime(Path::new("a.html")), None);
        assert_eq!(image_mime(Path::new("noext")), None);
    }
}
