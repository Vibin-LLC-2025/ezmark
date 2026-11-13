//! The open document, the active root, and the buffering of open requests.
//!
//! Every way of opening a file (argv, drag-drop, a forwarded second launch, a
//! macOS open event) goes through [`request_open`]. Requests that arrive before
//! the frontend has signalled readiness are buffered here and replayed by
//! [`frontend_ready`] (plan section 6). Each request gets its generation when it
//! is accepted, in arrival order; a render is committed only if no newer request
//! was accepted meanwhile (plan section 9).
//!
//! Rendering never runs on the UI thread, and never fails silently: a file
//! that cannot be read, and a render that panics, both end in an error view.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::MAIN_WINDOW;
use crate::asset;

/// Files above this size are refused before parsing (plan section 9).
pub const MAX_DOCUMENT_BYTES: u64 = 50 * 1024 * 1024;

/// Event telling the frontend that a newer view exists. Payload: its generation.
pub const VIEW_CHANGED: &str = "ezmark://view-changed";

/// Event telling the frontend that a render has started. Payload: [`Loading`].
pub const LOADING: &str = "ezmark://loading";

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
    /// `notices` are plain sentences shown above it (a file that was not
    /// valid text, a file shown as plain text).
    Document {
        title: String,
        path: String,
        html: String,
        notices: Vec<String>,
    },
    /// The file could not be opened; `message` says why, naming the path.
    Error { title: String, message: String },
}

/// A render that has been accepted and has not been committed yet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Loading {
    pub generation: u64,
    /// The file's name, for the "still rendering" notice.
    pub title: String,
}

/// The reply to the frontend's readiness signal.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ready {
    pub view: View,
    /// A render in flight whose view will follow in a `view-changed` event.
    pub loading: Option<Loading>,
}

/// The directory a document's local references resolve against.
///
/// Asset URLs carry `id`. It changes only when the directory or the root
/// changes, so re-rendering the same document (live reload, M3) keeps every
/// image URL stable, while a request made for a previous scope is refused.
#[derive(Debug, Clone)]
pub struct AssetScope {
    pub id: u64,
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
    loading: Option<Loading>,
    scope: Option<AssetScope>,
    scopes_issued: u64,
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
            loading: None,
            scope: None,
            scopes_issued: 0,
            #[cfg(feature = "test-harness")]
            opens: Vec::new(),
        }
    }
}

