//! Local, bounded diagnostics. Callers cannot pass arbitrary text fields.
use std::{
    collections::hash_map::DefaultHasher,
    fs::{self, OpenOptions},
    hash::{Hash, Hasher},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::{SystemTime, UNIX_EPOCH},
};

// One lock covers every producer, including video and independently reopened storage.
static WRITE_LOCK: Mutex<()> = Mutex::new(());
static WRITE_WARNING: AtomicBool = AtomicBool::new(false);
const MAX_BYTES: u64 = 1024 * 1024;

pub(crate) enum Value {
    Static(&'static str),
    Count(u64),
    Flag(bool),
    Opaque(u64),
}

impl Value {
    pub(crate) fn token(value: &str) -> Self {
        let mut hasher = DefaultHasher::new();
        value.hash(&mut hasher);
        Self::Opaque(hasher.finish())
    }

    pub(crate) fn status(status: &str) -> Self {
        Self::Static(match status {
            "starting" => "starting",
            "recording" => "recording",
            "completed" => "completed",
            "failed_partial" => "failed_partial",
            "failed" => "failed",
            "cancelled" => "cancelled",
            "processing" => "processing",
            _ => "unknown",
        })
    }
}

#[derive(Clone)]
pub(crate) struct DiagnosticLog {
    path: PathBuf,
    max_bytes: u64,
}

impl DiagnosticLog {
    pub(crate) fn new(path: PathBuf) -> Self {
        Self {
            path,
            max_bytes: MAX_BYTES,
        }
    }

