use std::path::{Path, PathBuf};

use crate::{
    audio::mixer,
    database::{meetings, Database, Meeting},
    storage::StorageManager,
};

pub(crate) fn audio_path(
    database: &Database,
    storage: &StorageManager,
    id: &str,
) -> Result<Option<PathBuf>, String> {
    let meeting = load_meeting(database, id)?;
    if meeting.finished_at.is_none() || meeting.status == "recording" {
        return Err("Finalize a gravação antes de reproduzir o áudio.".to_owned());
    }
    for path in storage
        .playback_candidates(id)
        .map_err(|error| error.to_string())?
    {
        let valid = if path.file_name().is_some_and(|name| name == "merged.wav") {
            mixer::inspect_output(&path).is_ok()
        } else {
            mixer::valid_source_bytes(&path) > 0
        };
        if valid {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

pub(crate) fn load_meeting(database: &Database, id: &str) -> Result<Meeting, String> {
    let connection = database.connect().map_err(|error| error.to_string())?;
    meetings::get(&connection, id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "A reunião não foi encontrada.".to_owned())
}

pub(crate) fn folder_path(
    database: &Database,
    storage: &StorageManager,
    id: &str,
) -> Result<PathBuf, String> {
    load_meeting(database, id)?;
    storage
        .existing_meeting_directory(id)
        .map_err(|error| error.to_string())
}

pub(crate) fn transcript_text(database: &Database, id: &str) -> Result<String, String> {
    let meeting = load_meeting(database, id)?;
    if meeting.transcription_status != "completed" {
        return Err("A transcrição ainda não está disponível para exportar.".to_owned());
    }
    let connection = database.connect().map_err(|error| error.to_string())?;
    let diarization =
        crate::database::segments::read(&connection, id).map_err(|error| error.to_string())?;
    if !diarization.segments.is_empty() {
        return Ok(crate::database::segments::format_transcript(
            &diarization.segments,
        ));
    }
    meeting
        .transcription
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| "Esta reunião não contém texto para exportar.".to_owned())
}

pub(crate) struct PreparedTxtExport {
    pub(crate) filename: String,
    text: String,
}

impl PreparedTxtExport {
    pub(crate) fn export_to(
        &self,
        storage: &StorageManager,
        destination: Option<&Path>,
    ) -> Result<bool, String> {
        let Some(destination) = destination else {
            return Ok(false);
        };
        storage
            .export_text(destination, &self.text)
            .map_err(|error| format!("Não foi possível exportar a transcrição TXT: {error}"))?;
        Ok(true)
    }
}

pub(crate) fn prepare_txt_export(
    database: &Database,
    id: &str,
    traditional: bool,
) -> Result<PreparedTxtExport, String> {
    let meeting = load_meeting(database, id)?;
    let text = if traditional {
        traditional_transcript_text(database, id)?
    } else {
        transcript_text(database, id)?
    };
    Ok(PreparedTxtExport {
        filename: StorageManager::transcript_export_filename(&meeting.title),
        text,
    })
}

#[cfg(test)]
pub(crate) fn export_transcript(
    database: &Database,
    storage: &StorageManager,
    id: &str,
    destination: &Path,
) -> Result<(), String> {
    prepare_txt_export(database, id, false)?.export_to(storage, Some(destination))?;
    Ok(())
}

#[cfg(test)]
pub(crate) fn export_traditional_transcript(
    database: &Database,
    storage: &StorageManager,
    id: &str,
    destination: &Path,
) -> Result<(), String> {
    prepare_txt_export(database, id, true)?.export_to(storage, Some(destination))?;
    Ok(())
}

pub(crate) fn traditional_transcript_text(database: &Database, id: &str) -> Result<String, String> {
    let meeting = load_meeting(database, id)?;
    if meeting.transcription_status != "completed" {
        return Err("A transcrição ainda não está disponível para exportar.".into());
    }
    meeting
        .transcription
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| "Esta reunião não contém texto para exportar.".into())
}

#[cfg(test)]
mod tests {
    use super::{audio_path, export_transcript, folder_path};
    use crate::{
        audio::wav::WavWriter,
        database::{meetings, Database, NewMeeting},
        storage::StorageManager,
    };
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn setup() -> (PathBuf, StorageManager, Database, String) {
        let root = std::env::temp_dir().join(format!(
            "meeting-details-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let storage = StorageManager::initialize_in(&root).unwrap();
        let database = Database::initialize(&storage).unwrap();
        let connection = database.connect().unwrap();
        let mut meeting = meetings::create(
            &connection,
            &NewMeeting {
                title: "Reunião".to_owned(),
                microphone_enabled: true,
                system_audio_enabled: true,
                video_enabled: false,
            },
        )
        .unwrap();
        storage.create_meeting_directory(&meeting.id).unwrap();
        meeting.finished_at = Some("2026-10-01T12:00:00Z".to_owned());
        meeting.status = "completed".to_owned();
        meeting.transcription_status = "completed".to_owned();
        meeting.transcription = Some("Olá, reunião!\nPróximos passos: revisão.".to_owned());
        meetings::update(&connection, &meeting).unwrap();
        (root, storage, database, meeting.id)
    }

    fn wav(path: &std::path::Path, rate: u32) {
        let mut writer = WavWriter::create_with_sample_rate(path, rate).unwrap();
        writer.write_samples(&[0, 0, 1, 0]).unwrap();
        writer.finish().unwrap();
    }

    #[test]
    fn prefers_merged_audio_and_falls_back_to_valid_source() {
        let (root, storage, database, id) = setup();
        let merged = storage.get_merged_audio_path(&id).unwrap();
        let microphone = storage.get_microphone_path(&id).unwrap();
        wav(&merged, 16_000);
        wav(&microphone, 48_000);
        assert_eq!(
            audio_path(&database, &storage, &id).unwrap(),
            Some(merged.canonicalize().unwrap())
        );
        fs::write(&merged, b"corrupted").unwrap();
        assert_eq!(
            audio_path(&database, &storage, &id).unwrap(),
            Some(microphone.canonicalize().unwrap())
        );
        fs::remove_file(&microphone).unwrap();
        assert!(audio_path(&database, &storage, &id).unwrap().is_none());
        assert_eq!(fs::read(&merged).unwrap(), b"corrupted");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn refuses_unknown_or_recording_meetings_and_ignores_database_paths() {
        let (root, storage, database, id) = setup();
        assert!(audio_path(&database, &storage, "unknown").is_err());
        let connection = database.connect().unwrap();
        let mut meeting = meetings::get(&connection, &id).unwrap().unwrap();
        let outside = root.join("outside.wav");
        wav(&outside, 16_000);
        meeting.merged_audio_path = Some(outside.to_string_lossy().into_owned());
        meetings::update(&connection, &meeting).unwrap();
        assert!(audio_path(&database, &storage, &id).unwrap().is_none());
        meeting.finished_at = None;
        meetings::update(&connection, &meeting).unwrap();
        assert!(audio_path(&database, &storage, &id).is_err());
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unavailable_source_does_not_hide_valid_audio_or_another_meeting() {
        let (root, storage, database, id) = setup();
        let merged = storage.get_merged_audio_path(&id).unwrap();
        wav(&merged, 16_000);
        fs::create_dir(storage.get_system_audio_path(&id).unwrap()).unwrap();
        assert_eq!(
            audio_path(&database, &storage, &id).unwrap(),
            Some(merged.canonicalize().unwrap())
        );
        fs::remove_file(&merged).unwrap();
        let system = storage.get_system_audio_path(&id).unwrap();
        fs::remove_dir(&system).unwrap();
        wav(&system, 48_000);
        assert_eq!(
            audio_path(&database, &storage, &id).unwrap(),
            Some(system.canonicalize().unwrap())
        );
        let folder = folder_path(&database, &storage, &id).unwrap();
        assert!(folder.ends_with(&id));
        assert!(folder_path(&database, &storage, "other-meeting").is_err());
        fs::remove_dir_all(folder).unwrap();
        assert!(folder_path(&database, &storage, &id).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn oversized_wav_header_falls_back_instead_of_panicking() {
        let (root, storage, database, id) = setup();
        let merged = storage.get_merged_audio_path(&id).unwrap();
        let microphone = storage.get_microphone_path(&id).unwrap();
        wav(&merged, 16_000);
        wav(&microphone, 48_000);
        let mut header = fs::read(&merged).unwrap();
        header.truncate(44);
        header[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        header[40..44].copy_from_slice(&(u32::MAX - 1).to_le_bytes());
        fs::write(&merged, &header).unwrap();
        assert_eq!(
            audio_path(&database, &storage, &id).unwrap(),
            Some(microphone.canonicalize().unwrap())
        );
        assert_eq!(fs::read(&merged).unwrap(), header);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn txt_snapshot_exports_safe_portuguese_filename_and_cancellation_writes_nothing() {
        let (root, storage, database, id) = setup();
        let conn = database.connect().unwrap();
        let mut meeting = meetings::get(&conn, &id).unwrap().unwrap();
        meeting.title = "Reunião: ação / orçamento?".into();
        meeting.transcription =
            Some("Olá! Ação, coração, revisão, São Paulo.\nVocê: amanhã às 10h. 😊".into());
        meetings::update(&conn, &meeting).unwrap();
        let prepared = super::prepare_txt_export(&database, &id, false).unwrap();
        assert_eq!(
            prepared.filename,
            "transcricao-Reunião_ ação _ orçamento_.txt"
        );
        let before = fs::read_dir(&root).unwrap().count();
        assert!(!prepared.export_to(&storage, None).unwrap());
        assert_eq!(fs::read_dir(&root).unwrap().count(), before);
        // A prepared export is the selected transcript even if the DB changes while Save is open.
        meeting.transcription = Some("Texto posterior".into());
        meetings::update(&conn, &meeting).unwrap();
        let destination = root.join(&prepared.filename);
        assert!(prepared.export_to(&storage, Some(&destination)).unwrap());
        assert_eq!(
            fs::read(&destination).unwrap(),
            "Olá! Ação, coração, revisão, São Paulo.\nVocê: amanhã às 10h. 😊".as_bytes()
        );
        assert_eq!(
            meetings::get(&conn, &id)
                .unwrap()
                .unwrap()
                .transcription
                .as_deref(),
            Some("Texto posterior")
        );
        drop(conn);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn exports_database_text_as_utf8_and_preserves_managed_files() {
        let (root, storage, database, id) = setup();
        let destination = root.join("export.txt");
        export_transcript(&database, &storage, &id, &destination).unwrap();
        assert_eq!(
            fs::read_to_string(&destination).unwrap(),
            "Olá, reunião!\nPróximos passos: revisão."
        );
        fs::write(&destination, "old text").unwrap();
        export_transcript(&database, &storage, &id, &destination).unwrap();
        assert!(fs::read_to_string(&destination).unwrap().starts_with("Olá"));
        assert!(
            export_transcript(&database, &storage, &id, &root.join("missing/export.txt")).is_err()
        );
        assert!(export_transcript(
            &database,
            &storage,
            &id,
            std::path::Path::new("relative.txt")
        )
        .is_err());
        assert!(export_transcript(&database, &storage, &id, &storage.database_path()).is_err());
        assert!(export_transcript(
            &database,
            &storage,
            &id,
            &storage.get_transcript_path(&id).unwrap()
        )
        .is_err());
        assert!(database.connect().is_ok());
        let connection = database.connect().unwrap();
        let mut meeting = meetings::get(&connection, &id).unwrap().unwrap();
        meeting.transcription_status = "processing".to_owned();
        meetings::update(&connection, &meeting).unwrap();
        assert!(export_transcript(&database, &storage, &id, &root.join("pending.txt")).is_err());
        assert!(!root.join("pending.txt").exists());
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn structured_export_keeps_timestamps_and_speakers_without_replacing_original_text() {
        use crate::database::segments::{replace, NewSegment};
        let (root, storage, database, id) = setup();
        let mut connection = database.connect().unwrap();
        replace(
            &mut connection,
            &id,
            &[
                NewSegment {
                    label: "Você".into(),
                    start_ms: 3000,
                    end_ms: 4200,
                    text: "Bom dia.".into(),
                    confidence: None,
                    source: "microphone".into(),
                },
                NewSegment {
                    label: "Participantes".into(),
                    start_ms: 12000,
                    end_ms: 14000,
                    text: "Tudo certo.".into(),
                    confidence: None,
                    source: "system".into(),
                },
            ],
        )
        .unwrap();
        drop(connection);
        let destination = root.join("export.txt");
        export_transcript(&database, &storage, &id, &destination).unwrap();
        assert_eq!(
            fs::read_to_string(destination).unwrap(),
            "00:00:03\nVocê:\nBom dia.\n\n00:00:12\nParticipantes:\nTudo certo."
        );
        super::export_traditional_transcript(
            &database,
            &storage,
            &id,
            &root.join("traditional.txt"),
        )
        .unwrap();
        assert_eq!(
            fs::read_to_string(root.join("traditional.txt")).unwrap(),
            "Olá, reunião!\nPróximos passos: revisão."
        );
        assert!(super::load_meeting(&database, &id)
            .unwrap()
            .transcription
            .unwrap()
            .starts_with("Olá"));
        fs::remove_dir_all(root).unwrap();
    }
}
