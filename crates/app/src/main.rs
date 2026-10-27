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

/// The one window in M0. Capabilities and the asset handler are keyed to it.
pub const MAIN_WINDOW: &str = "main";

fn main() {
    let cwd = std::env::current_dir().unwrap_or_default();
    let launch = launch::parse(std::env::args_os().skip(1), &cwd);

    #[cfg_attr(not(feature = "test-harness"), allow(unused_mut))]
    let mut context = tauri::generate_context!();
    #[cfg(feature = "test-harness")]
    harness::adjust_context(&mut context);

    let app = tauri::Builder::default()
        // Registered first, as the plugin requires. A second launch forwards
        // its argv and working directory here and exits.
        .plugin(tauri_plugin_single_instance::init(|app, argv, cwd| {
            focus_main_window(app);
            let launch = launch::parse(argv.into_iter().skip(1).map(Into::into), Path::new(&cwd));
            if let Some(path) = launch.path {
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
            if let Some(path) = launch.path {
                documents::request_open(app.handle(), path, OpenOrigin::Argv);
            }
            Ok(())
        })
        .build(context)
        .expect("ezmark failed to start");

    app.run(on_run_event);
}

fn create_main_window<R: Runtime>(app: &tauri::App<R>) -> tauri::Result<()> {
    let builder = WebviewWindowBuilder::new(app, MAIN_WINDOW, WebviewUrl::App("index.html".into()))
        .title("ezmark")
        .inner_size(920.0, 820.0)
        .min_inner_size(360.0, 300.0)
        // The webview never leaves the app page (plan section 7, "Navigation").
        .on_navigation(is_app_url);
    #[cfg(feature = "test-harness")]
    let builder = harness::configure_window(builder);
    builder.build()?;
    Ok(())
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
            "file:///C:/x.md",
            "ezasset://localhost/1/00",
            "about:blank",
            "javascript:alert(1)",
        ] {
            assert!(!is_app_url(&Url::parse(bad).unwrap()), "{bad}");
        }
    }
}