impl Inner {
    /// Start the next generation for `path` and record it as loading.
    fn begin(&mut self, path: &Path) -> Loading {
        self.generation += 1;
        let loading = Loading {
            generation: self.generation,
            title: display_name(path),
        };
        self.loading = Some(loading.clone());
        loading
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

    /// The asset scope, but only if `id` is the current one.
    pub fn asset_scope(&self, id: u64) -> Option<AssetScope> {
        self.lock()
            .scope
            .as_ref()
            .filter(|scope| scope.id == id)
            .cloned()
    }

    /// The scope id a render of a document in `doc_dir` under `root` should
    /// put in its asset URLs: the current one if it covers the same directory
    /// and root, otherwise a fresh one that becomes current if the render is
    /// committed.
    fn scope_id(&self, doc_dir: &Path, root: &Path) -> u64 {
        let mut inner = self.lock();
        if let Some(scope) = &inner.scope
            && scope.doc_dir == doc_dir
            && scope.root == root
        {
            return scope.id;
        }
        inner.scopes_issued += 1;
        inner.scopes_issued
    }

    #[cfg(feature = "test-harness")]
    pub fn harness_opens(&self) -> Vec<crate::harness::OpenRecord> {
        self.lock().opens.clone()
    }

    /// Accept an open request. Its generation is assigned here, under the lock,
    /// so the latest request wins however the render workers are scheduled.
    /// Returns `None` if the request is buffered until the frontend is ready.
    fn accept(&self, path: &Path, origin: OpenOrigin) -> Option<Loading> {
        let mut inner = self.lock();
        let buffered = !inner.frontend_ready;
        #[cfg(feature = "test-harness")]
        inner
            .opens
            .push(crate::harness::OpenRecord::new(path, origin, buffered));
        #[cfg(not(feature = "test-harness"))]
        let _ = origin;
        if buffered {
            // Pending requests coalesce: only the latest one matters.
            inner.pending = Some(path.to_path_buf());
            return None;
        }
        Some(inner.begin(path))
    }

    /// Mark the frontend ready and take the buffered request, if any. Its
    /// generation is assigned under the same lock that flips readiness, so no
    /// request accepted afterwards can be ordered before it.
    ///
    /// The frontend may signal readiness again (the page was reloaded); it
    /// then gets the current view and whatever is still loading.
    fn mark_ready(&self) -> (Ready, Option<(PathBuf, u64)>) {
        let mut inner = self.lock();
        inner.frontend_ready = true;
        let replay = inner.pending.take().map(|path| {
            let generation = inner.begin(&path).generation;
            (path, generation)
        });
        let ready = Ready {
            view: inner.view.clone(),
            loading: inner.loading.clone(),
        };
        (ready, replay)
    }

    /// Commit a finished render, unless a newer request was accepted since.
    fn commit(&self, generation: u64, content: ViewContent, scope: Option<AssetScope>) -> bool {
        let mut inner = self.lock();
        if inner.generation != generation {
            return false;
        }
        inner.view = View {
            generation,
            content,
        };
        inner.scope = scope;
        inner.loading = None;
        true
    }
}

/// Open `path` now, or once the frontend is ready.
pub fn request_open<R: Runtime>(app: &AppHandle<R>, path: PathBuf, origin: OpenOrigin) {
    let Some(loading) = app.state::<Documents>().accept(&path, origin) else {
        return;
    };
    let _ = app.emit_to(MAIN_WINDOW, LOADING, &loading);
    spawn_open(app.clone(), path, loading.generation);
}

/// Called once the frontend listens for view changes. Starts the buffered
/// open, if any, and tells the frontend what to show meanwhile. It does not
/// wait for the render: a slow one must not leave the window blank.
pub fn frontend_ready<R: Runtime>(app: &AppHandle<R>) -> Ready {
    let (ready, replay) = app.state::<Documents>().mark_ready();
    if let Some((path, generation)) = replay {
        spawn_open(app.clone(), path, generation);
    }
    ready
}

fn spawn_open<R: Runtime>(app: AppHandle<R>, path: PathBuf, generation: u64) {
    tauri::async_runtime::spawn_blocking(move || open_now(&app, &path, generation));
}

fn open_now<R: Runtime>(app: &AppHandle<R>, path: &Path, generation: u64) {
    let documents = app.state::<Documents>();
    let (content, scope) = guarded(path, || {
        load(
            path,
            &|doc_dir, root| documents.scope_id(doc_dir, root),
            home_dir(),
        )
    });
    if !documents.commit(generation, content, scope) {
        return; // superseded by a newer request
    }

    update_title(app);
    let _ = app.emit_to(MAIN_WINDOW, VIEW_CHANGED, generation);
}

/// Run a load, turning both a failure and a panic into an error view. The
/// parser and sanitizer are third-party code fed hostile input; if one of
/// them panics, the reader must still be told that the file did not open.
fn guarded(
    path: &Path,
    load: impl FnOnce() -> Result<(ViewContent, AssetScope), String>,
) -> (ViewContent, Option<AssetScope>) {
    let message = match catch_unwind(AssertUnwindSafe(load)) {
        Ok(Ok((content, scope))) => return (content, Some(scope)),
        Ok(Err(message)) => message,
        // The panic hook has already written the details to stderr.
        Err(_) => format!(
            "ezmark hit an internal error while rendering {}. The file itself was not \
             changed. Please report this at {}/issues with the file, if you can share it.",
            dunce::simplified(path).display(),
            env!("CARGO_PKG_REPOSITORY"),
        ),
    };
    eprintln!("ezmark: {message}");
    let content = ViewContent::Error {
        title: display_name(path),
        message,
    };
    (content, None)
}

/// Title the window after the view that is current when this runs, not the
/// one just committed. Every commit schedules one of these after it, so the
/// last to run sees the latest commit, however two workers' commits and
/// title updates interleave.
fn update_title<R: Runtime>(app: &AppHandle<R>) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let title = window_title(&handle.state::<Documents>().view().content);
        if let Some(window) = handle.get_webview_window(MAIN_WINDOW) {
            let _ = window.set_title(&title);
        }
    });
}

