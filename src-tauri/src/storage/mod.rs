pub(crate) mod diagnostics;
mod paths;

use std::{
    collections::HashMap,
    env, fs,
    io::{self, ErrorKind, Write},
    path::{Component, Path, PathBuf},
    sync::{Arc, RwLock},
    time::{SystemTime, UNIX_EPOCH},
};

fn is_link(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_type().is_symlink() || metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

fn ensure_directory(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !is_link(&metadata) => Ok(()),
        Ok(_) => Err(io::Error::new(
            ErrorKind::InvalidData,
            "Storage path is not a regular directory",
        )),
        Err(error) if error.kind() == ErrorKind::NotFound => match fs::create_dir(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => ensure_directory(path),
            Err(error) => Err(error),
        },
        Err(error) => Err(error),
    }
}

fn validate_meeting_id(id: &str) -> io::Result<()> {
    let valid_characters = id
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_');
    let upper = id.to_ascii_uppercase();
    let reserved = matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (upper.len() == 4
            && (upper.starts_with("COM") || upper.starts_with("LPT"))
            && upper.as_bytes()[3].is_ascii_digit()
            && upper.as_bytes()[3] != b'0');
    if id.is_empty() || id.len() > 128 || !valid_characters || reserved {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "Invalid meeting ID",
        ));
    }
    Ok(())
}

fn validate_txt_destination(destination: &Path) -> io::Result<()> {
    let invalid = || {
        io::Error::new(
            ErrorKind::InvalidInput,
            "Escolha um arquivo TXT com nome válido para Windows.",
        )
    };
    if !destination.is_absolute()
        || destination
            .components()
            .any(|part| matches!(part, Component::ParentDir))
    {
        return Err(invalid());
    }
    #[cfg(windows)]
    if destination.components().any(|part| matches!(part, Component::Prefix(prefix) if !matches!(prefix.kind(), std::path::Prefix::Disk(_) | std::path::Prefix::VerbatimDisk(_) | std::path::Prefix::UNC(_, _) | std::path::Prefix::VerbatimUNC(_, _)))) {
        return Err(invalid());
    }
    let name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(invalid)?;
    let stem = destination
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(invalid)?;
    let device = name
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end_matches(' ')
        .to_ascii_uppercase();
    let numbered_device = device
        .strip_prefix("COM")
        .or_else(|| device.strip_prefix("LPT"))
        .is_some_and(|number| {
            matches!(
                number,
                "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
            )
        });
    if !destination
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("txt"))
        || stem.trim_matches([' ', '.']).is_empty()
        || name.ends_with([' ', '.'])
        || name.encode_utf16().count() > 255
        || name
            .chars()
            .any(|character| character.is_control() || r#"<>:"/\|?*"#.contains(character))
        || matches!(
            device.as_str(),
            "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
        )
        || numbered_device
    {
        return Err(invalid());
    }
    Ok(())
}

#[derive(Clone)]
pub struct StorageManager {
    root: PathBuf,
    recordings: Arc<RwLock<RecordingDirectories>>,
}

struct RecordingDirectories {
    root: PathBuf,
    meetings: HashMap<String, PathBuf>,
}

fn validate_local_path(path: &Path) -> io::Result<()> {
    if !path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, Component::ParentDir))
    {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "Escolha um diretório absoluto sem '..'.",
        ));
    }
    #[cfg(windows)]
    {
        use std::path::Prefix;
        if path.components().any(|part|matches!(part,Component::Prefix(prefix) if !matches!(prefix.kind(),Prefix::Disk(_)|Prefix::VerbatimDisk(_)))) {
            return Err(io::Error::new(ErrorKind::InvalidInput,"Use um diretório local, não UNC/dispositivo."));
        }
        let wide: Vec<u16> = path
            .as_os_str()
            .to_string_lossy()
            .encode_utf16()
            .chain(Some(0))
            .collect();
        if unsafe {
            windows::Win32::UI::Shell::PathIsNetworkPathW(windows::core::PCWSTR(wide.as_ptr()))
        }
        .as_bool()
        {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "Diretórios de rede não são permitidos.",
            ));
        }
    }
    Ok(())
}

