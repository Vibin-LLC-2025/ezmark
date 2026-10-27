//! Security-harness plumbing (plan section 11). Compiled only with
//! `--features test-harness`; CI checks that none of it reaches a release build.
//!
//! The harness adds three things to the real app: a CSP-violation listener
//! (injected as an initialization script, so it never lives in `ui/`), a
//! command that injects raw HTML for the enforcement corpus, and a JSON
//! results channel on stdout. Everything else (CSP, sanitizer, ACL, asset
//! validation) is the production path. Behaviour is configured by environment
//! variables set by `tests/harness.rs`:
//!
//! - `EZMARK_HARNESS_ID`: suffix for the app identifier, so parallel test
//!   processes do not forward to each other through the single-instance plugin.
//! - `EZMARK_HARNESS_RENDERS`: document or error renders to wait for before
//!   reporting (default 1; 0 reports after the first render of any kind).
//! - `EZMARK_HARNESS_INJECT`: an HTML file to inject unsanitized after rendering.
//! - `EZMARK_HARNESS_PROBE_ASSET`: a raw reference to request directly from the
//!   asset handler, bypassing the renderer's own check.
//! - `EZMARK_HARNESS_MODE`: `report` (default, exit after reporting) or `hold`
//!   (stay open on top, for screenshots).
//! - `EZMARK_HARNESS_OUT`: also write the result to this file.
//! - `EZMARK_HARNESS_TIMEOUT_SECS`: give up and exit with code 3 (default 60).

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tauri::{AppHandle, Manager, Runtime, WebviewWindowBuilder};

use crate::documents::{Documents, OpenOrigin};

const CAPABILITY: &str = include_str!("../harness/capability.json");
const SCRIPT: &str = include_str!("../harness/harness.js");

pub const RESULT_PREFIX: &str = "EZMARK_HARNESS_RESULT ";
pub const EVENT_PREFIX: &str = "EZMARK_HARNESS_EVENT ";

/// Set if the ACL ever lets the denied probe through.
static DENIED_PROBE_EXECUTED: AtomicBool = AtomicBool::new(false);

/// The part of the configuration the injected script sees.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScriptConfig {
    renders: u32,
    inject: bool,
    probe_asset: Option<String>,
    hold: bool,
}

#[derive(Debug)]
struct Config {
    id: Option<String>,
    script: ScriptConfig,
    inject_path: Option<PathBuf>,
    out: Option<PathBuf>,
    timeout: Duration,
}

fn config() -> &'static Config {
    static CONFIG: OnceLock<Config> = OnceLock::new();
    CONFIG.get_or_init(|| {
        let var = |name: &str| std::env::var(name).ok().filter(|v| !v.is_empty());
        let inject_path = var("EZMARK_HARNESS_INJECT").map(PathBuf::from);
        Config {
            id: var("EZMARK_HARNESS_ID").map(|id| {
                id.chars()
                    .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
                    .collect()
            }),
            script: ScriptConfig {
                renders: var("EZMARK_HARNESS_RENDERS")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(1),
                inject: inject_path.is_some(),
                probe_asset: var("EZMARK_HARNESS_PROBE_ASSET"),
                hold: var("EZMARK_HARNESS_MODE").as_deref() == Some("hold"),
            },
            inject_path,
            out: var("EZMARK_HARNESS_OUT").map(PathBuf::from),
            timeout: Duration::from_secs(
                var("EZMARK_HARNESS_TIMEOUT_SECS")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(60),
            ),
        }
    })
}

/// One open request as the shell saw it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenRecord {
    path: String,
    origin: OpenOrigin,
    /// True if it arrived before the frontend was ready and was replayed.
    buffered: bool,
}

impl OpenRecord {
    pub fn new(path: &Path, origin: OpenOrigin, buffered: bool) -> Self {
        Self {
            path: path.display().to_string(),
            origin,
            buffered,
        }
    }
}

pub fn adjust_context<R: Runtime>(context: &mut tauri::Context<R>) {
    if let Some(id) = &config().id {
        let identifier = format!("{}.harness.{id}", context.config().identifier);
        context.config_mut().identifier = identifier;
    }
}

pub fn setup<R: Runtime>(app: &mut tauri::App<R>) -> tauri::Result<()> {
    app.add_capability(CAPABILITY)?;
    let timeout = config().timeout;
    std::thread::spawn(move || {
        std::thread::sleep(timeout);
        emit_result(&json!({ "timedOut": true }));
        std::process::exit(3);
    });
    Ok(())
}

pub fn configure_window<'a, R: Runtime, M: Manager<R>>(
    builder: WebviewWindowBuilder<'a, R, M>,
) -> WebviewWindowBuilder<'a, R, M> {
    let script_config = serde_json::to_string(&config().script).expect("harness config serializes");
    let data_dir = std::env::temp_dir()
        .join("ezmark-harness")
        .join(config().id.as_deref().unwrap_or("default"));
    builder
        .initialization_script(format!(
            "window.__EZMARK_HARNESS_CONFIG__ = {script_config};\n{SCRIPT}"
        ))
        .data_directory(data_dir)
}

/// The page reports a render (`rendered`) or its final findings (`final`).
#[tauri::command]
pub fn harness_report(app: AppHandle, kind: String, data: Value) {
    match kind.as_str() {
        "rendered" => {
            println!(
                "{EVENT_PREFIX}{}",
                json!({ "type": "rendered", "data": data })
            );
            let _ = std::io::stdout().flush();
        }
        "final" => {
            let documents = app.state::<Documents>();
            let result = json!({
                "timedOut": false,
                "page": data,
                "view": documents.view(),
                "opens": documents.harness_opens(),
                "deniedProbeExecuted": DENIED_PROBE_EXECUTED.load(Ordering::SeqCst),
            });
            emit_result(&result);
            if config().script.hold {
                if let Some(window) = app.get_webview_window(crate::MAIN_WINDOW) {
                    let _ = window.set_always_on_top(true);
                    let _ = window.set_focus();
                }
            } else {
                app.exit(0);
            }
        }
        other => eprintln!("ezmark harness: unknown report kind {other:?}"),
    }
}

/// Test-only: hand the enforcement fixture's raw HTML to the page.
#[tauri::command]
pub fn harness_inject() -> Result<String, String> {
    let path = config()
        .inject_path
        .as_ref()
        .ok_or("EZMARK_HARNESS_INJECT is not set")?;
    std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))
}

/// Enrolled in the ACL but granted to no capability. If this body ever runs,
/// the ACL failed, and the result says so.
#[tauri::command]
pub fn harness_denied_probe() {
    DENIED_PROBE_EXECUTED.store(true, Ordering::SeqCst);
}

fn emit_result(result: &Value) {
    let line = result.to_string();
    println!("{RESULT_PREFIX}{line}");
    let _ = std::io::stdout().flush();
    // Launches through `open` on macOS have no useful stdout, so also write a file.
    let fallback = std::env::temp_dir().join("ezmark-harness-last-result.json");
    for path in config().out.iter().chain(std::iter::once(&fallback)) {
        if let Err(error) = std::fs::write(path, &line) {
            eprintln!(
                "ezmark harness: could not write {}: {error}",
                path.display()
            );
        }
    }
}
