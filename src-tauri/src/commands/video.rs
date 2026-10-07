#[tauri::command]
pub(crate) async fn list_video_sources() -> Result<Vec<crate::video::VideoSource>, String> {
    tauri::async_runtime::spawn_blocking(crate::video::list_sources)
        .await
        .map_err(|e| e.to_string())?
}
