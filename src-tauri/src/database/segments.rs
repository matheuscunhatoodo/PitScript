use rusqlite::{params, Connection, OptionalExtension, Result};
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    pub id: String,
    pub meeting_id: String,
    pub diarization_label: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
    pub confidence: Option<f32>,
    pub source: String,
    pub created_at: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NewSegment {
    pub label: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
    pub confidence: Option<f32>,
    pub source: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiarizationState {
    pub meeting_id: String,
    pub status: String,
    pub progress: u8,
    pub stage: String,
    pub error: Option<String>,
    pub model: Option<String>,
    pub segments: Vec<Segment>,
}

pub fn read(connection: &Connection, id: &str) -> Result<DiarizationState> {
    if super::meetings::get(connection, id)?.is_none() {
        return Err(rusqlite::Error::QueryReturnedNoRows);
    }
    let mut state = connection.query_row("SELECT status, stage, progress, error, model FROM meeting_diarization WHERE meeting_id=?1", [id], |row| Ok(DiarizationState { meeting_id: id.into(), status: row.get(0)?, stage: row.get(1)?, progress: row.get(2)?, error: row.get(3)?, model: row.get(4)?, segments: Vec::new() })).optional()?.unwrap_or(DiarizationState { meeting_id: id.into(), status: "not_started".into(), stage: "idle".into(), progress: 0, error: None, model: None, segments: Vec::new() });
    let mut statement = connection.prepare("SELECT id, meeting_id, diarization_label, start_ms, end_ms, text, confidence, source, created_at FROM transcript_segments WHERE meeting_id=?1 ORDER BY start_ms, end_ms, id")?;
    state.segments = statement
        .query_map([id], |row| {
            Ok(Segment {
                id: row.get(0)?,
                meeting_id: row.get(1)?,
                diarization_label: row.get(2)?,
                start_ms: row.get(3)?,
                end_ms: row.get(4)?,
                text: row.get(5)?,
                confidence: row.get(6)?,
                source: row.get(7)?,
                created_at: row.get(8)?,
            })
        })?
        .collect::<Result<Vec<_>>>()?;
    Ok(state)
}
pub fn update_state(
    connection: &Connection,
    id: &str,
    status: &str,
    stage: &str,
    progress: u8,
    error: Option<&str>,
) -> Result<()> {
    connection.execute("INSERT INTO meeting_diarization(meeting_id,status,stage,progress,error,model,updated_at) VALUES(?1,?2,?3,?4,?5,'pyannote3-int8 + WeSpeaker ResNet34 LM',strftime('%Y-%m-%dT%H:%M:%fZ','now')) ON CONFLICT(meeting_id) DO UPDATE SET status=excluded.status,stage=excluded.stage,progress=excluded.progress,error=excluded.error,updated_at=excluded.updated_at", params![id,status,stage,progress.min(100),error])?;
    Ok(())
}
pub fn replace(connection: &mut Connection, id: &str, segments: &[NewSegment]) -> Result<()> {
    if segments.is_empty()
        || segments.iter().any(|segment| {
            segment.text.trim().is_empty()
                || segment
                    .confidence
                    .is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value))
        })
    {
        return Err(rusqlite::Error::InvalidQuery);
    }
    let transaction = connection.transaction()?;
    transaction.execute("DELETE FROM transcript_segments WHERE meeting_id=?1", [id])?;
    for segment in segments {
        transaction.execute("INSERT INTO transcript_segments(id,meeting_id,diarization_label,start_ms,end_ms,text,confidence,source,created_at) VALUES(lower(hex(randomblob(16))),?1,?2,?3,?4,?5,?6,?7,strftime('%Y-%m-%dT%H:%M:%fZ','now'))", params![id,segment.label,segment.start_ms,segment.end_ms,segment.text,segment.confidence,segment.source])?;
    }
    update_state(&transaction, id, "completed", "completed", 100, None)?;
    transaction.commit()
}

pub fn format_transcript(segments: &[Segment]) -> String {
    segments
        .iter()
        .map(|segment| {
            format!(
                "{:02}:{:02}:{:02}\n{}:\n{}",
                segment.start_ms / 3_600_000,
                (segment.start_ms / 60_000) % 60,
                (segment.start_ms / 1000) % 60,
                segment.diarization_label,
                segment.text.trim()
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{meetings, migrations, NewMeeting};

    fn database() -> (Connection, String) {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .pragma_update(None, "foreign_keys", true)
            .unwrap();
        migrations::apply(&mut connection).unwrap();
        let meeting = meetings::create(
            &connection,
            &NewMeeting {
                title: "Antiga".into(),
                microphone_enabled: true,
                system_audio_enabled: true,
                video_enabled: false,
            },
        )
        .unwrap();
        (connection, meeting.id)
    }
    fn segment() -> NewSegment {
        NewSegment {
            label: "Você".into(),
            start_ms: 30,
            end_ms: 800,
            text: "Olá ação".into(),
            confidence: None,
            source: "microphone".into(),
        }
    }
    #[test]
    fn old_meeting_has_no_segments_and_replacement_is_atomic() {
        let (mut connection, id) = database();
        assert_eq!(read(&connection, &id).unwrap().status, "not_started");
        assert!(read(&connection, &id).unwrap().segments.is_empty());
        replace(&mut connection, &id, &[segment()]).unwrap();
        let saved = read(&connection, &id).unwrap();
        assert_eq!(saved.status, "completed");
        assert_eq!(saved.segments[0].text, "Olá ação");
        let mut invalid = segment();
        invalid.end_ms = 0;
        assert!(replace(&mut connection, &id, &[segment(), invalid]).is_err());
        assert_eq!(read(&connection, &id).unwrap().segments, saved.segments);
        let mut invalid = segment();
        invalid.confidence = Some(f32::NAN);
        assert!(replace(&mut connection, &id, &[invalid]).is_err());
        assert!(meetings::get(&connection, &id).unwrap().is_some());
    }
    #[test]
    fn failure_keeps_text_and_deletion_cascades_only_this_meeting() {
        let (mut connection, id) = database();
        replace(&mut connection, &id, &[segment()]).unwrap();
        update_state(
            &connection,
            &id,
            "failed",
            "diarization",
            45,
            Some("Model missing"),
        )
        .unwrap();
        let state = read(&connection, &id).unwrap();
        assert_eq!(state.error.as_deref(), Some("Model missing"));
        assert_eq!(state.segments.len(), 1);
        meetings::delete(&connection, &id).unwrap();
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM transcript_segments", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0);
        assert!(read(&connection, &id).is_err());
    }
}