fn ensure_local_directory(path: &Path) -> io::Result<()> {
    validate_local_path(path)?;
    // Walk ancestors before creating children: never traverse symlinks/junctions.
    let mut current = PathBuf::new();
    for part in path.components() {
        current.push(part.as_os_str());
        if matches!(part, Component::Normal(_)) {
            ensure_directory(&current)?;
        }
    }
    Ok(())
}

fn canonical_local_directory(path: &Path) -> io::Result<PathBuf> {
    let canonical = path.canonicalize()?;
    validate_local_path(&canonical)?;
    #[cfg(windows)]
    if let Some(plain) = canonical
        .to_str()
        .and_then(|path| path.strip_prefix(r"\\?\"))
    {
        return Ok(PathBuf::from(plain));
    }
    Ok(canonical)
}

impl StorageManager {
    pub(crate) fn log(&self) -> diagnostics::DiagnosticLog {
        diagnostics::DiagnosticLog::new(self.log_path())
    }

    pub(crate) fn transcript_export_filename(title: &str) -> String {
        let stem: String = title
            .chars()
            .take(90)
            .map(|character| {
                if character.is_control() || r#"<>:"/\|?*"#.contains(character) {
                    '_'
                } else {
                    character
                }
            })
            .collect();
        let stem = stem.trim_matches([' ', '.']);
        // The prefix also prevents titles like CON, NUL or COM1 becoming device filenames.
        if stem.is_empty() {
            "transcricao.txt".into()
        } else {
            format!("transcricao-{stem}.txt")
        }
    }

    pub(crate) fn recordings_root(&self) -> PathBuf {
        self.recordings
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .root
            .clone()
    }
    pub(crate) fn models_path(&self) -> PathBuf {
        self.root.join("models")
    }
    pub(crate) fn default_recordings_root(&self) -> PathBuf {
        self.root.join("recordings")
    }
    pub(crate) fn prepare_recordings_root(&self, path: &Path) -> io::Result<PathBuf> {
        validate_local_path(path)?;
        if path != self.default_recordings_root()
            && (path.starts_with(&self.root) || self.root.starts_with(path))
        {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "Não use os diretórios internos do aplicativo.",
            ));
        }
        ensure_local_directory(path)?;
        let canonical = canonical_local_directory(path)?;
        let app_root = canonical_local_directory(&self.root)?;
        if canonical != app_root.join("recordings")
            && (canonical.starts_with(&app_root) || app_root.starts_with(&canonical))
        {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "Não use os diretórios internos do aplicativo.",
            ));
        }
        if self
            .recordings
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .meetings
            .values()
            .any(|meeting| {
                canonical_local_directory(meeting)
                    .is_ok_and(|resolved| canonical.starts_with(resolved))
            })
        {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "Não use a pasta de uma reunião como diretório de gravações.",
            ));
        }
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(io::Error::other)?
            .as_nanos();
        let probe = canonical.join(format!(
            ".meeting-write-probe-{}-{nonce}",
            std::process::id()
        ));
        let result = (|| {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&probe)?;
            file.write_all(b"write probe")?;
            file.sync_all()?;
            drop(file);
            Ok::<_, io::Error>(())
        })();
        let removed = fs::remove_file(&probe);
        result?;
        removed?;
        Ok(canonical)
    }
    pub(crate) fn set_recordings_root(&self, path: PathBuf) {
        self.recordings
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .root = path;
    }
    pub(crate) fn restore_meeting_directory(&self, id: &str, path: PathBuf) -> io::Result<()> {
        validate_meeting_id(id)?;
        validate_local_path(&path)?;
        if path.file_name().is_none_or(|name| name != id) {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "Pasta persistida não corresponde à reunião.",
            ));
        }
        self.recordings
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .meetings
            .insert(id.to_owned(), path);
        Ok(())
    }
    pub fn initialize() -> io::Result<Self> {
        let local_data_root = env::var_os("LOCALAPPDATA")
            .filter(|value| !value.is_empty())
            .ok_or_else(|| io::Error::new(ErrorKind::NotFound, "LOCALAPPDATA is not set"))?;
        Self::initialize_in(Path::new(&local_data_root))
    }

    pub(crate) fn initialize_in(local_data_root: &Path) -> io::Result<Self> {
        fs::create_dir_all(local_data_root)?;
        let root = local_data_root.join("MeetingRecorder");
        ensure_directory(&root)?;
        for name in ["database", "recordings", "models", "logs"] {
            ensure_directory(&root.join(name))?;
        }
        let recordings = Arc::new(RwLock::new(RecordingDirectories {
            root: root.join("recordings"),
            meetings: HashMap::new(),
        }));
        Ok(Self { root, recordings })
    }

    fn meeting_directory(&self, id: &str) -> io::Result<PathBuf> {
        validate_meeting_id(id)?;
        ensure_directory(&self.root)?;
        let directories = self.recordings.read().unwrap_or_else(|e| e.into_inner());
        let path = directories
            .meetings
            .get(id)
            .cloned()
            .unwrap_or_else(|| directories.root.join(id));
        drop(directories);
        let parent = path
            .parent()
            .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "Pasta da reunião inválida."))?;
        ensure_local_directory(parent)?;
        Ok(path)
    }

    pub fn create_meeting_directory(&self, id: &str) -> io::Result<PathBuf> {
        self.create_meeting_directory_inner(id)
            .inspect_err(|error| self.log().io_failure("meeting_directory_failed", error))
    }

    fn create_meeting_directory_inner(&self, id: &str) -> io::Result<PathBuf> {
        let path = self.meeting_directory(id)?;
        ensure_directory(&path)?;
        self.restore_meeting_directory(id, path.clone())?;
        Ok(path)
    }

    pub fn delete_meeting_files(&self, id: &str) -> io::Result<bool> {
        self.delete_meeting_files_inner(id)
            .inspect_err(|error| self.log().io_failure("meeting_delete_files_failed", error))
    }

    fn delete_meeting_files_inner(&self, id: &str) -> io::Result<bool> {
        let Some(path) = self.deletion_target(id)? else {
            return Ok(false);
        };
        fs::remove_dir_all(path)?;
        Ok(true)
    }

    pub(crate) fn existing_meeting_directory(&self, id: &str) -> io::Result<PathBuf> {
        self.deletion_target(id)?
            .ok_or_else(|| {
                io::Error::new(
                    ErrorKind::NotFound,
                    "A pasta da reunião não foi encontrada.",
                )
            })?
            .canonicalize()
    }

    pub(crate) fn create_postprocessing_directory(&self, id: &str) -> io::Result<PathBuf> {
        let parent = self.existing_meeting_directory(id)?;
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(io::Error::other)?
            .as_nanos();
        let path = parent.join(format!("postprocess-{}-{nonce}", std::process::id()));
        fs::create_dir(&path)?;
        Ok(path)
    }

    pub(crate) fn playback_candidates(&self, id: &str) -> io::Result<Vec<PathBuf>> {
        let directory = self.existing_meeting_directory(id)?;
        let mut candidates = Vec::new();
        for name in ["merged.wav", "microphone.wav", "system.wav"] {
            let path = directory.join(name);
            match fs::symlink_metadata(&path) {
                Ok(metadata) if metadata.is_file() && !is_link(&metadata) => candidates.push(path),
                // Never expose links/directories; keep trying the other valid sources.
                Ok(_) => {}
                Err(error) if error.kind() == ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
        Ok(candidates)
    }

    pub(crate) fn playback_video(&self, id: &str) -> io::Result<Option<PathBuf>> {
        let path = self.existing_meeting_directory(id)?.join("video.mp4");
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_file() && !is_link(&metadata) => Ok(Some(path)),
            Ok(_) => Ok(None),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub(crate) fn export_text(&self, destination: &Path, text: &str) -> io::Result<()> {
        self.export_text_inner(destination, text)
            .inspect_err(|error| self.log().io_failure("export_file_failed", error))
    }

    fn export_text_inner(&self, destination: &Path, text: &str) -> io::Result<()> {
        validate_txt_destination(destination)?;
        let parent = destination
            .parent()
            .ok_or_else(|| io::Error::new(ErrorKind::InvalidInput, "Destino inválido."))?
            .canonicalize()?;
        if parent.starts_with(self.root.canonicalize()?) {
            return Err(io::Error::new(
                ErrorKind::PermissionDenied,
                "Escolha uma pasta fora dos dados do aplicativo para exportar.",
            ));
        }
        let directories = self.recordings.read().unwrap_or_else(|e| e.into_inner());
        let protected = std::iter::once(directories.root.as_path()).chain(
            directories
                .meetings
                .values()
                .filter_map(|directory| directory.parent()),
        );
        for root in protected {
            if root
                .canonicalize()
                .is_ok_and(|root| parent.starts_with(root))
            {
                return Err(io::Error::new(
                    ErrorKind::PermissionDenied,
                    "Não exporte sobre arquivos de gravações.",
                ));
            }
        }
        drop(directories);
        let destination = parent.join(
            destination
                .file_name()
                .ok_or_else(|| io::Error::new(ErrorKind::InvalidInput, "Destino inválido."))?,
        );
        match fs::symlink_metadata(&destination) {
            Ok(metadata) if !metadata.is_file() || is_link(&metadata) => {
                return Err(io::Error::new(ErrorKind::InvalidInput, "Destino inválido."))
            }
            Err(error) if error.kind() != ErrorKind::NotFound => return Err(error),
            _ => {}
        }
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(io::Error::other)?
            .as_nanos();
        let temporary = parent.join(format!(
            ".meeting-export-{}-{timestamp}.tmp",
            std::process::id()
        ));
        let result = (|| {
            let mut file = fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temporary)?;
            file.write_all(text.as_bytes())?;
            file.sync_all()?;
            drop(file);
            fs::rename(&temporary, &destination)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }

    fn deletion_target(&self, id: &str) -> io::Result<Option<PathBuf>> {
        let path = self.meeting_directory(id)?;
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_dir() && !is_link(&metadata) => Ok(Some(path)),
            Ok(_) => Err(io::Error::new(
                ErrorKind::InvalidData,
                "Meeting path is not a regular directory",
            )),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::StorageManager;

    fn temp_root(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "meeting-recorder-storage-{label}-{}-{nanos}",
            std::process::id()
        ))
    }

    #[test]
    fn generated_names_preserve_unicode_and_fit_windows_limits() {
        let root = temp_root("txt-default-names");
        let storage = StorageManager::initialize_in(&root).unwrap();
        for (title, expected) in [
            ("", "transcricao.txt"),
            (" ... ", "transcricao.txt"),
            ("CON", "transcricao-CON.txt"),
            ("NUL.txt", "transcricao-NUL.txt.txt"),
            ("COM¹", "transcricao-COM¹.txt"),
            ("  Ação. ", "transcricao-Ação.txt"),
            ("Reunião / revisão\n", "transcricao-Reunião _ revisão_.txt"),
        ] {
            let name = StorageManager::transcript_export_filename(title);
            assert_eq!(name, expected);
            let destination = root.join(name);
            storage
                .export_text(&destination, "Português: ç, ã, é, õ.")
                .unwrap();
            assert_eq!(
                fs::read_to_string(destination).unwrap(),
                "Português: ç, ã, é, õ."
            );
        }
        let long_name = StorageManager::transcript_export_filename(&"😊".repeat(500));
        assert!(long_name.encode_utf16().count() <= 255);
        storage
            .export_text(&root.join(long_name), "Texto UTF-8")
            .unwrap();
        assert!(storage
            .export_text(&root.join(format!("{}.txt", "é".repeat(256))), "too long")
            .is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn failed_replacement_preserves_previous_export_and_cleans_temporary_file() {
        use std::os::windows::fs::OpenOptionsExt;
        let root = temp_root("txt-replace-failure");
        let storage = StorageManager::initialize_in(&root).unwrap();
        let destination = root.join("ação.txt");
        fs::write(&destination, "Arquivo anterior: coração.").unwrap();
        let locked = fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&destination)
            .unwrap();
        assert!(storage
            .export_text(&destination, "Nova exportação")
            .is_err());
        drop(locked);
        assert_eq!(
            fs::read_to_string(&destination).unwrap(),
            "Arquivo anterior: coração."
        );
        assert_eq!(fs::read_dir(&root).unwrap().count(), 2);
        storage
            .export_text(&destination, "Nova exportação")
            .unwrap();
        assert_eq!(fs::read_to_string(destination).unwrap(), "Nova exportação");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn text_export_rejects_unsafe_windows_names_without_touching_existing_files() {
        let root = temp_root("txt-names");
        let storage = StorageManager::initialize_in(&root).unwrap();
        let existing = root.join("keep.txt");
        fs::write(&existing, b"keep").unwrap();
        for name in [
            "keep.txt:stream.txt",
            "CON.txt",
            "con.backup.txt",
            "PRN.txt",
            "AUX.txt",
            "NUL.txt",
            "COM1.txt",
            "LPT9.txt",
            "COM¹.txt",
            "LPT³.txt",
            "report?.txt",
            "report\u{0001}.txt",
            "report.txt.",
            "report.txt ",
            "report.json",
            ".txt",
        ] {
            let error = storage
                .export_text(&root.join(name), "Não modificar: ação, revisão.")
                .unwrap_err();
            assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput, "{name}");
            assert_eq!(fs::read(&existing).unwrap(), b"keep");
        }
        assert!(storage
            .export_text(&root.join("missing/../keep.txt"), "overwrite")
            .is_err());
        assert_eq!(fs::read(&existing).unwrap(), b"keep");
        assert_eq!(fs::read_dir(&root).unwrap().count(), 2);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn configured_root_affects_only_new_meetings_and_survives_database_reload() {
        use crate::{
            database::{settings, Database, NewMeeting},
            meeting::manager::create_meeting_with_directory,
            settings::AppSettings,
        };
        let root = temp_root("configured-root");
        let storage = StorageManager::initialize_in(&root).unwrap();
        let db = Database::initialize(&storage).unwrap();
        let input = NewMeeting {
            title: "Config".into(),
            microphone_enabled: true,
            system_audio_enabled: false,
            video_enabled: false,
        };
        let old = create_meeting_with_directory(&db, &storage, &input).unwrap();
        let old_path = storage.get_microphone_path(&old.id).unwrap();
        fs::write(&old_path, b"preserve old").unwrap();
        let external = root.join("external-recordings");
        let prepared = storage.prepare_recordings_root(&external).unwrap();
        settings::save(
            &db.connect().unwrap(),
            &AppSettings {
                recordings_directory: Some(prepared.to_string_lossy().into_owned()),
                ..Default::default()
            },
        )
        .unwrap();
        storage.set_recordings_root(prepared);
        let newer = create_meeting_with_directory(&db, &storage, &input).unwrap();
        let new_path = storage.get_microphone_path(&newer.id).unwrap();
        assert_eq!(new_path.parent().unwrap().parent().unwrap(), external);
        fs::write(&new_path, b"preserve new").unwrap();
        assert_eq!(storage.get_microphone_path(&old.id).unwrap(), old_path);
        let reopened_storage = StorageManager::initialize_in(&root).unwrap();
        let _reopened_db = Database::initialize(&reopened_storage).unwrap();
        assert_eq!(
            reopened_storage.get_microphone_path(&old.id).unwrap(),
            old_path
        );
        assert_eq!(
            reopened_storage.get_microphone_path(&newer.id).unwrap(),
            new_path
        );
        assert!(reopened_storage
            .export_text(&new_path.with_file_name("transcript.txt"), "overwrite")
            .is_err());
        reopened_storage.delete_meeting_files(&newer.id).unwrap();
        assert!(!new_path.exists());
        assert_eq!(fs::read(old_path).unwrap(), b"preserve old");
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn configured_root_rejects_network_and_device_namespaces_before_creating_directories() {
        for path in [
            r"\\server\share\recordings",
            r"\\?\UNC\server\share\recordings",
            r"\\.\C:\recordings",
        ] {
            assert!(super::validate_local_path(std::path::Path::new(path)).is_err());
        }
    }

    #[cfg(windows)]
    #[test]
    fn configured_root_rejects_nested_meeting_even_when_saved_path_has_different_case() {
        let root = std::env::temp_dir().join(format!(
            "settings-case-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let storage = StorageManager::initialize_in(&root).unwrap();
        let external = root.join("CustomRecordings");
        storage.set_recordings_root(storage.prepare_recordings_root(&external).unwrap());
        let meeting = storage.create_meeting_directory("old").unwrap();
        storage
            .restore_meeting_directory(
                "old",
                PathBuf::from(meeting.to_string_lossy().to_lowercase()),
            )
            .unwrap();
        assert!(storage
            .prepare_recordings_root(&meeting.join("nested"))
            .is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn configured_root_rejects_relative_internal_and_file_destinations() {
        let root = temp_root("invalid-configured-root");
        let storage = StorageManager::initialize_in(&root).unwrap();
        fs::write(root.join("file"), b"keep").unwrap();
        for path in [
            PathBuf::from("relative"),
            storage.root.clone(),
            storage.root.join("models"),
            storage.root.join("database"),
            storage.root.join("logs"),
            root.join("file"),
            root.join("external/../unsafe"),
        ] {
            assert!(
                storage.prepare_recordings_root(&path).is_err(),
                "{}",
                path.display()
            );
        }
        assert_eq!(fs::read(root.join("file")).unwrap(), b"keep");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn initializes_expected_directories_under_local_user_data() {
        let root = temp_root("structure");
        let storage = StorageManager::initialize_in(&root).unwrap();
        for directory in ["database", "recordings", "models", "logs"] {
            assert!(root.join("MeetingRecorder").join(directory).is_dir());
        }
        assert_eq!(
            storage.database_path(),
            root.join("MeetingRecorder/database/meeting-recorder.db")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn creates_distinct_meeting_directories() {
        let root = temp_root("distinct");
        let storage = StorageManager::initialize_in(&root).unwrap();
        let first = storage.create_meeting_directory("meeting-1").unwrap();
        let second = storage.create_meeting_directory("meeting-2").unwrap();

        assert_ne!(first, second);
        assert!(first.is_dir());
        assert!(second.is_dir());
        assert_eq!(
            storage.create_meeting_directory("meeting-1").unwrap(),
            first
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn generates_all_expected_paths_inside_meeting_directory() {
        let root = temp_root("paths");
        let storage = StorageManager::initialize_in(&root).unwrap();
        let base = root.join("MeetingRecorder/recordings/meeting-1");

        assert_eq!(
            storage.get_microphone_path("meeting-1").unwrap(),
            base.join("microphone.wav")
        );
        assert_eq!(
            storage.get_system_audio_path("meeting-1").unwrap(),
            base.join("system.wav")
        );
        assert_eq!(
            storage.get_merged_audio_path("meeting-1").unwrap(),
            base.join("merged.wav")
        );
        assert_eq!(
            storage.get_video_path("meeting-1").unwrap(),
            base.join("video.mp4")
        );
        assert_eq!(
            storage.get_transcript_path("meeting-1").unwrap(),
            base.join("transcript.txt")
        );
        assert!(!base.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn deletion_removes_target_files_and_preserves_other_meetings() {
        let root = temp_root("delete");
        let storage = StorageManager::initialize_in(&root).unwrap();
        let first = storage.create_meeting_directory("meeting-1").unwrap();
        let second = storage.create_meeting_directory("meeting-2").unwrap();
        for path in [
            storage.get_microphone_path("meeting-1").unwrap(),
            storage.get_system_audio_path("meeting-1").unwrap(),
            storage.get_merged_audio_path("meeting-1").unwrap(),
            storage.get_video_path("meeting-1").unwrap(),
            storage.get_transcript_path("meeting-1").unwrap(),
        ] {
            fs::write(path, b"synthetic").unwrap();
        }
        fs::write(second.join("keep.txt"), b"keep").unwrap();
        let outside = root.join("outside.txt");
        fs::write(&outside, b"keep").unwrap();

        assert!(storage.delete_meeting_files("meeting-1").unwrap());
        assert!(!first.exists());
        assert!(!storage.delete_meeting_files("meeting-1").unwrap());
        assert_eq!(fs::read(second.join("keep.txt")).unwrap(), b"keep");
        assert_eq!(fs::read(outside).unwrap(), b"keep");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_ids_that_could_escape_the_recordings_directory() {
        let root = temp_root("ids");
        let storage = StorageManager::initialize_in(&root).unwrap();
        let outside = root.join("outside.txt");
        fs::write(&outside, b"keep").unwrap();

        for id in ["", "..", "../outside", "a/b", r"C:\outside", "CON"] {
            assert!(storage.create_meeting_directory(id).is_err(), "{id}");
            assert!(storage.get_microphone_path(id).is_err(), "{id}");
            assert!(storage.delete_meeting_files(id).is_err(), "{id}");
        }
        assert_eq!(fs::read(outside).unwrap(), b"keep");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn refuses_to_delete_a_non_directory_meeting_path() {
        let root = temp_root("non-directory");
        let storage = StorageManager::initialize_in(&root).unwrap();
        let path = root.join("MeetingRecorder/recordings/meeting-1");
        fs::write(&path, b"keep").unwrap();

        assert!(storage.delete_meeting_files("meeting-1").is_err());
        assert_eq!(fs::read(path).unwrap(), b"keep");
        fs::remove_dir_all(root).unwrap();
    }
}
