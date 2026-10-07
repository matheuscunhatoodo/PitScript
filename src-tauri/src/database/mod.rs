pub(crate) mod locations;
pub(crate) mod meetings;
mod migrations;
pub(crate) mod segments;
pub(crate) mod settings;

use std::{
    error::Error,
    path::{Path, PathBuf},
    time::Duration,
};

use rusqlite::Connection;

use crate::storage::StorageManager;

pub use meetings::{Meeting, NewMeeting};

#[derive(Clone)]
pub struct Database {
    path: PathBuf,
    log: crate::storage::diagnostics::DiagnosticLog,
}

impl Database {
    pub fn initialize(storage: &StorageManager) -> Result<Self, Box<dyn Error>> {
        let path = storage.database_path();
        let mut connection = Self::connect_path(&path)?;
        migrations::apply(&mut connection)?;
        locations::restore(&connection, storage)?;
        let preferences = settings::load(&connection)?;
        if let Some(path) = preferences.recordings_directory {
            // A removable/unavailable destination must not prevent opening existing meetings.
            if let Ok(prepared) = storage.prepare_recordings_root(Path::new(&path)) {
                storage.set_recordings_root(prepared);
            }
        }
        Ok(Self {
            path,
            log: storage.log(),
        })
    }

    #[cfg(test)]
    fn initialize_in(local_data_root: &Path) -> Result<Self, Box<dyn Error>> {
        let storage = StorageManager::initialize_in(local_data_root)?;
        Self::initialize(&storage)
    }

    pub fn connect(&self) -> rusqlite::Result<Connection> {
        Self::connect_path(&self.path).inspect_err(|error| {
            self.log
                .failure("database_connect_failed", None, &error.to_string())
        })
    }

    fn connect_path(path: &Path) -> rusqlite::Result<Connection> {
        let connection = Connection::open(path)?;
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.pragma_update(None, "foreign_keys", true)?;
        Ok(connection)
    }

