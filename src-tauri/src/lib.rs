#![deny(clippy::all)]

mod audio;
mod commands;
mod database;
mod meeting;
mod settings;
pub mod storage;
mod transcription;
mod tray;
mod video;

use std::sync::Arc;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let storage = storage::StorageManager::initialize()?;
            use storage::diagnostics::Value;
            storage.log().event(
                "INFO",
                "application_start",
                &[
                    ("version", Value::Static(env!("CARGO_PKG_VERSION"))),
                    ("os", Value::Static(std::env::consts::OS)),
                    ("arch", Value::Static(std::env::consts::ARCH)),
                ],
            );
            let panic_log = storage.log();
            std::panic::set_hook(Box::new(move |info| {
                // Panic payloads can contain paths, SQL or transcript text. Never print them.
                panic_log.event(
                    "ERROR",
                    "internal_panic",
                    &[(
                        "source_line",
                        Value::Count(
                            info.location()
                                .map_or(0, |location| u64::from(location.line())),
                        ),
                    )],
                );
            }));
            let database = database::Database::initialize(&storage).inspect_err(|error| {
                storage
                    .log()
                    .failure("database_initialize_failed", None, &error.to_string())
            })?;
            let resource_dir = app
                .path()
                .resource_dir()
                .inspect_err(|error| {
                    storage
                        .log()
                        .failure("resource_directory_failed", None, &error.to_string())
                })?
                .join("resources/whisper");
            let resource_dir = if resource_dir.join("whisper-cli.exe").is_file() {
                resource_dir
            } else if cfg!(debug_assertions) {
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/whisper")
            } else {
                resource_dir
            };
            let meeting_manager =
                Arc::new(meeting::manager::MeetingManager::with_whisper_resources(
                    database.clone(),
                    storage.clone(),
                    resource_dir,
                ));
            meeting_manager
                .recover_transcription()
                .inspect_err(|error| {
                    storage
                        .log()
                        .failure("transcription_recovery_failed", None, error)
                })?;
            app.manage(meeting_manager.clone());
            app.manage(storage.clone());
            app.manage(database);
            tray::install(app.handle(), meeting_manager).inspect_err(|error| {
                storage
                    .log()
                    .failure("tray_initialize_failed", None, &error.to_string())
            })?;
            storage.log().event("INFO", "application_ready", &[]);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::meeting::create_meeting,
            commands::meeting::list_meetings,
            commands::meeting::get_meeting,
            commands::meeting::update_meeting,
            commands::meeting::delete_meeting,
            commands::meeting::start_meeting,
            commands::meeting::stop_meeting,
            commands::meeting::get_recording_state,
            commands::meeting::transcribe_meeting,
            commands::meeting::cancel_transcription,
            commands::meeting::get_transcription_state,
            commands::meeting::diarize_meeting,
            commands::meeting::get_meeting_diarization,
            commands::audio::list_input_devices,
            commands::audio::list_output_devices,
            commands::video::list_video_sources,
            commands::settings::get_settings,
            commands::settings::save_settings,
            commands::files::get_meeting_audio,
            commands::files::get_meeting_video,
            commands::files::export_transcript,
            commands::files::open_meeting_folder,
        ])
        .on_window_event(tray::on_window_event)
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(tray::on_run_event);
}
