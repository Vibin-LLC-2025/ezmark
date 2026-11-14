//! IPC commands. Each one is enrolled in the ACL by `build.rs` and granted only
//! to the main window by `capabilities/main.json`. The frontend is untrusted:
//! no command takes a path, sets the root, or carries a trust flag. Neither
//! command takes any argument at all.

use tauri::{AppHandle, State};

use crate::documents::{self, Documents, Ready, View};

/// The frontend is listening for events. Starts any buffered open and returns
/// what to show now, and what is still rendering.
#[tauri::command]
pub fn frontend_ready(app: AppHandle) -> Ready {
    documents::frontend_ready(&app)
}

/// What the frontend should show now.
#[tauri::command]
pub fn current_view(documents: State<'_, Documents>) -> View {
    documents.view()
}
