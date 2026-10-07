use crate::storage::StorageManager;
use rusqlite::Connection;
use std::path::PathBuf;

pub(crate) fn restore(connection: &Connection, storage: &StorageManager) -> Result<(), String> {
    let mut statement=connection.prepare("SELECT m.id,l.directory FROM meetings m LEFT JOIN recording_locations l ON l.meeting_id=m.id").map_err(|e|e.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })
        .map_err(|e| e.to_string())?;
    for row in rows {
        let (id, path) = row.map_err(|e| e.to_string())?;
        let path = path
            .map(PathBuf::from)
            .unwrap_or_else(|| storage.default_recordings_root().join(&id));
        storage
            .restore_meeting_directory(&id, path)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
pub(crate) fn save(
    connection: &Connection,
    id: &str,
    directory: &std::path::Path,
) -> rusqlite::Result<()> {
    connection.execute(
        "INSERT INTO recording_locations(meeting_id,directory) VALUES(?1,?2)",
        rusqlite::params![id, directory.to_string_lossy()],
    )?;
    Ok(())
}