    pub(crate) fn event(
        &self,
        level: &'static str,
        event: &'static str,
        fields: &[(&'static str, Value)],
    ) {
        if let Err(error) = self.write(level, event, fields) {
            // Never make logging a reason to stop capture; emit one sanitized fallback.
            if !WRITE_WARNING.swap(true, Ordering::Relaxed) {
                let _ = writeln!(
                    io::stderr().lock(),
                    "Local diagnostic log unavailable: {:?}",
                    error.kind()
                );
            }
        }
    }

    pub(crate) fn failure(&self, event: &'static str, meeting: Option<&str>, error: &str) {
        let lower = error.to_lowercase();
        let class = if lower.contains("cancel") {
            "cancelled"
        } else if lower.contains("panic") || lower.contains("worker") {
            "worker"
        } else if lower.contains("disconnect")
            || lower.contains("device")
            || lower.contains("dispositivo")
        {
            "device_unavailable"
        } else if lower.contains("permission")
            || lower.contains("access")
            || lower.contains("permiss")
        {
            "permission_denied"
        } else if lower.contains("file")
            || lower.contains("arquivo")
            || lower.contains("directory")
            || lower.contains("disk")
        {
            "file"
        } else if lower.contains("sqlite") || lower.contains("database") {
            "database"
        } else {
            "internal"
        };
        let mut fields = vec![("class", Value::Static(class))];
        if let Some(meeting) = meeting {
            fields.push(("meeting", Value::token(meeting)));
        }
        // Preserve only a numeric native code, never the message or process output.
        if let Some(code) = lower.split("0x").skip(1).find_map(|part| {
            let digits = part.get(..8)?;
            if part.as_bytes().get(8).is_some_and(u8::is_ascii_hexdigit) {
                return None;
            }
            u32::from_str_radix(digits, 16).ok()
        }) {
            fields.push(("native_code", Value::Opaque(u64::from(code))));
        }
        self.event("ERROR", event, &fields);
    }

    pub(crate) fn io_failure(&self, event: &'static str, error: &io::Error) {
        let kind = match error.kind() {
            io::ErrorKind::NotFound => "not_found",
            io::ErrorKind::PermissionDenied => "permission_denied",
            io::ErrorKind::AlreadyExists => "already_exists",
            io::ErrorKind::InvalidInput => "invalid_input",
            io::ErrorKind::InvalidData => "invalid_data",
            _ => "io",
        };
        let mut fields = vec![("class", Value::Static(kind))];
        if let Some(code) = error.raw_os_error() {
            fields.push(("os_code", Value::Count(code as u32 as u64)));
        }
        self.event("ERROR", event, &fields);
    }

    fn write(
        &self,
        level: &'static str,
        event: &'static str,
        fields: &[(&'static str, Value)],
    ) -> io::Result<()> {
        let atom = |text: &str| {
            !text.is_empty()
                && text
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c))
        };
        if !atom(level) || !atom(event) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let mut line = format!("timestamp_ms={timestamp} {level} {event}");
        for (key, value) in fields {
            if !atom(key) {
                return Err(io::ErrorKind::InvalidInput.into());
            }
            let text = match value {
                Value::Static(text) if atom(text) => (*text).to_owned(),
                Value::Static(_) => return Err(io::ErrorKind::InvalidInput.into()),
                Value::Count(value) => value.to_string(),
                Value::Flag(value) => value.to_string(),
                Value::Opaque(value) => format!("0x{value:08x}"),
            };
            line.push_str(&format!(" {key}={text}"));
        }
        line.push('\n');
        if line.len() as u64 > self.max_bytes {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let _guard = WRITE_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let current = regular_file_size(&self.path)?;
        if current > self.max_bytes {
            // Bound legacy unrotated logs too; media and database files are never touched.
            OpenOptions::new()
                .write(true)
                .open(&self.path)?
                .set_len(0)?;
        } else if current + line.len() as u64 > self.max_bytes {
            let oldest = self.path.with_extension("log.2");
            let previous = self.path.with_extension("log.1");
            regular_file_size(&oldest)?;
            regular_file_size(&previous)?;
            if oldest.exists() {
                fs::remove_file(&oldest)?;
            }
            if previous.exists() {
                fs::rename(&previous, &oldest)?;
            }
            fs::rename(&self.path, &previous)?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        file.write_all(line.as_bytes())?;
        file.flush()
    }
}

fn regular_file_size(path: &Path) -> io::Result<u64> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() && !super::is_link(&metadata) => Ok(metadata.len()),
        Ok(_) => Err(io::ErrorKind::InvalidInput.into()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs, thread,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn fixture() -> (std::path::PathBuf, DiagnosticLog) {
        let root = std::env::temp_dir().join(format!(
            "meeting-logs-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        let log = DiagnosticLog::new(root.join("meeting-recorder.log"));
        (root, log)
    }

    #[test]
    fn utf8_events_append_after_reopening_without_recording_error_content() {
        let (root, log) = fixture();
        log.write(
            "INFO",
            "application_start",
            &[("version", Value::Static("0.1.0"))],
        )
        .unwrap();
        let error = "WASAPI disconnected HRESULT 0x88890004 C:\\Users\\Pessoa\\segredo reunião.txt\nTRANSCRIÇÃO: ação confidencial";
        log.failure("wasapi_failed", Some("meeting-secret"), error);
        DiagnosticLog::new(log.path.clone())
            .write("INFO", "application_ready", &[])
            .unwrap();
        let text = fs::read_to_string(&log.path).unwrap();
        assert_eq!(text.lines().count(), 3);
        assert!(text.contains("application_start version=0.1.0"));
        assert!(text.contains("class=device_unavailable"));
        assert!(text.contains("native_code=0x88890004"));
        assert!(!text.contains("segredo"));
        assert!(!text.contains("TRANSCRIÇÃO"));
        assert!(!text.contains("meeting-secret"));
        assert!(text.lines().all(|line| line.starts_with("timestamp_ms=")));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rotation_bounds_total_size_and_retains_recent_events() {
        let (root, mut log) = fixture();
        log.max_bytes = 250;
        for _ in 0..30 {
            log.write("INFO", "recording_started", &[]).unwrap();
        }
        let entries: Vec<_> = fs::read_dir(&root)
            .unwrap()
            .map(|entry| entry.unwrap())
            .collect();
        assert_eq!(entries.len(), 3);
        assert!(entries
            .iter()
            .all(|entry| entry.metadata().unwrap().len() <= 250));
        assert!(fs::read_to_string(&log.path)
            .unwrap()
            .contains("recording_started"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn concurrent_writers_preserve_complete_lines() {
        let (root, log) = fixture();
        let workers: Vec<_> = (0..8)
            .map(|_| {
                let log = log.clone();
                thread::spawn(move || {
                    for _ in 0..20 {
                        log.write("INFO", "capture_finished", &[]).unwrap();
                    }
                })
            })
            .collect();
        for worker in workers {
            worker.join().unwrap();
        }
        let text = fs::read_to_string(&log.path).unwrap();
        assert_eq!(text.lines().count(), 160);
        assert!(text
            .lines()
            .all(|line| line.ends_with("INFO capture_finished")));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unavailable_log_destination_is_best_effort_and_can_recover() {
        let (root, log) = fixture();
        fs::create_dir(&log.path).unwrap();
        assert!(log.write("INFO", "application_ready", &[]).is_err());
        log.event("INFO", "recording_started", &[]);
        fs::remove_dir(&log.path).unwrap();
        log.write("INFO", "recording_finished", &[]).unwrap();
        assert!(fs::read_to_string(&log.path)
            .unwrap()
            .contains("recording_finished"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn oversized_record_does_not_exceed_file_limit() {
        let (root, mut log) = fixture();
        log.max_bytes = 128;
        assert_eq!(log.write("INFO","diagnostic",&[("value",Value::Static("abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz"))]).unwrap_err().kind(),std::io::ErrorKind::InvalidInput);
        assert!(!log.path.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn file_error_logs_only_kind_and_code_and_export_preserves_private_text() {
        let root = std::env::temp_dir().join(format!(
            "meeting-file-log-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let storage = crate::storage::StorageManager::initialize_in(&root).unwrap();
        let destination = root.join("private-folder-missing/private-title.txt");
        let text = "Transcrição privada: ação, coração.";
        assert!(storage.export_text(&destination, text).is_err());
        let log = storage.log();
        log.io_failure(
            "file_read_failed",
            &io::Error::new(
                io::ErrorKind::PermissionDenied,
                "private-title: conteúdo privado",
            ),
        );
        let contents = fs::read_to_string(storage.log_path()).unwrap();
        assert!(contents.contains("export_file_failed class=not_found"));
        assert!(contents.contains("file_read_failed class=permission_denied"));
        assert!(!contents.contains("private-title"));
        assert!(!contents.contains("Transcrição"));
        assert!(!destination.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn legacy_oversized_log_is_bounded_without_touching_other_files() {
        let (root, mut log) = fixture();
        log.max_bytes = 250;
        fs::write(&log.path, vec![b'x'; 1000]).unwrap();
        let media = root.join("microphone.wav");
        fs::write(&media, b"preserved media").unwrap();
        log.write("INFO", "application_start", &[]).unwrap();
        assert!(fs::metadata(&log.path).unwrap().len() <= 250);
        assert!(fs::read_to_string(&log.path)
            .unwrap()
            .contains("application_start"));
        assert_eq!(fs::read(&media).unwrap(), b"preserved media");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rotation_does_not_remove_directory_at_backup_destination() {
        let (root, mut log) = fixture();
        log.max_bytes = 128;
        log.write("INFO", "recording_started", &[]).unwrap();
        log.write("INFO", "recording_started", &[]).unwrap();
        let blocked = root.join("meeting-recorder.log.1");
        fs::create_dir(&blocked).unwrap();
        fs::write(blocked.join("preserve.txt"), b"preserved").unwrap();
        assert!(log.write("INFO", "recording_started", &[]).is_err());
        assert_eq!(
            fs::read(blocked.join("preserve.txt")).unwrap(),
            b"preserved"
        );
        assert_eq!(fs::read_to_string(&log.path).unwrap().lines().count(), 2);
        fs::remove_dir_all(root).unwrap();
    }
}
