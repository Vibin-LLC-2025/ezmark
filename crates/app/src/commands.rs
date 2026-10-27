//! IPC commands. Each one is enrolled in the ACL by `build.rs` and granted only
//! to the main window by `capabilities/main.json`. The frontend is untrusted:
//! no command takes a path, sets the root, or carries a trust flag.

use tauri::{AppHandle, State};

use crate::documents::{self, Documents, View};

/// The frontend is listening for view changes; replay any buffered open.
#[tauri::command]
pub async fn frontend_ready(app: AppHandle) {
    documents::frontend_ready(app).await;
}

/// What the frontend should show now.
#[tauri::command]
pub fn current_view(documents: State<'_, Documents>) -> View {
    documents.view()
}
