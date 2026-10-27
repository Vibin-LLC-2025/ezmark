//! The open document, the active root, and the buffering of open requests.
//!
//! Every way of opening a file (argv, drag-drop, a forwarded second launch, a
//! macOS open event) goes through [`request_open`]. Requests that arrive before
//! the frontend has signalled readiness are buffered here and replayed by
//! [`frontend_ready`] (plan section 6). Each open bumps the generation; a render
//! is committed only if no newer request started meanwhile (plan section 9).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::MAIN_WINDOW;
use crate::asset;

/// Files above this size are refused before parsing (plan section 9).
pub const MAX_DOCUMENT_BYTES: u64 = 50 * 1024 * 1024;

/// Event telling the frontend to fetch the current view.
pub const VIEW_CHANGED: &str = "ezmark://view-changed";

/// Where an open request came from. All four originate in Rust or the OS,
/// never in a document (plan section 7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum OpenOrigin {
    Argv,
    DragDrop,
    SecondInstance,
    /// macOS `RunEvent::Opened`: Finder, `open`, Dock drops.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    OpenedEvent,
}

/// What the frontend shows.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    pub generation: u64,
    #[serde(flatten)]
    pub content: ViewContent,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ViewContent {
    /// No file yet: the drop-a-file screen.
    Empty,
    /// A rendered document. `html` is mdcore output and already sanitized.
    Document {
        title: String,
        path: String,
        html: String,
    },
    /// The file could not be opened; `message` says why, naming the path.
    Error { title: String, message: String },
}

/// The directory a document's local references resolve against, for one generation.
#[derive(Debug, Clone)]
pub struct AssetScope {
    pub generation: u64,
    pub doc_dir: PathBuf,
    pub root: PathBuf,
}

#[derive(Debug, Default)]
pub struct Documents {
    inner: Mutex<Inner>,
}

#[derive(Debug)]
struct Inner {
    frontend_ready: bool,
    pending: Option<PathBuf>,
    generation: u64,
    view: View,
    scope: Option<AssetScope>,
    #[cfg(feature = "test-harness")]
    opens: Vec<crate::harness::OpenRecord>,
}

impl Default for Inner {
    fn default() -> Self {
        Self {
            frontend_ready: false,
            pending: None,
            generation: 0,
            view: View {
                generation: 0,
                content: ViewContent::Empty,
            },
            scope: None,
            #[cfg(feature = "test-harness")]
            opens: Vec::new(),
        }
    }
}

impl Documents {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        // A panic while holding the lock leaves plain data behind; keep going.
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn view(&self) -> View {
        self.lock().view.clone()
    }

    /// The asset scope, but only for the current generation.
    pub fn asset_scope(&self, generation: u64) -> Option<AssetScope> {
        self.lock()
            .scope
            .as_ref()
            .filter(|scope| scope.generation == generation)
            .cloned()
    }

    #[cfg(feature = "test-harness")]
    pub fn harness_opens(&self) -> Vec<crate::harness::OpenRecord> {
        self.lock().opens.clone()
    }
}

/// Open `path` now, or once the frontend is ready.
pub fn request_open<R: Runtime>(app: &AppHandle<R>, path: PathBuf, origin: OpenOrigin) {
    let docs = app.state::<Documents>();
    let mut inner = docs.lock();
    let buffered = !inner.frontend_ready;
    #[cfg(feature = "test-harness")]
    inner
        .opens
        .push(crate::harness::OpenRecord::new(&path, origin, buffered));
    #[cfg(not(feature = "test-harness"))]
    let _ = origin;
    if buffered {
        // Pending requests coalesce: only the latest one matters.
        inner.pending = Some(path);
        return;
    }
    drop(inner);
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || open_now(&app, &path));
}

/// Called once the frontend listens for view changes. Replays any buffered
/// request before returning, so the frontend's first fetch sees its result.
pub async fn frontend_ready<R: Runtime>(app: AppHandle<R>) {
    let pending = {
        let docs = app.state::<Documents>();
        let mut inner = docs.lock();
        inner.frontend_ready = true;
        inner.pending.take()
    };
    if let Some(path) = pending {
        let task = tauri::async_runtime::spawn_blocking(move || open_now(&app, &path));
        // A panic in a render has already been reported by the panic hook.
        let _ = task.await;
    }
}

fn open_now<R: Runtime>(app: &AppHandle<R>, path: &Path) {
    let docs = app.state::<Documents>();
    let generation = {
        let mut inner = docs.lock();
        inner.generation += 1;
        inner.generation
    };

    let (content, scope) = match load(path, generation) {
        Ok((content, scope)) => (content, Some(scope)),
        Err(message) => {
            eprintln!("ezmark: {message}");
            let content = ViewContent::Error {
                title: display_name(path),
                message,
            };
            (content, None)
        }
    };
    let title = match &content {
        ViewContent::Document { title, .. } | ViewContent::Error { title, .. } => title.clone(),
        ViewContent::Empty => String::new(),
    };

    {
        let mut inner = docs.lock();
        if inner.generation != generation {
            return; // superseded by a newer request
        }
        inner.view = View {
            generation,
            content,
        };
        inner.scope = scope;
    }

    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        let _ = window.set_title(&format!("{title} - ezmark"));
    }
    let _ = app.emit_to(MAIN_WINDOW, VIEW_CHANGED, generation);
}

fn load(path: &Path, generation: u64) -> Result<(ViewContent, AssetScope), String> {
    let shown = dunce::simplified(path).display();
    let canonical = std::fs::canonicalize(path).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => format!("File not found: {shown}"),
        _ => format!("Could not open {shown}: {error}"),
    })?;
    let metadata = std::fs::metadata(&canonical)
        .map_err(|error| format!("Could not open {shown}: {error}"))?;
    if metadata.is_dir() {
        return Err(format!("{shown} is a folder, not a markdown file."));
    }
    if metadata.len() > MAX_DOCUMENT_BYTES {
        return Err(format!(
            "{shown} is larger than {} MB, so ezmark will not open it.",
            MAX_DOCUMENT_BYTES / (1024 * 1024)
        ));
    }
    let bytes =
        std::fs::read(&canonical).map_err(|error| format!("Could not read {shown}: {error}"))?;
    let text = String::from_utf8_lossy(&bytes);
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);

    let doc_dir = canonical
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| format!("{shown} has no parent folder."))?;
    let root = find_root(&doc_dir);
    let ctx = mdcore::RenderContext {
        doc_dir: doc_dir.clone(),
        root: root.clone(),
        asset_url: Arc::new(move |raw: &str| asset::url(generation, raw)),
    };
    let html = mdcore::render(text, &ctx);

    let content = ViewContent::Document {
        title: display_name(&canonical),
        path: dunce::simplified(&canonical).display().to_string(),
        html,
    };
    let scope = AssetScope {
        generation,
        doc_dir,
        root,
    };
    Ok((content, scope))
}

/// The enclosing git repository root if there is one, else the document's own
/// directory (plan section 7).
fn find_root(doc_dir: &Path) -> PathBuf {
    doc_dir
        .ancestors()
        .find(|dir| dir.join(".git").exists())
        .unwrap_or(doc_dir)
        .to_path_buf()
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| dunce::simplified(path).display().to_string())
}
