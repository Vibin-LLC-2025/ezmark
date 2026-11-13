//! ezmark's Tauri shell: windows, open events, IPC commands, asset protocol.
//! Thin by design; the markdown engine is the `mdcore` crate.

// Release builds are GUI-subsystem on Windows so no console flashes up. Debug
// and test-harness builds keep the console for diagnostics and harness output.
#![cfg_attr(
    all(not(debug_assertions), not(feature = "test-harness")),
    windows_subsystem = "windows"
)]

mod asset;
mod commands;
mod documents;
#[cfg(feature = "test-harness")]
mod harness;
mod launch;

use std::path::Path;

use tauri::{
    AppHandle, DragDropEvent, Manager, RunEvent, Runtime, Url, WebviewUrl, WebviewWindowBuilder,
    Window, WindowEvent,
};

use documents::OpenOrigin;
use launch::Launch;

/// The one window in M0. Capabilities and the asset handler are keyed to it.
pub const MAIN_WINDOW: &str = "main";

/// Command-line arguments for WebView2: the network lock.
///
/// The CSP stops a page's resource loads, but two things reach the network
/// stack without asking it. A navigation the Rust guard denies has already
/// sent its request by the time WebView2 reports it, and a `preconnect` hint
/// opens a connection no CSP directive governs. Both need markup that the
/// sanitizer removes, but a second layer should not depend on the first, so
/// the webview's network stack is given nowhere to go:
///
/// - every request is routed to a proxy on the loopback discard port, where
///   nothing listens, with Chromium's built-in exception for loopback
///   addresses removed, so `localhost` services are out of reach too;
/// - no host name is ever resolved, so a `dns-prefetch` hint cannot leak one;
/// - the browser's own background requests are switched off.
///
/// The app's own schemes (`tauri`, `ipc`, `ezasset`) are answered inside the
/// process before the network stack is consulted, so they are unaffected.
///
/// Setting any argument replaces wry's defaults, so its `--disable-features`
/// list (no mini menu, no SmartScreen lookups) is repeated first.
///
/// There is no macOS counterpart. WKWebView takes no arguments; giving its
/// data store the same dead proxy was tried on 2026-11-13 and made no
/// reliable difference to the connections it opens (ADR 0003).
#[cfg(windows)]
const WEBVIEW2_ARGUMENTS: &str = concat!(
    "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection",
    " --proxy-server=http://127.0.0.1:9",
    " --proxy-bypass-list=<-loopback>",
    " --host-resolver-rules=\"MAP * ~NOTFOUND\"",
    " --dns-prefetch-disable",
    " --disable-background-networking",
    " --disable-component-update",
    " --disable-domain-reliability",
    " --no-pings",
);

fn main() {
    let cwd = std::env::current_dir().unwrap_or_default();
    let file = match launch::parse(std::env::args_os().skip(1), &cwd) {
        Launch::Open(file) => file,
        Launch::Help => return launch::tell(launch::HELP, false),
        Launch::Version => return launch::tell(&launch::version(), false),
        Launch::Invalid(problem) => {
            launch::tell(&format!("ezmark: {problem}\n\n{}", launch::HELP), true);
            std::process::exit(2);
        }
    };

    #[cfg_attr(not(feature = "test-harness"), allow(unused_mut))]
    let mut context = tauri::generate_context!();
    #[cfg(feature = "test-harness")]
    harness::adjust_context(&mut context);

    let built = tauri::Builder::default()
        // Registered first, as the plugin requires. A second launch forwards
        // its argv and working directory here and exits. It has already
        // refused arguments that make no sense, in its own process.
        .plugin(tauri_plugin_single_instance::init(|app, argv, cwd| {
            focus_main_window(app);
            let launch = launch::parse(argv.into_iter().skip(1).map(Into::into), Path::new(&cwd));
            if let Launch::Open(Some(path)) = launch {
                documents::request_open(app, path, OpenOrigin::SecondInstance);
            }
        }))
        .manage(documents::Documents::default())
        .register_asynchronous_uri_scheme_protocol(asset::SCHEME, asset::handle)
        .invoke_handler(tauri::generate_handler![
            commands::frontend_ready,
            commands::current_view,
            #[cfg(feature = "test-harness")]
            harness::harness_report,
            #[cfg(feature = "test-harness")]
            harness::harness_inject,
            #[cfg(feature = "test-harness")]
            harness::harness_denied_probe,
        ])
        .on_window_event(on_window_event)
        .setup(move |app| {
            #[cfg(feature = "test-harness")]
            harness::setup(app)?;
            create_main_window(app)?;
            if let Some(path) = file {
                documents::request_open(app.handle(), path, OpenOrigin::Argv);
            }
            Ok(())
        })
        .build(context);

    match built {
        Ok(app) => app.run(on_run_event),
        Err(error) => {
            // A release build has no console; say it where it will be seen.
            launch::tell(&launch::startup_failure(&error.to_string()), true);
            std::process::exit(1);
        }
    }
}

