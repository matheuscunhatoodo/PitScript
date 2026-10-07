use rusqlite::{Connection, Result};

pub fn apply(connection: &mut Connection) -> Result<()> {
    apply_previous(connection)?;
    let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version == 2 {
        let transaction = connection.transaction()?;
        transaction.execute_batch(
            "CREATE TABLE transcript_segments (
            id TEXT PRIMARY KEY,
            meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
            diarization_label TEXT NOT NULL,
            start_ms INTEGER NOT NULL CHECK(start_ms >= 0),
            end_ms INTEGER NOT NULL CHECK(end_ms > start_ms),
            text TEXT NOT NULL,
            confidence REAL CHECK(confidence IS NULL OR confidence BETWEEN 0 AND 1),
            source TEXT NOT NULL CHECK(source IN ('microphone','system')),
            created_at TEXT NOT NULL
        );
        CREATE INDEX transcript_segments_meeting_time ON transcript_segments(meeting_id, start_ms);
        CREATE TABLE meeting_diarization (
            meeting_id TEXT PRIMARY KEY REFERENCES meetings(id) ON DELETE CASCADE,
            status TEXT NOT NULL,
            stage TEXT NOT NULL,
            progress INTEGER NOT NULL DEFAULT 0,
            error TEXT,
            model TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );",
        )?;
        transaction.pragma_update(None, "user_version", 3)?;
        transaction.commit()?;
    }
    let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version == 3 {
        let transaction = connection.transaction()?;
        transaction.execute_batch("CREATE TABLE app_settings(id INTEGER PRIMARY KEY CHECK(id=1),value TEXT NOT NULL,updated_at TEXT NOT NULL);
            CREATE TABLE recording_locations(meeting_id TEXT PRIMARY KEY REFERENCES meetings(id) ON DELETE CASCADE,directory TEXT NOT NULL);")?;
        transaction.pragma_update(None, "user_version", 4)?;
        transaction.commit()?;
    }
    Ok(())
}

fn apply_previous(connection: &mut Connection) -> Result<()> {
    let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    match version {
        0 => {
            let transaction = connection.transaction()?;
            transaction.execute_batch(
                "
                CREATE TABLE meetings (
                    id TEXT PRIMARY KEY,
                    title TEXT NOT NULL,
                    started_at TEXT NOT NULL,
                    finished_at TEXT,
                    duration_seconds INTEGER DEFAULT 0,
                    microphone_enabled INTEGER NOT NULL DEFAULT 1,
                    system_audio_enabled INTEGER NOT NULL DEFAULT 1,
                    video_enabled INTEGER NOT NULL DEFAULT 0,
                    microphone_path TEXT,
                    system_audio_path TEXT,
                    merged_audio_path TEXT,
                    video_path TEXT,
                    transcription TEXT,
                    transcription_status TEXT NOT NULL DEFAULT 'pending',
                    transcription_model TEXT,
                    language TEXT DEFAULT 'pt',
                    status TEXT NOT NULL DEFAULT 'created',
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );
                ",
            )?;
            transaction.pragma_update(None, "user_version", 1)?;
            transaction.commit()?;
            apply_transcription_recovery_migration(connection)
        }
        1 => apply_transcription_recovery_migration(connection),
        2..=4 => Ok(()),
        _ => Err(rusqlite::Error::InvalidQuery),
    }
}

fn apply_transcription_recovery_migration(connection: &mut Connection) -> Result<()> {
    let transaction = connection.transaction()?;
    transaction.execute_batch("ALTER TABLE meetings ADD COLUMN recording_status TEXT;")?;
    transaction.pragma_update(None, "user_version", 2)?;
    transaction.commit()
}
