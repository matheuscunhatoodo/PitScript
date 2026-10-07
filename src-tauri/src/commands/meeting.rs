use std::sync::Arc;

use rusqlite::Connection;
use tauri::{AppHandle, State};

use crate::database::{meetings, Database, Meeting, NewMeeting};
use crate::meeting::manager::{
    create_meeting_with_directory, MeetingManager, MeetingRecordingState, StartMeetingInput,
};
use crate::storage::StorageManager;
use crate::transcription::TranscriptionState;

async fn with_connection<T, F>(database: State<'_, Database>, operation: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(&Connection) -> rusqlite::Result<T> + Send + 'static,
{
    let database = database.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let connection = database.connect().map_err(|error| error.to_string())?;
        operation(&connection).map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn create_meeting(
    database: State<'_, Database>,
    storage: State<'_, StorageManager>,
    input: NewMeeting,
) -> Result<Meeting, String> {
    if input.title.trim().is_empty() {
        return Err("O nome da reunião não pode ficar vazio.".to_owned());
    }
    let database = database.inner().clone();
    let storage = storage.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        create_meeting_with_directory(&database, &storage, &input)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn start_meeting(
    manager: State<'_, Arc<MeetingManager>>,
    app: AppHandle,
    input: StartMeetingInput,
) -> Result<MeetingRecordingState, String> {
    let manager = manager.inner().clone();
    tauri::async_runtime::spawn_blocking(move || manager.start_meeting(input, app))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn stop_meeting(
    manager: State<'_, Arc<MeetingManager>>,
    threads: Option<usize>,
) -> Result<MeetingRecordingState, String> {
    let manager = manager.inner().clone();
    tauri::async_runtime::spawn_blocking(move || manager.stop_meeting_and_transcribe(threads))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn transcribe_meeting(
    manager: State<'_, Arc<MeetingManager>>,
    id: String,
    threads: Option<usize>,
) -> Result<TranscriptionState, String> {
    let manager = manager.inner().clone();
    tauri::async_runtime::spawn_blocking(move || manager.transcribe_meeting(&id, threads))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn cancel_transcription(
    manager: State<'_, Arc<MeetingManager>>,
    id: String,
) -> Result<TranscriptionState, String> {
    manager.cancel_transcription(&id)
}

#[tauri::command]
pub async fn diarize_meeting(
    manager: State<'_, Arc<MeetingManager>>,
    id: String,
    threads: Option<usize>,
) -> Result<crate::database::segments::DiarizationState, String> {
    let manager = manager.inner().clone();
    tauri::async_runtime::spawn_blocking(move || manager.diarize_meeting(&id, threads))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn get_meeting_diarization(
    database: State<'_, Database>,
    id: String,
) -> Result<crate::database::segments::DiarizationState, String> {
    with_connection(database, move |connection| {
        crate::database::segments::read(connection, &id)
    })
    .await
}

#[tauri::command]
pub async fn get_transcription_state(
    manager: State<'_, Arc<MeetingManager>>,
    id: String,
) -> Result<TranscriptionState, String> {
    manager.get_transcription_state(&id)
}

#[tauri::command]
pub async fn get_recording_state(
    manager: State<'_, Arc<MeetingManager>>,
) -> Result<MeetingRecordingState, String> {
    let manager = manager.inner().clone();
    tauri::async_runtime::spawn_blocking(move || manager.get_recording_state())
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn list_meetings(database: State<'_, Database>) -> Result<Vec<Meeting>, String> {
    with_connection(database, meetings::list).await
}

#[tauri::command]
pub async fn get_meeting(
    database: State<'_, Database>,
    id: String,
) -> Result<Option<Meeting>, String> {
    with_connection(database, move |connection| meetings::get(connection, &id)).await
}

#[tauri::command]
pub async fn update_meeting(
    database: State<'_, Database>,
    meeting: Meeting,
) -> Result<Option<Meeting>, String> {
    if meeting.title.trim().is_empty() {
        return Err("O nome da reunião não pode ficar vazio.".to_owned());
    }
    with_connection(database, move |connection| {
        meetings::update(connection, &meeting)
    })
    .await
}

#[tauri::command]
pub async fn delete_meeting(
    manager: State<'_, Arc<MeetingManager>>,
    id: String,
) -> Result<bool, String> {
    let manager = manager.inner().clone();
    tauri::async_runtime::spawn_blocking(move || manager.delete_meeting(&id))
        .await
        .map_err(|error| error.to_string())?
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use crate::{
        database::{meetings, Database, NewMeeting},
        meeting::manager::MeetingManager,
        storage::StorageManager,
    };

    use super::create_meeting_with_directory;

    fn temp_root() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "meeting-recorder-delete-{}-{nanos}",
            std::process::id()
        ))
    }

    #[test]
    fn deleting_a_meeting_removes_its_record_and_files_only() {
        let root = temp_root();
        let storage = StorageManager::initialize_in(&root).unwrap();
        let database = Database::initialize(&storage).unwrap();
        let manager = MeetingManager::new(database.clone(), storage.clone());
        let connection = database.connect().unwrap();
        let input = |title: &str| NewMeeting {
            title: title.to_owned(),
            microphone_enabled: true,
            system_audio_enabled: true,
            video_enabled: false,
        };
        let first = meetings::create(&connection, &input("Primeira")).unwrap();
        let second = meetings::create(&connection, &input("Segunda")).unwrap();
        let first_directory = storage.create_meeting_directory(&first.id).unwrap();
        let second_directory = storage.create_meeting_directory(&second.id).unwrap();
        fs::write(storage.get_microphone_path(&first.id).unwrap(), b"first").unwrap();
        fs::write(storage.get_microphone_path(&second.id).unwrap(), b"second").unwrap();

        assert!(manager.delete_meeting(&first.id).unwrap());
        assert!(meetings::get(&connection, &first.id).unwrap().is_none());
        assert!(!first_directory.exists());
        assert!(meetings::get(&connection, &second.id).unwrap().is_some());
        assert_eq!(
            fs::read(second_directory.join("microphone.wav")).unwrap(),
            b"second"
        );
        assert!(!manager.delete_meeting(&first.id).unwrap());

        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn creating_meetings_creates_separate_directories() {
        let root = temp_root();
        let storage = StorageManager::initialize_in(&root).unwrap();
        let database = Database::initialize(&storage).unwrap();
        let input = NewMeeting {
            title: "Planejamento".to_owned(),
            microphone_enabled: true,
            system_audio_enabled: true,
            video_enabled: false,
        };

        let first = create_meeting_with_directory(&database, &storage, &input).unwrap();
        let second = create_meeting_with_directory(&database, &storage, &input).unwrap();
        let first_directory = storage
            .get_microphone_path(&first.id)
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let second_directory = storage
            .get_microphone_path(&second.id)
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        assert_ne!(first.id, second.id);
        assert_ne!(first_directory, second_directory);
        assert!(first_directory.is_dir());
        assert!(second_directory.is_dir());
        let connection = database.connect().unwrap();
        assert!(meetings::get(&connection, &first.id).unwrap().is_some());
        assert!(meetings::get(&connection, &second.id).unwrap().is_some());
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_database_record_does_not_delete_an_unrelated_directory() {
        let root = temp_root();
        let storage = StorageManager::initialize_in(&root).unwrap();
        let database = Database::initialize(&storage).unwrap();
        let manager = MeetingManager::new(database.clone(), storage.clone());
        let directory = storage.create_meeting_directory("orphan").unwrap();
        fs::write(directory.join("keep.txt"), b"keep").unwrap();

        assert!(!manager.delete_meeting("orphan").unwrap());
        assert_eq!(fs::read(directory.join("keep.txt")).unwrap(), b"keep");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn deletion_failure_preserves_database_record() {
        let root = temp_root();
        let storage = StorageManager::initialize_in(&root).unwrap();
        let database = Database::initialize(&storage).unwrap();
        let manager = MeetingManager::new(database.clone(), storage.clone());
        let connection = database.connect().unwrap();
        let meeting = meetings::create(
            &connection,
            &NewMeeting {
                title: "Preservar".to_owned(),
                microphone_enabled: true,
                system_audio_enabled: false,
                video_enabled: false,
            },
        )
        .unwrap();
        let path = storage
            .get_microphone_path(&meeting.id)
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        fs::write(&path, b"keep").unwrap();

        assert!(manager.delete_meeting(&meeting.id).is_err());
        assert!(meetings::get(&connection, &meeting.id).unwrap().is_some());
        assert_eq!(fs::read(path).unwrap(), b"keep");
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn directory_creation_failure_rolls_back_database_record() {
        let root = temp_root();
        let storage = StorageManager::initialize_in(&root).unwrap();
        let database = Database::initialize(&storage).unwrap();
        let recordings = root.join("MeetingRecorder/recordings");
        fs::remove_dir(&recordings).unwrap();
        fs::write(&recordings, b"blocked").unwrap();
        let input = NewMeeting {
            title: "Sem pasta".to_owned(),
            microphone_enabled: true,
            system_audio_enabled: false,
            video_enabled: false,
        };

        assert!(create_meeting_with_directory(&database, &storage, &input).is_err());
        let connection = database.connect().unwrap();
        assert!(meetings::list(&connection).unwrap().is_empty());
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }
}
