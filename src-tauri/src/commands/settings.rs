use crate::{
    meeting::manager::MeetingManager,
    settings::{AppSettings, SettingsInfo},
};
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub(crate) async fn get_settings(
    manager: State<'_, Arc<MeetingManager>>,
) -> Result<SettingsInfo, String> {
    let manager = manager.inner().clone();
    tauri::async_runtime::spawn_blocking(move || manager.get_settings())
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
pub(crate) async fn save_settings(
    manager: State<'_, Arc<MeetingManager>>,
    settings: AppSettings,
) -> Result<SettingsInfo, String> {
    let manager = manager.inner().clone();
    tauri::async_runtime::spawn_blocking(move || manager.save_settings(settings))
        .await
        .map_err(|e| e.to_string())?
}
