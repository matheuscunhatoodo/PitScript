use rusqlite::{params, Connection, OptionalExtension, Result, Row};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Meeting {
    pub id: String,
    pub title: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub duration_seconds: i64,
    pub microphone_enabled: bool,
    pub system_audio_enabled: bool,
    pub video_enabled: bool,
    pub microphone_path: Option<String>,
    pub system_audio_path: Option<String>,
    pub merged_audio_path: Option<String>,
    pub video_path: Option<String>,
    pub transcription: Option<String>,
    pub transcription_status: String,
    pub transcription_model: Option<String>,
    pub language: String,
    pub status: String,
    pub recording_status: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewMeeting {
    pub title: String,
    pub microphone_enabled: bool,
    pub system_audio_enabled: bool,
    pub video_enabled: bool,
}

const COLUMNS: &str = "
    id, title, started_at, finished_at, duration_seconds,
    microphone_enabled, system_audio_enabled, video_enabled,
    microphone_path, system_audio_path, merged_audio_path, video_path,
    transcription, transcription_status, transcription_model, language,
    status, recording_status, created_at, updated_at
";

fn from_row(row: &Row<'_>) -> Result<Meeting> {
    Ok(Meeting {
        id: row.get(0)?,
        title: row.get(1)?,
        started_at: row.get(2)?,
        finished_at: row.get(3)?,
        duration_seconds: row.get(4)?,
        microphone_enabled: row.get(5)?,
        system_audio_enabled: row.get(6)?,
        video_enabled: row.get(7)?,
        microphone_path: row.get(8)?,
        system_audio_path: row.get(9)?,
        merged_audio_path: row.get(10)?,
        video_path: row.get(11)?,
        transcription: row.get(12)?,
        transcription_status: row.get(13)?,
        transcription_model: row.get(14)?,
        language: row.get(15)?,
        status: row.get(16)?,
        recording_status: row.get(17)?,
        created_at: row.get(18)?,
        updated_at: row.get(19)?,
    })
}

pub fn create(connection: &Connection, input: &NewMeeting) -> Result<Meeting> {
    let id: String =
        connection.query_row("SELECT lower(hex(randomblob(16)))", [], |row| row.get(0))?;
    connection.execute(
        "
        INSERT INTO meetings (
            id, title, started_at, microphone_enabled, system_audio_enabled,
            video_enabled, created_at, updated_at
        ) VALUES (
            ?1, ?2, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), ?3, ?4,
            ?5, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
            strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
        )
        ",
        params![
            id,
            input.title.trim(),
            input.microphone_enabled,
            input.system_audio_enabled,
            input.video_enabled,
        ],
    )?;
    get(connection, &id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)
}

pub fn list(connection: &Connection) -> Result<Vec<Meeting>> {
    let mut statement = connection.prepare(&format!(
        "SELECT {COLUMNS} FROM meetings ORDER BY started_at DESC, id DESC"
    ))?;
    let meetings = statement.query_map([], from_row)?.collect();
    meetings
}

pub fn get(connection: &Connection, id: &str) -> Result<Option<Meeting>> {
    connection
        .query_row(
            &format!("SELECT {COLUMNS} FROM meetings WHERE id = ?1"),
            [id],
            from_row,
        )
        .optional()
}

pub fn update(connection: &Connection, meeting: &Meeting) -> Result<Option<Meeting>> {
    let changed = connection.execute(
        "
        UPDATE meetings SET
            title = ?1,
            started_at = ?2,
            finished_at = ?3,
            duration_seconds = ?4,
            microphone_enabled = ?5,
            system_audio_enabled = ?6,
            video_enabled = ?7,
            microphone_path = ?8,
            system_audio_path = ?9,
            merged_audio_path = ?10,
            video_path = ?11,
            transcription = ?12,
            transcription_status = ?13,
            transcription_model = ?14,
            language = ?15,
            status = ?16,
            recording_status = ?18,
            updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
        WHERE id = ?17
        ",
        params![
            &meeting.title,
            &meeting.started_at,
            meeting.finished_at.as_deref(),
            meeting.duration_seconds,
            meeting.microphone_enabled,
            meeting.system_audio_enabled,
            meeting.video_enabled,
            meeting.microphone_path.as_deref(),
            meeting.system_audio_path.as_deref(),
            meeting.merged_audio_path.as_deref(),
            meeting.video_path.as_deref(),
            meeting.transcription.as_deref(),
            &meeting.transcription_status,
            meeting.transcription_model.as_deref(),
            &meeting.language,
            &meeting.status,
            &meeting.id,
            meeting.recording_status.as_deref(),
        ],
    )?;
    if changed == 0 {
        Ok(None)
    } else {
        get(connection, &meeting.id)
    }
}

pub fn delete(connection: &Connection, id: &str) -> Result<bool> {
    Ok(connection.execute("DELETE FROM meetings WHERE id = ?1", [id])? > 0)
}