fn window_title(content: &ViewContent) -> String {
    match content {
        ViewContent::Document { title, .. } | ViewContent::Error { title, .. } => {
            format!("{title} - ezmark")
        }
        ViewContent::Empty => "ezmark".to_owned(),
    }
}

/// Read, decode, and render `path`. `scope_id` supplies the id for the asset
/// URLs of a document in a given directory and root; `home` is the user's
/// canonical home directory, which bounds how far up a root may reach.
fn load(
    path: &Path,
    scope_id: &dyn Fn(&Path, &Path) -> u64,
    home: Option<&Path>,
) -> Result<(ViewContent, AssetScope), String> {
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
    let decoded = mdcore::decode(&bytes);

    let doc_dir = canonical
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| format!("{shown} has no parent folder."))?;
    let root = find_root(&doc_dir, home);
    let id = scope_id(&doc_dir, &root);
    let ctx = mdcore::RenderContext {
        doc_dir: doc_dir.clone(),
        root: root.clone(),
        asset_url: Arc::new(move |raw: &str| asset::url(id, raw)),
    };
    let rendered = mdcore::render(&decoded.text, &ctx);

    let mut notices = Vec::new();
    if decoded.lossy {
        notices.push(
            "This file is not valid UTF-8 or UTF-16 text. Characters that could not be \
             read are shown as \u{FFFD}."
                .to_owned(),
        );
    }
    if let Some(fallback) = rendered.fallback {
        let mut notice = format!(
            "This file nests more than {} levels deep, so it is shown as plain text \
             instead of being rendered.",
            mdcore::MAX_DEPTH
        );
        if fallback.truncated {
            notice.push_str(&format!(
                " Only the first {} MB is shown.",
                mdcore::PLAIN_TEXT_LIMIT / (1024 * 1024)
            ));
        }
        notices.push(notice);
    }

    let content = ViewContent::Document {
        title: display_name(&canonical),
        path: dunce::simplified(&canonical).display().to_string(),
        html: rendered.html,
        notices,
    };
    let scope = AssetScope { id, doc_dir, root };
    Ok((content, scope))
}

/// The enclosing git repository root if there is one, else the document's own
/// directory (plan section 7).
///
/// A repository root that is a filesystem root, or the user's home directory
/// or anything above it, is not adopted: some people keep their whole home
/// directory in git, and one stray `.git` must not make every file they own
/// readable by any document they open.
fn find_root(doc_dir: &Path, home: Option<&Path>) -> PathBuf {
    doc_dir
        .ancestors()
        .find(|dir| dir.join(".git").exists())
        .filter(|root| !is_too_broad(root, home))
        .unwrap_or(doc_dir)
        .to_path_buf()
}

fn is_too_broad(root: &Path, home: Option<&Path>) -> bool {
    root.parent().is_none() || home.is_some_and(|home| home.starts_with(root))
}

