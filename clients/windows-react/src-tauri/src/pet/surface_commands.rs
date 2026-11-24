use tauri::{AppHandle, Emitter, Manager};

use super::{PET_ACTIVITY_WINDOW_LABEL, PET_WINDOW_LABEL};

#[tauri::command]
pub(crate) fn open_pet_notepad(app: AppHandle) -> Result<(), String> {
    open_pet_surface(&app, "notepad", "无法打开快速记事本。")
}

#[tauri::command]
pub(crate) fn open_pet_quick_chat(app: AppHandle) -> Result<(), String> {
    open_pet_surface(&app, "quick-chat", "无法打开快捷聊天。")
}

fn open_pet_surface(app: &AppHandle, surface: &str, error: &str) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "无法找到 ChatOS 主窗口。".to_owned())?;
    window
        .show()
        .map_err(|_| "无法显示 ChatOS 主窗口。".to_owned())?;
    window
        .set_focus()
        .map_err(|_| "无法聚焦 ChatOS 主窗口。".to_owned())?;
    app.emit("pet-open-surface", surface)
        .map_err(|_| error.to_owned())
}

pub(crate) fn hide_pet_overlay(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(PET_WINDOW_LABEL) {
        let _ = window.hide();
    }
    if let Some(window) = app.get_webview_window(PET_ACTIVITY_WINDOW_LABEL) {
        let _ = window.hide();
    }
}