    #[cfg(test)]
    fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use std::{
        env, fs,
        path::PathBuf,
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::{meetings, Database, NewMeeting, StorageManager};

    fn temp_root(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "meeting-recorder-{label}-{}-{nanos}",
            std::process::id()
        ))
    }

    fn new_meeting(title: &str) -> NewMeeting {
        NewMeeting {
            title: title.to_owned(),
            microphone_enabled: true,
            system_audio_enabled: false,
            video_enabled: false,
        }
    }

    #[test]
    fn initialization_creates_database_and_metadata_schema() {
        let root = temp_root("schema");
        let db = Database::initialize_in(&root).unwrap();
        assert!(db.path().exists());
        assert_eq!(
            db.path(),
            root.join("MeetingRecorder/database/meeting-recorder.db")
        );

        let conn = db.connect().unwrap();
        let mut statement = conn
            .prepare("SELECT name, type FROM pragma_table_info('meetings')")
            .unwrap();
        let columns: Vec<(String, String)> = statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert_eq!(columns.len(), 20);
        assert!(columns.iter().all(|(_, kind)| kind != "BLOB"));
        for name in [
            "microphone_path",
            "system_audio_path",
            "merged_audio_path",
            "video_path",
            "transcription",
        ] {
            assert!(columns.contains(&(name.to_owned(), "TEXT".to_owned())));
        }
        drop(statement);
        drop(conn);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn create_and_get_preserve_defaults_and_input() {
        let root = temp_root("create");
        let db = Database::initialize_in(&root).unwrap();
        let conn = db.connect().unwrap();
        let created = meetings::create(&conn, &new_meeting("Reunião de produto")).unwrap();

        assert_eq!(created.id.len(), 32);
        assert_eq!(created.title, "Reunião de produto");
        assert!(created.microphone_enabled);
        assert!(!created.system_audio_enabled);
        assert_eq!(created.status, "created");
        assert_eq!(created.transcription_status, "pending");
        assert_eq!(created.language, "pt");
        assert_eq!(created.duration_seconds, 0);
        assert!(created.microphone_path.is_none());
        assert_eq!(meetings::get(&conn, &created.id).unwrap(), Some(created));
        drop(conn);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn list_returns_created_meetings() {
        let root = temp_root("list");
        let db = Database::initialize_in(&root).unwrap();
        let conn = db.connect().unwrap();
        meetings::create(&conn, &new_meeting("Primeira")).unwrap();
        meetings::create(&conn, &new_meeting("Segunda")).unwrap();

        let mut titles: Vec<String> = meetings::list(&conn)
            .unwrap()
            .into_iter()
            .map(|meeting| meeting.title)
            .collect();
        titles.sort();
        assert_eq!(titles, ["Primeira", "Segunda"]);
        drop(conn);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn update_persists_metadata_paths_status_and_text() {
        let root = temp_root("update");
        let db = Database::initialize_in(&root).unwrap();
        let conn = db.connect().unwrap();
        let mut meeting = meetings::create(&conn, &new_meeting("Antes")).unwrap();
        let created_at = meeting.created_at.clone();
        meeting.title = "Depois".to_owned();
        meeting.finished_at = Some("2026-09-30T12:30:00Z".to_owned());
        meeting.duration_seconds = 1800;
        meeting.microphone_path = Some("recordings/example/microphone.wav".to_owned());
        meeting.system_audio_path = Some("recordings/example/system.wav".to_owned());
        meeting.merged_audio_path = Some("recordings/example/merged.wav".to_owned());
        meeting.video_path = Some("recordings/example/video.mp4".to_owned());
        meeting.transcription = Some("Texto local".to_owned());
        meeting.transcription_status = "completed".to_owned();
        meeting.transcription_model = Some("Base Multilingual Q5".to_owned());
        meeting.status = "completed".to_owned();

        let updated = meetings::update(&conn, &meeting).unwrap().unwrap();
        assert_eq!(updated.title, "Depois");
        assert_eq!(updated.created_at, created_at);
        assert_eq!(updated.duration_seconds, 1800);
        assert_eq!(
            updated.microphone_path.as_deref(),
            Some("recordings/example/microphone.wav")
        );
        assert_eq!(
            updated.system_audio_path.as_deref(),
            Some("recordings/example/system.wav")
        );
        assert_eq!(
            updated.merged_audio_path.as_deref(),
            Some("recordings/example/merged.wav")
        );
        assert_eq!(
            updated.video_path.as_deref(),
            Some("recordings/example/video.mp4")
        );
        assert_eq!(updated.transcription.as_deref(), Some("Texto local"));
        assert_eq!(updated.transcription_status, "completed");
        assert_eq!(updated.status, "completed");
        assert_eq!(meetings::get(&conn, &meeting.id).unwrap(), Some(updated));
        drop(conn);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn delete_removes_only_the_requested_meeting() {
        let root = temp_root("delete");
        let db = Database::initialize_in(&root).unwrap();
        let conn = db.connect().unwrap();
        let first = meetings::create(&conn, &new_meeting("Primeira")).unwrap();
        let second = meetings::create(&conn, &new_meeting("Segunda")).unwrap();

        assert!(meetings::delete(&conn, &first.id).unwrap());
        assert!(!meetings::delete(&conn, &first.id).unwrap());
        assert!(meetings::get(&conn, &first.id).unwrap().is_none());
        assert_eq!(meetings::get(&conn, &second.id).unwrap(), Some(second));
        drop(conn);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reopening_database_keeps_meetings_and_applies_migration_once() {
        let root = temp_root("reopen");
        let id = {
            let db = Database::initialize_in(&root).unwrap();
            let conn = db.connect().unwrap();
            meetings::create(&conn, &new_meeting("Persistida"))
                .unwrap()
                .id
        };

        let reopened = Database::initialize_in(&root).unwrap();
        let conn = reopened.connect().unwrap();
        assert_eq!(
            meetings::get(&conn, &id).unwrap().unwrap().title,
            "Persistida"
        );
        let version: i64 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, 4);
        drop(conn);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn migrates_version_one_without_losing_existing_rows() {
        let root = temp_root("migrate-transcription");
        let storage = StorageManager::initialize_in(&root).unwrap();
        let connection = rusqlite::Connection::open(storage.database_path()).unwrap();
        connection.execute_batch("CREATE TABLE meetings (id TEXT PRIMARY KEY, title TEXT NOT NULL); INSERT INTO meetings VALUES ('old', 'Antes'); PRAGMA user_version = 1;").unwrap();
        drop(connection);
        let database = Database::initialize(&storage).unwrap();
        let connection = database.connect().unwrap();
        let title: String = connection
            .query_row("SELECT title FROM meetings WHERE id = 'old'", [], |row| {
                row.get(0)
            })
            .unwrap();
        let recovery: Option<String> = connection
            .query_row(
                "SELECT recording_status FROM meetings WHERE id = 'old'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(title, "Antes");
        assert_eq!(recovery, None);
        assert_eq!(version, 4);
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn version_three_upgrade_preserves_meetings_segments_and_defaults() {
        let root = temp_root("settings-upgrade");
        let storage = StorageManager::initialize_in(&root).unwrap();
        let database = Database::initialize(&storage).unwrap();
        let mut conn = database.connect().unwrap();
        let meeting = meetings::create(&conn, &new_meeting("Anterior")).unwrap();
        super::segments::replace(
            &mut conn,
            &meeting.id,
            &[super::segments::NewSegment {
                label: "Você".into(),
                start_ms: 0,
                end_ms: 1000,
                text: "Texto existente".into(),
                confidence: None,
                source: "microphone".into(),
            }],
        )
        .unwrap();
        conn.execute_batch(
            "DROP TABLE recording_locations; DROP TABLE app_settings; PRAGMA user_version=3;",
        )
        .unwrap();
        drop(conn);
        let upgraded = Database::initialize(&storage).unwrap();
        let conn = upgraded.connect().unwrap();
        assert_eq!(meetings::get(&conn, &meeting.id).unwrap().unwrap(), meeting);
        assert_eq!(
            super::segments::read(&conn, &meeting.id).unwrap().segments[0].text,
            "Texto existente"
        );
        assert_eq!(super::settings::load(&conn).unwrap(), Default::default());
        assert_eq!(
            conn.pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
                .unwrap(),
            4
        );
        drop(conn);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unavailable_configured_directory_keeps_old_locations_and_uses_default() {
        let root = temp_root("settings-missing-root");
        let storage = StorageManager::initialize_in(&root).unwrap();
        let database = Database::initialize(&storage).unwrap();
        let conn = database.connect().unwrap();
        let old = meetings::create(&conn, &new_meeting("Existente")).unwrap();
        let old_directory = storage.create_meeting_directory(&old.id).unwrap();
        super::locations::save(&conn, &old.id, &old_directory).unwrap();
        let invalid_directory = root.join("unavailable");
        fs::write(&invalid_directory, b"file cannot be a directory").unwrap();
        super::settings::save(
            &conn,
            &crate::settings::AppSettings {
                recordings_directory: Some(invalid_directory.to_string_lossy().into_owned()),
                ..Default::default()
            },
        )
        .unwrap();
        drop(conn);
        let fresh = StorageManager::initialize_in(&root).unwrap();
        let reopened = Database::initialize(&fresh).unwrap();
        assert_eq!(fresh.recordings_root(), fresh.default_recordings_root());
        assert_eq!(
            fresh.existing_meeting_directory(&old.id).unwrap(),
            fs::canonicalize(old_directory).unwrap()
        );
        let manager = crate::meeting::manager::MeetingManager::new(reopened, fresh);
        assert!(manager
            .get_settings()
            .unwrap()
            .warnings
            .iter()
            .any(|w| w.contains("Diretório configurado indisponível")));
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn settings_and_both_recording_roots_survive_separate_processes() {
        if let Ok(mode) = env::var("MEETING_RECORDER_PHASE12_PROBE") {
            let storage = StorageManager::initialize().unwrap();
            let database = Database::initialize(&storage).unwrap();
            let manager =
                crate::meeting::manager::MeetingManager::new(database.clone(), storage.clone());
            let app_root = storage
                .default_recordings_root()
                .parent()
                .unwrap()
                .to_owned();
            let custom = app_root.parent().unwrap().join("Custom Recordings");
            let prefs = crate::settings::AppSettings {
                microphone_device_id: Some("removed-mic".into()),
                output_device_id: Some("removed-output".into()),
                transcription_model: "tiny-q5_1".into(),
                language: "es".into(),
                max_threads: 1,
                video_resolution: "480p".into(),
                video_fps: 10,
                recordings_directory: Some(custom.to_string_lossy().into_owned()),
            };
            if mode == "write" {
                fs::write(
                    storage.models_path().join("ggml-tiny-q5_1.bin"),
                    b"availability fixture only; no inference",
                )
                .unwrap();
                let old = crate::meeting::manager::create_meeting_with_directory(
                    &database,
                    &storage,
                    &new_meeting("Old root"),
                )
                .unwrap();
                manager.save_settings(prefs).unwrap();
                let new = crate::meeting::manager::create_meeting_with_directory(
                    &database,
                    &storage,
                    &new_meeting("New root"),
                )
                .unwrap();
                for mut meeting in [old, new] {
                    let path = storage.get_microphone_path(&meeting.id).unwrap();
                    let mut wav = crate::audio::wav::WavWriter::create(&path).unwrap();
                    wav.write_samples(&[0; 9600]).unwrap();
                    wav.finish().unwrap();
                    meeting.microphone_path = Some(path.to_string_lossy().into_owned());
                    meeting.finished_at = Some("2026-10-02T12:00:00Z".into());
                    meeting.status = "completed".into();
                    meetings::update(&database.connect().unwrap(), &meeting).unwrap();
                }
            } else {
                assert_eq!(mode, "read");
                assert_eq!(manager.get_settings().unwrap().settings, prefs);
                assert_eq!(storage.recordings_root(), custom);
                let conn = database.connect().unwrap();
                let meetings = meetings::list(&conn).unwrap();
                let old = meetings.iter().find(|m| m.title == "Old root").unwrap();
                let new = meetings.iter().find(|m| m.title == "New root").unwrap();
                assert_eq!(
                    storage.existing_meeting_directory(&old.id).unwrap(),
                    fs::canonicalize(storage.default_recordings_root().join(&old.id)).unwrap()
                );
                assert_eq!(
                    storage.existing_meeting_directory(&new.id).unwrap(),
                    fs::canonicalize(custom.join(&new.id)).unwrap()
                );
                for meeting in [old, new] {
                    let audio =
                        crate::meeting::details::audio_path(&database, &storage, &meeting.id)
                            .unwrap()
                            .unwrap();
                    assert_eq!(
                        audio,
                        fs::canonicalize(storage.get_microphone_path(&meeting.id).unwrap())
                            .unwrap()
                    );
                    assert_eq!(crate::audio::mixer::valid_source_bytes(&audio), 9600);
                }
                manager.delete_meeting(&new.id).unwrap();
                assert!(storage.get_microphone_path(&old.id).unwrap().is_file());
            }
            return;
        }
        let root = temp_root("settings-processes");
        for mode in ["write", "read"] {
            let output = Command::new(env::current_exe().unwrap())
                .args([
                    "--exact",
                    "database::tests::settings_and_both_recording_roots_survive_separate_processes",
                ])
                .env("LOCALAPPDATA", &root)
                .env("MEETING_RECORDER_PHASE12_PROBE", mode)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "process {mode}: {} {}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn persistence_survives_separate_process_initializations() {
        if let Ok(mode) = env::var("MEETING_RECORDER_PHASE2_PROBE") {
            let storage = StorageManager::initialize().unwrap();
            let db = Database::initialize(&storage).unwrap();
            let connection = db.connect().unwrap();
            match mode.as_str() {
                "write" => {
                    meetings::create(&connection, &new_meeting("Entre processos")).unwrap();
                }
                "read" => {
                    let saved = meetings::list(&connection).unwrap();
                    assert_eq!(saved.len(), 1);
                    assert_eq!(saved[0].title, "Entre processos");
                }
                _ => panic!("unexpected probe mode"),
            }
            return;
        }

        let root = temp_root("process-restart");
        for mode in ["write", "read"] {
            let output = Command::new(env::current_exe().unwrap())
                .arg("--exact")
                .arg("database::tests::persistence_survives_separate_process_initializations")
                .env("LOCALAPPDATA", &root)
                .env("MEETING_RECORDER_PHASE2_PROBE", mode)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "process {mode} failed: {}",
                String::from_utf8_lossy(&output.stdout)
            );
        }
        fs::remove_dir_all(root).unwrap();
    }
}
