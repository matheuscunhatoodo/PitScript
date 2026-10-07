use std::sync::Arc;

use tauri::State;

use crate::{
    audio::{loopback::OutputDevice, microphone::InputDevice},
    meeting::manager::MeetingManager,
};

#[tauri::command]
pub async fn list_input_devices(
    manager: State<'_, Arc<MeetingManager>>,
) -> Result<Vec<InputDevice>, String> {
    let manager = manager.inner().clone();
    tauri::async_runtime::spawn_blocking(move || manager.list_input_devices())
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn list_output_devices(
    manager: State<'_, Arc<MeetingManager>>,
) -> Result<Vec<OutputDevice>, String> {
    let manager = manager.inner().clone();
    tauri::async_runtime::spawn_blocking(move || manager.list_output_devices())
        .await
        .map_err(|error| error.to_string())?
}