fn create_main_window<R: Runtime>(app: &tauri::App<R>) -> tauri::Result<()> {
    let builder = WebviewWindowBuilder::new(app, MAIN_WINDOW, WebviewUrl::App("index.html".into()))
        .title("ezmark")
        .inner_size(920.0, 820.0)
        .min_inner_size(360.0, 300.0)
        // The webview never leaves the app page (plan section 7, "Navigation").
        .on_navigation(|url| {
            let allowed = is_app_url(url);
            #[cfg(feature = "test-harness")]
            harness::record_navigation(url, allowed);
            allowed
        })
        // A viewer has nothing to download. This closes the webview's own
        // "Save link as" and "Save image as" paths, which would otherwise
        // fetch a URL the document chose.
        .on_download(|_webview, _event| false);
    #[cfg(windows)]
    let builder = if network_lock() {
        builder.additional_browser_args(WEBVIEW2_ARGUMENTS)
    } else {
        builder
    };
    #[cfg(feature = "test-harness")]
    let builder = harness::configure_window(builder);
    builder.build()?;
    Ok(())
}

/// Always on, except that the test harness can switch it off to prove what
/// the CSP stops on its own.
#[cfg(windows)]
fn network_lock() -> bool {
    #[cfg(feature = "test-harness")]
    let locked = harness::network_lock();
    #[cfg(not(feature = "test-harness"))]
    let locked = true;
    locked
}

/// The bundled UI's origin: `tauri://localhost` on macOS,
/// `http(s)://tauri.localhost` on Windows.
fn is_app_url(url: &Url) -> bool {
    match url.scheme() {
        "tauri" => url.host_str() == Some("localhost"),
        "http" | "https" => url.host_str() == Some("tauri.localhost"),
        _ => false,
    }
}

fn focus_main_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn on_window_event<R: Runtime>(window: &Window<R>, event: &WindowEvent) {
    if let WindowEvent::DragDrop(DragDropEvent::Drop { paths, .. }) = event
        && let Some(path) = paths.first()
    {
        documents::request_open(window.app_handle(), path.clone(), OpenOrigin::DragDrop);
    }
}

fn on_run_event<R: Runtime>(app: &AppHandle<R>, event: RunEvent) {
    // Finder, `open`, and Dock drops arrive here on macOS. On a cold start
    // they can arrive before the webview is ready; `request_open` buffers them.
    #[cfg(target_os = "macos")]
    if let RunEvent::Opened { urls } = event {
        for url in urls {
            if let Ok(path) = url.to_file_path() {
                documents::request_open(app, path, OpenOrigin::OpenedEvent);
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (app, event);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_app_origin_is_navigable() {
        for ok in [
            "tauri://localhost/index.html",
            "http://tauri.localhost/index.html#x",
            "https://tauri.localhost/",
        ] {
            assert!(is_app_url(&Url::parse(ok).unwrap()), "{ok}");
        }
        for bad in [
            "https://example.com/",
            "http://tauri.localhost.example.com/",
            "http://localhost/",
            "tauri://example.com/",
            "file:///C:/x.md",
            "ezasset://localhost/1/00",
            "about:blank",
            "javascript:alert(1)",
            "data:text/html,x",
        ] {
            assert!(!is_app_url(&Url::parse(bad).unwrap()), "{bad}");
        }
    }
}