/// The user's home directory in canonical form, so it compares with the
/// canonical paths `load` works with.
fn home_dir() -> Option<&'static Path> {
    static HOME: OnceLock<Option<PathBuf>> = OnceLock::new();
    HOME.get_or_init(|| std::fs::canonicalize(std::env::home_dir()?).ok())
        .as_deref()
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| dunce::simplified(path).display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document(name: &str) -> ViewContent {
        ViewContent::Document {
            title: name.to_owned(),
            path: name.to_owned(),
            html: String::new(),
            notices: Vec::new(),
        }
    }

    fn shown(docs: &Documents) -> (u64, Option<String>) {
        let view = docs.view();
        let title = match view.content {
            ViewContent::Document { title, .. } => Some(title),
            _ => None,
        };
        (view.generation, title)
    }

    fn accept(docs: &Documents, name: &str) -> u64 {
        docs.accept(Path::new(name), OpenOrigin::DragDrop)
            .expect("the frontend is ready")
            .generation
    }

    /// A scratch directory that is removed when the test ends.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir()
                .join(format!("ezmark-documents-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self(std::fs::canonicalize(dir).unwrap())
        }

        fn file(&self, relative: &str, bytes: &[u8]) -> PathBuf {
            let path = self.0.join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, bytes).unwrap();
            path
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn load_plain(path: &Path) -> Result<(ViewContent, AssetScope), String> {
        load(path, &|_, _| 7, None)
    }

    fn loaded(path: &Path) -> (String, Vec<String>) {
        match load_plain(path).expect("loads").0 {
            ViewContent::Document { html, notices, .. } => (html, notices),
            other => panic!("not a document: {other:?}"),
        }
    }

    #[test]
    fn buffered_requests_coalesce_and_replay_before_later_ones() {
        let docs = Documents::default();
        assert_eq!(docs.accept(Path::new("a.md"), OpenOrigin::Argv), None);
        assert_eq!(docs.accept(Path::new("b.md"), OpenOrigin::DragDrop), None);

        let (ready, replay) = docs.mark_ready();
        let (replayed, first) = replay.expect("a buffered request");
        assert_eq!(replayed, Path::new("b.md"), "only the latest is kept");
        assert_eq!(
            ready.loading,
            Some(Loading {
                generation: first,
                title: "b.md".to_owned()
            }),
            "the frontend is told what is on its way"
        );
        let later = accept(&docs, "c.md");
        assert!(later > first, "{later} is not after {first}");
        assert!(docs.mark_ready().1.is_none(), "replayed once");
    }

    #[test]
    fn the_latest_request_wins_whichever_render_finishes_first() {
        let docs = Documents::default();
        docs.mark_ready();
        let older = accept(&docs, "a.md");
        let newer = accept(&docs, "b.md");

        assert!(docs.commit(newer, document("b.md"), None));
        assert!(!docs.commit(older, document("a.md"), None));
        assert_eq!(shown(&docs), (newer, Some("b.md".to_owned())));
    }

    #[test]
    fn a_render_superseded_before_it_finishes_is_discarded() {
        let docs = Documents::default();
        docs.mark_ready();
        let older = accept(&docs, "a.md");
        let newer = accept(&docs, "b.md");

        assert!(!docs.commit(older, document("a.md"), None));
        assert_eq!(shown(&docs), (0, None), "still the empty view");
        assert!(docs.commit(newer, document("b.md"), None));
        assert_eq!(shown(&docs), (newer, Some("b.md".to_owned())));
    }

    #[test]
    fn loading_lasts_from_acceptance_to_commit() {
        let docs = Documents::default();
        assert_eq!(docs.mark_ready().0.loading, None);
        let older = accept(&docs, "a.md");
        let newer = accept(&docs, "b.md");
        let loading = |docs: &Documents| docs.mark_ready().0.loading.map(|l| l.generation);

        assert_eq!(loading(&docs), Some(newer));
        docs.commit(older, document("a.md"), None);
        assert_eq!(
            loading(&docs),
            Some(newer),
            "a superseded commit changes nothing"
        );
        docs.commit(newer, document("b.md"), None);
        assert_eq!(loading(&docs), None);
        // A page reload asks again and gets the committed view back.
        assert_eq!(docs.mark_ready().0.view.generation, newer);
    }

    #[test]
    fn asset_scope_ids_change_only_with_the_directory_or_root() {
        let docs = Documents::default();
        docs.mark_ready();
        let (a, b) = (Path::new("a"), Path::new("b"));
        let scope = |id, dir: &Path| AssetScope {
            id,
            doc_dir: dir.to_path_buf(),
            root: dir.to_path_buf(),
        };

        let first = docs.scope_id(a, a);
        docs.commit(
            accept(&docs, "a/one.md"),
            document("one.md"),
            Some(scope(first, a)),
        );
        assert_eq!(
            docs.scope_id(a, a),
            first,
            "a re-render keeps its image URLs"
        );
        assert!(docs.asset_scope(first).is_some());

        let second = docs.scope_id(b, b);
        assert_ne!(second, first);
        assert_ne!(
            docs.scope_id(a, b),
            first,
            "a different root is a different scope"
        );
        assert!(
            docs.asset_scope(second).is_none(),
            "not current until committed"
        );
        docs.commit(
            accept(&docs, "b/two.md"),
            document("two.md"),
            Some(scope(second, b)),
        );
        assert!(
            docs.asset_scope(first).is_none(),
            "the old scope is refused"
        );
        assert!(docs.asset_scope(second).is_some());

        // An error view has no scope, and the next document gets a new one.
        docs.commit(accept(&docs, "missing.md"), ViewContent::Empty, None);
        assert!(docs.asset_scope(second).is_none());
        assert!(docs.scope_id(b, b) > second);
    }

    #[test]
    fn a_panicking_render_becomes_an_error_view() {
        let path = Path::new("dir").join("boom.md");
        let (content, scope) = guarded(&path, || panic!("simulated parser panic"));
        assert!(scope.is_none());
        let ViewContent::Error { title, message } = content else {
            panic!("not an error view: {content:?}");
        };
        assert_eq!(title, "boom.md");
        assert!(message.contains("internal error"), "{message}");
        assert!(message.contains("boom.md"), "{message}");
        assert!(message.contains("/issues"), "{message}");

        let (content, _) = guarded(&path, || Err("File not found: x".to_owned()));
        assert!(
            matches!(content, ViewContent::Error { message, .. } if message == "File not found: x")
        );
    }

    #[test]
    fn load_renders_a_document_and_scopes_it_to_its_directory() {
        let dir = Scratch::new("load");
        let path = dir.file("docs/read me.md", "# Héllo\n".as_bytes());
        let (content, scope) = load_plain(&path).unwrap();
        let ViewContent::Document {
            title,
            path: shown,
            html,
            notices,
        } = content
        else {
            panic!("not a document");
        };
        assert_eq!(title, "read me.md");
        assert!(
            shown.ends_with("read me.md") && !shown.starts_with(r"\\?\"),
            "{shown}"
        );
        assert!(html.contains("Héllo"), "{html}");
        assert!(notices.is_empty(), "{notices:?}");
        assert_eq!(scope.id, 7);
        assert_eq!(scope.doc_dir, dir.0.join("docs"));
        assert_eq!(
            scope.root,
            dir.0.join("docs"),
            "no repository: the file's folder"
        );
    }

    #[test]
    fn load_decodes_byte_order_marks_and_utf16() {
        let dir = Scratch::new("encodings");
        let utf8 = dir.file("utf8.md", b"\xEF\xBB\xBF# Marked\n");
        let (html, notices) = loaded(&utf8);
        assert!(
            html.contains(">Marked<") && !html.contains('\u{feff}'),
            "{html}"
        );
        assert!(notices.is_empty());

        let mut bytes = vec![0xFF, 0xFE];
        bytes.extend("# Wïde ✓\n".encode_utf16().flat_map(u16::to_le_bytes));
        let (html, notices) = loaded(&dir.file("utf16.md", &bytes));
        assert!(html.contains("Wïde ✓"), "{html}");
        assert!(notices.is_empty(), "{notices:?}");
    }

    #[test]
    fn load_says_when_text_could_not_be_decoded() {
        let dir = Scratch::new("lossy");
        let (html, notices) = loaded(&dir.file("latin1.md", b"caf\xE9\n"));
        assert!(html.contains("caf\u{FFFD}"), "{html}");
        assert_eq!(notices.len(), 1);
        assert!(notices[0].contains("not valid UTF-8"), "{notices:?}");
    }

    #[test]
    fn load_says_when_a_file_is_shown_as_plain_text() {
        let dir = Scratch::new("deep");
        let source = format!("{}deep\n", ">".repeat(mdcore::MAX_DEPTH + 1));
        let (html, notices) = loaded(&dir.file("deep.md", source.as_bytes()));
        assert!(
            html.starts_with("<pre class=\"plain-text\">"),
            "{}",
            &html[..40]
        );
        assert_eq!(notices.len(), 1);
        assert!(notices[0].contains("plain text"), "{notices:?}");
        assert!(!notices[0].contains("first"), "not truncated: {notices:?}");
    }

    #[test]
    fn load_refuses_what_it_cannot_show_and_says_why() {
        let dir = Scratch::new("refusals");
        let missing = load_plain(&dir.0.join("no such.md")).unwrap_err();
        assert!(missing.starts_with("File not found:") && missing.contains("no such.md"));

        let folder = load_plain(&dir.0).unwrap_err();
        assert!(folder.contains("is a folder"), "{folder}");

        // A sparse file: over the limit without writing 50 MB.
        let huge = dir.0.join("huge.md");
        std::fs::File::create(&huge)
            .unwrap()
            .set_len(MAX_DOCUMENT_BYTES + 1)
            .unwrap();
        let too_large = load_plain(&huge).unwrap_err();
        assert!(too_large.contains("larger than 50 MB"), "{too_large}");

        let at_limit = dir.0.join("empty.md");
        std::fs::File::create(&at_limit).unwrap();
        assert!(load_plain(&at_limit).is_ok(), "an empty file is a document");
    }

    #[test]
    fn the_root_is_the_enclosing_repository() {
        let dir = Scratch::new("root");
        let doc = dir.file("repo/docs/guide/a.md", b"x");
        let doc_dir = doc.parent().unwrap();
        assert_eq!(find_root(doc_dir, None), doc_dir, "no .git anywhere");

        // `.git` is a directory in a clone and a file in a worktree or submodule.
        dir.file("repo/.git/HEAD", b"ref");
        assert_eq!(find_root(doc_dir, None), dir.0.join("repo"));
        dir.file("repo/docs/.git", b"gitdir: elsewhere");
        assert_eq!(
            find_root(doc_dir, None),
            dir.0.join("repo/docs"),
            "the nearest wins"
        );
    }

    #[test]
    fn a_repository_at_or_above_home_is_not_adopted_as_the_root() {
        let dir = Scratch::new("home");
        let home = dir.0.join("home/user");
        let doc = dir.file("home/user/notes/a.md", b"x");
        let doc_dir = doc.parent().unwrap();

        dir.file("home/user/.git/HEAD", b"ref");
        assert_eq!(
            find_root(doc_dir, Some(&home)),
            doc_dir,
            "home itself is in git"
        );
        assert_eq!(find_root(doc_dir, None), home, "without a home it would be");

        std::fs::remove_dir_all(home.join(".git")).unwrap();
        dir.file("home/.git/HEAD", b"ref");
        assert_eq!(
            find_root(doc_dir, Some(&home)),
            doc_dir,
            "a parent of home is in git"
        );

        // A repository below home is still adopted.
        dir.file("home/user/notes/.git/HEAD", b"ref");
        let deeper = dir.file("home/user/notes/sub/b.md", b"x");
        assert_eq!(find_root(deeper.parent().unwrap(), Some(&home)), doc_dir);
    }

    #[test]
    fn a_filesystem_root_is_never_the_root() {
        let top = std::env::temp_dir()
            .ancestors()
            .last()
            .unwrap()
            .to_path_buf();
        assert!(is_too_broad(&top, None), "{}", top.display());
        assert!(!is_too_broad(&std::env::temp_dir(), None));
    }
}
