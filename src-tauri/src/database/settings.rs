use crate::settings::AppSettings;
use rusqlite::{Connection, OptionalExtension};

pub(crate) struct LoadedSettings {
    pub settings: AppSettings,
    pub warning: Option<String>,
    pub customized: bool,
}
pub(crate) fn read(connection: &Connection) -> Result<LoadedSettings, String> {
    let json: Option<String> = connection
        .query_row("SELECT value FROM app_settings WHERE id=1", [], |row| {
            row.get(0)
        })
        .optional()
        .map_err(|e| e.to_string())?;
    let Some(json) = json else {
        return Ok(LoadedSettings {
            settings: AppSettings::default(),
            warning: None,
            customized: false,
        });
    };
    let parsed = if json.len() <= 16384 {
        serde_json::from_str::<AppSettings>(&json).map_err(|_| "Preferências inválidas".to_owned())
    } else {
        Err("Preferências muito grandes".into())
    };
    match parsed.and_then(|settings|{settings.validate()?;Ok(settings)}) {
        Ok(settings)=>Ok(LoadedSettings {settings,warning:None,customized:true}),
        Err(_)=>Ok(LoadedSettings {settings:AppSettings::default(),warning:Some("Preferências salvas inválidas; usando padrões seguros. Salve novamente para corrigir.".into()),customized:false}),
    }
}
pub(crate) fn load(connection: &Connection) -> Result<AppSettings, String> {
    Ok(read(connection)?.settings)
}
pub(crate) fn save(connection: &Connection, settings: &AppSettings) -> Result<(), String> {
    settings.validate()?;
    let json = serde_json::to_string(settings).map_err(|e| e.to_string())?;
    connection.execute("INSERT INTO app_settings(id,value,updated_at) VALUES(1,?1,strftime('%Y-%m-%dT%H:%M:%fZ','now')) ON CONFLICT(id) DO UPDATE SET value=excluded.value,updated_at=excluded.updated_at",[json]).map_err(|e|e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{database::Database, storage::StorageManager};
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };
    #[test]
    fn corrupt_preferences_fall_back_without_overwriting_and_partial_preferences_use_defaults() {
        let mut connection = Connection::open_in_memory().unwrap();
        crate::database::migrations::apply(&mut connection).unwrap();
        for json in [
            "{",
            r#"{"maxThreads":0}"#,
            r#"{"videoFps":"wrong"}"#,
            r#"{"unexpected":true}"#,
        ] {
            connection
                .execute(
                    "INSERT OR REPLACE INTO app_settings VALUES(1,?1,'test')",
                    [json],
                )
                .unwrap();
            let loaded = read(&connection).unwrap();
            assert_eq!(loaded.settings, AppSettings::default());
            assert!(loaded.warning.is_some());
            assert!(!loaded.customized);
            let stored: String = connection
                .query_row("SELECT value FROM app_settings", [], |r| r.get(0))
                .unwrap();
            assert_eq!(stored, json);
        }
        connection
            .execute("UPDATE app_settings SET value=?1", [r#"{"language":"en"}"#])
            .unwrap();
        let loaded = read(&connection).unwrap();
        assert!(loaded.warning.is_none());
        assert_eq!(
            loaded.settings,
            AppSettings {
                language: "en".into(),
                ..Default::default()
            }
        );
    }

    #[test]
    fn preferences_persist_after_reopening_and_invalid_updates_keep_previous_value() {
        let root = std::env::temp_dir().join(format!(
            "settings-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let storage = StorageManager::initialize_in(&root).unwrap();
        let database = Database::initialize(&storage).unwrap();
        let prefs = AppSettings {
            language: "es".into(),
            max_threads: 1,
            video_resolution: "480p".into(),
            video_fps: 10,
            microphone_device_id: Some("removed-device".into()),
            ..Default::default()
        };
        save(&database.connect().unwrap(), &prefs).unwrap();
        let reopened = Database::initialize(&storage).unwrap();
        assert_eq!(load(&reopened.connect().unwrap()).unwrap(), prefs);
        let invalid = AppSettings {
            max_threads: 0,
            ..prefs.clone()
        };
        assert!(save(&reopened.connect().unwrap(), &invalid).is_err());
        assert_eq!(load(&reopened.connect().unwrap()).unwrap(), prefs);
        fs::remove_dir_all(root).unwrap();
    }
}
