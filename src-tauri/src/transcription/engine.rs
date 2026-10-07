use crate::storage::diagnostics::Value;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
};

use serde::Serialize;

use crate::{
    database::{meetings, segments, Database},
    storage::StorageManager,
};

use super::pipeline::SegmentProcessor;
use super::whisper::{TranscriptionFailure, TranscriptionRunner};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TranscriptionState {
    pub meeting_id: String,
    pub status: String,
    pub progress: u8,
    pub error: Option<String>,
    pub threads: usize,
    pub stage: String,
}

struct Job {
    state: Arc<Mutex<TranscriptionState>>,
    cancel: Arc<AtomicBool>,
    worker: JoinHandle<()>,
}

pub(crate) struct TranscriptionEngine {
    database: Database,
    storage: StorageManager,
    runner: Arc<dyn TranscriptionRunner>,
    processor: Option<Arc<dyn SegmentProcessor>>,
    job: Mutex<Option<Job>>,
}

impl TranscriptionEngine {
    pub(crate) fn shutdown(&self) -> Result<(), String> {
        let mut active = self.job.lock().map_err(|error| error.to_string())?;
        if let Some(job) = active.take() {
            job.cancel.store(true, Ordering::Release);
            job.worker
                .join()
                .map_err(|_| "Falha ao finalizar o worker de transcrição.".to_owned())?;
        }
        Ok(())
    }
    pub(crate) fn new(
        database: Database,
        storage: StorageManager,
        runner: Arc<dyn TranscriptionRunner>,
    ) -> Self {
        Self {
            database,
            storage,
            runner,
            processor: None,
            job: Mutex::new(None),
        }
    }

    pub(crate) fn with_processor(mut self, processor: Arc<dyn SegmentProcessor>) -> Self {
        self.processor = Some(processor);
        self
    }

    pub(crate) fn start_diarization(
        &self,
        id: &str,
        requested_threads: Option<usize>,
    ) -> Result<segments::DiarizationState, String> {
        let preferences =
            crate::database::settings::read(&self.database.connect().map_err(|e| e.to_string())?)?;
        let threads = preferences.settings.effective_threads(requested_threads)?;
        let processor = self.processor.clone().ok_or("Diarization is unavailable")?;
        let mut active = self.job.lock().map_err(|error| error.to_string())?;
        if active.as_ref().is_some_and(|job| !job.worker.is_finished()) {
            return Err("Another post-processing job is active".into());
        }
        if let Some(previous) = active.take() {
            previous
                .worker
                .join()
                .map_err(|_| "Post-processing worker panicked")?;
        }
        let connection = self.database.connect().map_err(|error| error.to_string())?;
        let mut meeting = meetings::get(&connection, id)
            .map_err(|error| error.to_string())?
            .ok_or("Meeting not found")?;
        if meeting.finished_at.is_none()
            || meeting.transcription_status != "completed"
            || !matches!(meeting.status.as_str(), "completed" | "failed_partial")
        {
            return Err(
                "Finalize e transcreva a reunião antes de identificar participantes.".into(),
            );
        }
        if segments::read(&connection, id)
            .map_err(|error| error.to_string())?
            .status
            == "completed"
        {
            return Err("Esta reunião já possui segmentos identificados.".into());
        }
        let runner = self
            .runner
            .configured(
                &preferences.settings.transcription_model,
                &self.storage.models_path(),
            )?
            .unwrap_or_else(|| self.runner.clone());
        // Only this new inference uses the current language; the traditional transcript keeps its metadata.
        if preferences.customized {
            meeting.language = preferences.settings.language;
        }
        segments::update_state(&connection, id, "processing", "preparing_sources", 0, None)
            .map_err(|error| error.to_string())?;
        let initial = segments::read(&connection, id).map_err(|error| error.to_string())?;
        let state = Arc::new(Mutex::new(TranscriptionState {
            meeting_id: id.into(),
            status: "processing".into(),
            progress: 0,
            error: None,
            threads,
            stage: "preparing_sources".into(),
        }));
        let cancel = Arc::new(AtomicBool::new(false));
        let context = super::diarization_job::Context {
            database: self.database.clone(),
            storage: self.storage.clone(),
            processor,
            runner,
            meeting,
            threads,
            cancel: cancel.clone(),
            state: state.clone(),
            progress_base: 0,
        };
        let worker = thread::spawn(move || context.run());
        *active = Some(Job {
            state,
            cancel,
            worker,
        });
        Ok(initial)
    }

    pub(crate) fn recover_interrupted(&self) -> Result<(), String> {
        let connection = self.database.connect().map_err(|error| error.to_string())?;
        for mut meeting in meetings::list(&connection).map_err(|error| error.to_string())? {
            let diarization =
                segments::read(&connection, &meeting.id).map_err(|error| error.to_string())?;
            if matches!(diarization.status.as_str(), "processing" | "pending") {
                segments::update_state(&connection,&meeting.id,"failed","interrupted",0,Some("Diarização interrompida. A transcrição tradicional foi preservada; tente novamente.")).map_err(|error|error.to_string())?;
            }
            if meeting.transcription_status == "processing" {
                let recovered =
                    self.storage
                        .get_transcript_path(&meeting.id)
                        .and_then(|transcript| {
                            let _ = fs::remove_file(transcript.with_extension("txt.tmp"));
                            let _ =
                                fs::remove_file(transcript.with_file_name("whisper-output.txt"));
                            fs::read_to_string(transcript)
                        });
                match recovered {
                    Ok(text) => {
                        meeting.transcription = Some(text);
                        meeting.transcription_status = "completed".to_owned();
                    }
                    Err(error) => {
                        // An unavailable drive is a failure of this meeting, not of application startup.
                        meeting.transcription_status = if meeting
                            .transcription
                            .as_ref()
                            .is_some_and(|text| !text.trim().is_empty())
                        {
                            "completed"
                        } else {
                            "failed"
                        }
                        .into();
                        self.storage
                            .log()
                            .io_failure("transcript_recovery_file_failed", &error);
                    }
                }
                meeting.status = meeting
                    .recording_status
                    .take()
                    .unwrap_or_else(|| "failed_partial".to_owned());
                meetings::update(&connection, &meeting).map_err(|error| error.to_string())?;
            }
        }
        Ok(())
    }

    pub(crate) fn is_processing(&self) -> Result<bool, String> {
        let job = self.job.lock().map_err(|error| error.to_string())?;
        Ok(job.as_ref().is_some_and(|job| !job.worker.is_finished()))
    }

    pub(crate) fn is_processing_meeting(&self, id: &str) -> Result<bool, String> {
        let job = self.job.lock().map_err(|error| error.to_string())?;
        Ok(job.as_ref().is_some_and(|job| {
            !job.worker.is_finished() && job.state.lock().is_ok_and(|state| state.meeting_id == id)
        }))
    }

    pub(crate) fn start(
        &self,
        id: &str,
        requested_threads: Option<usize>,
    ) -> Result<TranscriptionState, String> {
        self.start_inner(id, requested_threads)
            .inspect_err(|error| {
                self.storage
                    .log()
                    .failure("transcription_start_failed", Some(id), error)
            })
    }

    fn start_inner(
        &self,
        id: &str,
        requested_threads: Option<usize>,
    ) -> Result<TranscriptionState, String> {
        let preferences =
            crate::database::settings::read(&self.database.connect().map_err(|e| e.to_string())?)?;
        let threads = preferences.settings.effective_threads(requested_threads)?;
        let mut active = self.job.lock().map_err(|error| error.to_string())?;
        if active.as_ref().is_some_and(|job| !job.worker.is_finished()) {
            return Err("Another transcription is processing".to_owned());
        }
        if let Some(previous) = active.take() {
            previous
                .worker
                .join()
                .map_err(|_| "Transcription worker panicked".to_owned())?;
        }
        let connection = self.database.connect().map_err(|error| error.to_string())?;
        let mut meeting = meetings::get(&connection, id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "Meeting not found".to_owned())?;
        if meeting.finished_at.is_none()
            || !matches!(meeting.status.as_str(), "completed" | "failed_partial")
        {
            return Err("A meeting must finish recording before transcription".to_owned());
        }
        if meeting.transcription_status == "completed" {
            return Err("This meeting has already been transcribed".to_owned());
        }
        self.storage
            .create_meeting_directory(id)
            .map_err(|error| error.to_string())?;
        let merged = self
            .storage
            .get_merged_audio_path(id)
            .map_err(|error| error.to_string())?;
        if meeting.merged_audio_path.as_deref() != Some(merged.to_string_lossy().as_ref())
            || !merged.is_file()
        {
            return Err("Prepared meeting audio is unavailable".to_owned());
        }
        let transcript = self
            .storage
            .get_transcript_path(id)
            .map_err(|error| error.to_string())?;
        if transcript.exists() {
            return Err("An existing transcript file must be preserved".to_owned());
        }
        let configured_runner = self
            .runner
            .configured(
                &preferences.settings.transcription_model,
                &self.storage.models_path(),
            )?
            .unwrap_or_else(|| self.runner.clone());
        meeting.recording_status = Some(meeting.status.clone());
        meeting.status = "processing".to_owned();
        meeting.transcription_status = "processing".to_owned();
        meeting.transcription_model =
            Some(crate::settings::model_name(&preferences.settings.transcription_model).to_owned());
        if preferences.customized {
            meeting.language = preferences.settings.language;
        }
        let language = meeting.language.clone();
        meetings::update(&connection, &meeting).map_err(|error| error.to_string())?;
        if self.processor.is_some() {
            segments::update_state(
                &connection,
                id,
                "pending",
                "waiting_for_transcript",
                0,
                None,
            )
            .map_err(|error| error.to_string())?;
        }
        drop(connection);
        let state = Arc::new(Mutex::new(TranscriptionState {
            meeting_id: id.to_owned(),
            status: "processing".to_owned(),
            progress: 0,
            error: None,
            threads,
            stage: "transcription".into(),
        }));
        let cancel = Arc::new(AtomicBool::new(false));
        let output_base = transcript.with_file_name("whisper-output");
        let worker = {
            let database = self.database.clone();
            let runner = configured_runner;
            let processor = self.processor.clone();
            let storage = self.storage.clone();
            let state = state.clone();
            let cancel = cancel.clone();
            let id = id.to_owned();
            thread::spawn(move || {
                storage.log().event(
                    "INFO",
                    "transcription_started",
                    &[
                        ("meeting", Value::token(&id)),
                        ("threads", Value::Count(threads as u64)),
                    ],
                );
                let mut progress = |percent: u8| {
                    if let Ok(mut current) = state.lock() {
                        let percent = if processor.is_some() {
                            percent / 2
                        } else {
                            percent.min(99)
                        };
                        current.progress = current.progress.max(percent);
                    }
                };
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    runner.run(
                        &merged,
                        &output_base,
                        &language,
                        threads,
                        &cancel,
                        &mut progress,
                    )
                }))
                .unwrap_or_else(|_| {
                    Err(TranscriptionFailure::Failed(
                        "Falha interna na transcrição. O áudio foi preservado; tente novamente."
                            .into(),
                    ))
                });
                if let Err(TranscriptionFailure::Failed(error)) = &result {
                    storage.log().failure("whisper_failed", Some(&id), error);
                }
                let result = match result {
                    Ok(_) if cancel.load(Ordering::Acquire) => Err(TranscriptionFailure::Cancelled),
                    Ok(text) => save_transcript(&transcript, &text)
                        .inspect_err(|error| {
                            storage
                                .log()
                                .failure("transcript_file_failed", Some(&id), error)
                        })
                        .map(|()| text)
                        .map_err(TranscriptionFailure::Failed),
                    Err(error) => Err(error),
                };
                let result = persist_result(&database, &id, result);
                let status = match &result {
                    Ok(()) => "completed",
                    Err(TranscriptionFailure::Cancelled) => "cancelled",
                    Err(TranscriptionFailure::Failed(_)) => "failed",
                };
                storage.log().event(
                    "INFO",
                    "transcription_finished",
                    &[
                        ("meeting", Value::token(&id)),
                        ("status", Value::status(status)),
                    ],
                );
                if let Err(TranscriptionFailure::Failed(error)) = &result {
                    storage
                        .log()
                        .failure("transcription_failed", Some(&id), error);
                }
                if result.is_ok() {
                    if let Some(processor) = processor {
                        super::diarization_job::Context {
                            database,
                            storage,
                            processor,
                            runner,
                            meeting,
                            threads,
                            cancel,
                            state,
                            progress_base: 50,
                        }
                        .run();
                        return;
                    }
                } else if processor.is_some() {
                    if let Ok(connection) = database.connect() {
                        let _ = segments::update_state(
                            &connection,
                            &id,
                            "failed",
                            "transcription_failed",
                            0,
                            Some("A transcrição não terminou; não foi iniciada a diarização."),
                        );
                    }
                }
                if let Ok(mut current) = state.lock() {
                    match result {
                        Ok(()) => {
                            current.status = "completed".to_owned();
                            current.progress = 100;
                            current.stage = "completed".into();
                        }
                        Err(TranscriptionFailure::Cancelled) => {
                            current.status = "cancelled".to_owned()
                        }
                        Err(TranscriptionFailure::Failed(error)) => {
                            current.status = "failed".to_owned();
                            current.error = Some(error);
                        }
                    }
                }
            })
        };
        let initial = state.lock().map_err(|error| error.to_string())?.clone();
        *active = Some(Job {
            state,
            cancel,
            worker,
        });
        Ok(initial)
    }

    pub(crate) fn cancel(&self, id: &str) -> Result<TranscriptionState, String> {
        let active = self.job.lock().map_err(|error| error.to_string())?;
        let job = active
            .as_ref()
            .ok_or_else(|| "No transcription is processing".to_owned())?;
        let state = job.state.lock().map_err(|error| error.to_string())?.clone();
        if job.worker.is_finished() || state.meeting_id != id {
            return Err("No transcription is processing for this meeting".to_owned());
        }
        job.cancel.store(true, Ordering::Release);
        Ok(state)
    }

    pub(crate) fn get_state(&self, id: &str) -> Result<TranscriptionState, String> {
        let active = self.job.lock().map_err(|error| error.to_string())?;
        if let Some(job) = active.as_ref() {
            let state = job.state.lock().map_err(|error| error.to_string())?;
            if state.meeting_id == id {
                return Ok(state.clone());
            }
        }
        drop(active);
        let connection = self.database.connect().map_err(|error| error.to_string())?;
        let meeting = meetings::get(&connection, id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "Meeting not found".to_owned())?;
        Ok(TranscriptionState {
            meeting_id: id.to_owned(),
            progress: if meeting.transcription_status == "completed" {
                100
            } else {
                0
            },
            status: meeting.transcription_status,
            error: None,
            threads: 0,
            stage: "idle".into(),
        })
    }
}

impl Drop for TranscriptionEngine {
    fn drop(&mut self) {
        if let Ok(mut active) = self.job.lock() {
            if let Some(job) = active.take() {
                job.cancel.store(true, Ordering::Release);
                let _ = job.worker.join();
            }
        }
    }
}

#[cfg(test)]
fn thread_limit(requested: Option<usize>) -> Result<usize, String> {
    let available = thread::available_parallelism().map_or(2, |value| value.get());
    let default = (available / 2).clamp(1, 4);
    match requested {
        Some(0) => Err("Thread count must be at least one".to_owned()),
        Some(value) => Ok(value.min(default)),
        None => Ok(default),
    }
}

fn save_transcript(path: &Path, text: &str) -> Result<(), String> {
    let temp = path.with_extension("txt.tmp");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(|error| error.to_string())?;
    if let Err(error) = file
        .write_all(text.as_bytes())
        .and_then(|()| file.sync_all())
    {
        let _ = fs::remove_file(&temp);
        return Err(error.to_string());
    }
    drop(file);
    if path.exists() {
        let _ = fs::remove_file(&temp);
        return Err("Transcript already exists".to_owned());
    }
    fs::rename(&temp, path).map_err(|error| {
        let _ = fs::remove_file(&temp);
        error.to_string()
    })
}

fn persist_result(
    database: &Database,
    id: &str,
    result: Result<String, TranscriptionFailure>,
) -> Result<(), TranscriptionFailure> {
    let connection = database
        .connect()
        .map_err(|error| TranscriptionFailure::Failed(error.to_string()))?;
    let mut meeting = meetings::get(&connection, id)
        .map_err(|error| TranscriptionFailure::Failed(error.to_string()))?
        .ok_or_else(|| TranscriptionFailure::Failed("Meeting disappeared".to_owned()))?;
    meeting.status = meeting
        .recording_status
        .take()
        .unwrap_or_else(|| "failed_partial".to_owned());
    meeting.transcription_status = match &result {
        Ok(text) => {
            meeting.transcription = Some(text.clone());
            "completed"
        }
        Err(TranscriptionFailure::Cancelled) => "cancelled",
        Err(TranscriptionFailure::Failed(_)) => "failed",
    }
    .to_owned();
    meetings::update(&connection, &meeting)
        .map_err(|error| TranscriptionFailure::Failed(error.to_string()))?;
    result.map(|_| ())
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc, Mutex,
        },
        thread,
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    use super::{thread_limit, TranscriptionEngine, TranscriptionFailure, TranscriptionRunner};
    use crate::transcription::WhisperCli;
    use crate::{
        database::{meetings, Database, NewMeeting},
        storage::StorageManager,
    };

    fn root() -> PathBuf {
        std::env::temp_dir().join(format!(
            "meeting-recorder-transcription-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn prepared() -> (PathBuf, StorageManager, Database, String) {
        let root = root();
        let storage = StorageManager::initialize_in(&root).unwrap();
        let database = Database::initialize(&storage).unwrap();
        let connection = database.connect().unwrap();
        let mut meeting = meetings::create(
            &connection,
            &NewMeeting {
                title: "Teste".to_owned(),
                microphone_enabled: true,
                system_audio_enabled: false,
                video_enabled: false,
            },
        )
        .unwrap();
        storage.create_meeting_directory(&meeting.id).unwrap();
        let audio = storage.get_merged_audio_path(&meeting.id).unwrap();
        fs::copy(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/jfk.wav"),
            &audio,
        )
        .unwrap();
        meeting.merged_audio_path = Some(audio.to_string_lossy().into_owned());
        meeting.finished_at = Some("2026-09-30T12:00:00Z".to_owned());
        meeting.status = "completed".to_owned();
        meetings::update(&connection, &meeting).unwrap();
        (root, storage, database, meeting.id)
    }

    struct FakeRunner {
        seen: Arc<Mutex<Vec<(String, usize)>>>,
        wait_cancel: bool,
        fail: bool,
    }
    impl TranscriptionRunner for FakeRunner {
        fn run(
            &self,
            _input: &Path,
            _output_base: &Path,
            language: &str,
            threads: usize,
            cancel: &AtomicBool,
            progress: &mut dyn FnMut(u8),
        ) -> Result<String, TranscriptionFailure> {
            self.seen
                .lock()
                .unwrap()
                .push((language.to_owned(), threads));
            progress(42);
            if self.wait_cancel {
                while !cancel.load(Ordering::Acquire) {
                    thread::sleep(Duration::from_millis(10));
                }
                return Err(TranscriptionFailure::Cancelled);
            }
            if self.fail {
                return Err(TranscriptionFailure::Failed(
                    "simulated model failure".to_owned(),
                ));
            }
            Ok("Transcrição local curta".to_owned())
        }
    }

    fn wait_finished(engine: &TranscriptionEngine, id: &str) -> String {
        for _ in 0..3000 {
            let state = engine.get_state(id).unwrap();
            if state.status != "processing" {
                return state.status;
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("transcription did not finish");
    }

    #[test]
    fn panicking_transcription_preserves_audio_marks_failure_and_allows_retry() {
        struct PanicOnce(AtomicBool);
        impl TranscriptionRunner for PanicOnce {
            fn run(
                &self,
                _: &Path,
                _: &Path,
                _: &str,
                _: usize,
                _: &AtomicBool,
                _: &mut dyn FnMut(u8),
            ) -> Result<String, TranscriptionFailure> {
                if !self.0.swap(true, Ordering::AcqRel) {
                    panic!("simulated transcription panic");
                }
                Ok("Recuperação após falha".into())
            }
        }
        let (root, storage, database, id) = prepared();
        let audio = storage.get_merged_audio_path(&id).unwrap();
        let original = fs::read(&audio).unwrap();
        let engine = TranscriptionEngine::new(
            database.clone(),
            storage.clone(),
            Arc::new(PanicOnce(AtomicBool::new(false))),
        );
        engine.start(&id, None).unwrap();
        for _ in 0..300 {
            if !engine.is_processing().unwrap() {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        assert!(!engine.is_processing().unwrap());
        assert_eq!(engine.get_state(&id).unwrap().status, "failed");
        let saved = meetings::get(&database.connect().unwrap(), &id)
            .unwrap()
            .unwrap();
        assert_eq!(saved.transcription_status, "failed");
        assert_eq!(saved.status, "completed");
        assert_eq!(fs::read(&audio).unwrap(), original);
        assert!(!storage.get_transcript_path(&id).unwrap().exists());
        engine.start(&id, None).unwrap();
        assert_eq!(wait_finished(&engine, &id), "completed");
        engine.shutdown().unwrap();
        drop(engine);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn diagnostics_record_transcription_outcomes_without_text() {
        for (fail, cancel) in [(false, false), (true, false), (false, true)] {
            let (root, storage, database, id) = prepared();
            let engine = TranscriptionEngine::new(
                database,
                storage.clone(),
                Arc::new(FakeRunner {
                    seen: Arc::new(Mutex::new(Vec::new())),
                    wait_cancel: cancel,
                    fail,
                }),
            );
            engine.start(&id, None).unwrap();
            if cancel {
                engine.cancel(&id).unwrap();
            }
            let status = wait_finished(&engine, &id);
            assert_eq!(
                status,
                if cancel {
                    "cancelled"
                } else if fail {
                    "failed"
                } else {
                    "completed"
                }
            );
            engine.shutdown().unwrap();
            let log = fs::read_to_string(storage.log_path()).unwrap();
            assert!(log.contains("transcription_started"));
            assert!(log.contains("transcription_finished"));
            assert!(log.contains(&format!("status={status}")));
            if fail {
                assert!(log.contains("whisper_failed"));
            }
            assert!(!log.contains("Transcrição local curta"));
            assert!(!log.contains("simulated model failure"));
            drop(engine);
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn explicit_diarization_uses_current_language_and_threads_without_changing_traditional_metadata(
    ) {
        struct SeenProcessor(Arc<Mutex<Option<(String, usize)>>>);
        impl super::SegmentProcessor for SeenProcessor {
            fn run(
                &self,
                storage: &StorageManager,
                meeting: &crate::database::Meeting,
                runner: &dyn TranscriptionRunner,
                threads: usize,
                cancel: &AtomicBool,
                progress: &mut dyn FnMut(&str, u8),
            ) -> Result<Vec<crate::database::segments::NewSegment>, TranscriptionFailure>
            {
                *self.0.lock().unwrap() = Some((meeting.language.clone(), threads));
                FakeProcessor { fail: false }
                    .run(storage, meeting, runner, threads, cancel, progress)
            }
        }
        let (root, storage, database, id) = prepared();
        let conn = database.connect().unwrap();
        let mut meeting = meetings::get(&conn, &id).unwrap().unwrap();
        meeting.transcription_status = "completed".into();
        meeting.transcription = Some("Texto tradicional original".into());
        meetings::update(&conn, &meeting).unwrap();
        fs::write(
            storage.get_transcript_path(&id).unwrap(),
            "Texto tradicional original",
        )
        .unwrap();
        crate::database::settings::save(
            &conn,
            &crate::settings::AppSettings {
                language: "en".into(),
                max_threads: 1,
                ..Default::default()
            },
        )
        .unwrap();
        let seen = Arc::new(Mutex::new(None));
        let engine = TranscriptionEngine::new(
            database.clone(),
            storage,
            Arc::new(FakeRunner {
                seen: Arc::new(Mutex::new(vec![])),
                wait_cancel: false,
                fail: false,
            }),
        )
        .with_processor(Arc::new(SeenProcessor(seen.clone())));
        engine.start_diarization(&id, Some(16)).unwrap();
        assert_eq!(wait_finished(&engine, &id), "completed");
        assert_eq!(*seen.lock().unwrap(), Some(("en".into(), 1)));
        assert_eq!(meetings::get(&conn, &id).unwrap().unwrap().language, "pt");
        assert_eq!(
            meetings::get(&conn, &id).unwrap().unwrap().transcription,
            meeting.transcription
        );
        drop(engine);
        drop(conn);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn recovery_with_unavailable_recording_root_preserves_text_and_does_not_abort_startup() {
        let (root, storage, database, id) = prepared();
        let conn = database.connect().unwrap();
        let mut meeting = meetings::get(&conn, &id).unwrap().unwrap();
        meeting.transcription_status = "processing".into();
        meeting.status = "processing".into();
        meeting.recording_status = Some("completed".into());
        meeting.transcription = Some("Texto já preservado no SQLite".into());
        meetings::update(&conn, &meeting).unwrap();
        let unavailable = root.join("unavailable");
        fs::write(&unavailable, b"unavailable directory").unwrap();
        storage
            .restore_meeting_directory(&id, unavailable.join(&id))
            .unwrap();
        let engine = TranscriptionEngine::new(
            database.clone(),
            storage,
            Arc::new(FakeRunner {
                seen: Arc::new(Mutex::new(vec![])),
                wait_cancel: false,
                fail: false,
            }),
        );
        engine.recover_interrupted().unwrap();
        let recovered = meetings::get(&conn, &id).unwrap().unwrap();
        assert_eq!(recovered.status, "completed");
        assert_eq!(recovered.transcription_status, "completed");
        assert_eq!(recovered.transcription, meeting.transcription);
        assert_eq!(recovered.merged_audio_path, meeting.merged_audio_path);
        assert!(Path::new(meeting.merged_audio_path.as_ref().unwrap()).is_file());
        meeting.transcription = None;
        meetings::update(&conn, &meeting).unwrap();
        engine.recover_interrupted().unwrap();
        let without_text = meetings::get(&conn, &id).unwrap().unwrap();
        assert_eq!(without_text.transcription_status, "failed");
        assert_eq!(without_text.status, "completed");
        drop(conn);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn saved_transcription_preferences_control_language_threads_and_persist_model() {
        let (root, storage, database, id) = prepared();
        crate::database::settings::save(
            &database.connect().unwrap(),
            &crate::settings::AppSettings {
                language: "es".into(),
                max_threads: 1,
                transcription_model: "tiny-q5_1".into(),
                ..Default::default()
            },
        )
        .unwrap();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let engine = TranscriptionEngine::new(
            database.clone(),
            storage,
            Arc::new(FakeRunner {
                seen: seen.clone(),
                wait_cancel: false,
                fail: false,
            }),
        );
        assert_eq!(engine.start(&id, Some(16)).unwrap().threads, 1);
        assert_eq!(wait_finished(&engine, &id), "completed");
        assert_eq!(seen.lock().unwrap().as_slice(), &[("es".into(), 1)]);
        let meeting = meetings::get(&database.connect().unwrap(), &id)
            .unwrap()
            .unwrap();
        assert_eq!(meeting.language, "es");
        assert_eq!(
            meeting.transcription_model.as_deref(),
            Some("Tiny Multilingual Q5")
        );
        drop(engine);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn short_audio_saves_sqlite_and_text_after_recording() {
        let (root, storage, database, id) = prepared();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let engine = TranscriptionEngine::new(
            database.clone(),
            storage.clone(),
            Arc::new(FakeRunner {
                seen: seen.clone(),
                wait_cancel: false,
                fail: false,
            }),
        );
        assert_eq!(engine.start(&id, Some(2)).unwrap().status, "processing");
        assert_eq!(wait_finished(&engine, &id), "completed");
        let saved = meetings::get(&database.connect().unwrap(), &id)
            .unwrap()
            .unwrap();
        assert_eq!(
            saved.transcription.as_deref(),
            Some("Transcrição local curta")
        );
        assert_eq!(saved.transcription_status, "completed");
        assert_eq!(saved.status, "completed");
        assert_eq!(
            saved.transcription_model.as_deref(),
            Some("Base Multilingual Q5")
        );
        assert_eq!(
            fs::read_to_string(storage.get_transcript_path(&id).unwrap()).unwrap(),
            "Transcrição local curta"
        );
        assert_eq!(
            seen.lock().unwrap().as_slice(),
            &[("pt".to_owned(), thread_limit(Some(2)).unwrap())]
        );
        drop(engine);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn shutdown_cancels_and_joins_processing_before_returning() {
        let (root, storage, database, id) = prepared();
        let engine = TranscriptionEngine::new(
            database.clone(),
            storage.clone(),
            Arc::new(FakeRunner {
                seen: Arc::new(Mutex::new(Vec::new())),
                wait_cancel: true,
                fail: false,
            }),
        );
        engine.start(&id, None).unwrap();
        engine.shutdown().unwrap();
        assert!(!engine.is_processing().unwrap());
        assert_eq!(
            meetings::get(&database.connect().unwrap(), &id)
                .unwrap()
                .unwrap()
                .transcription_status,
            "cancelled"
        );
        assert!(storage.get_merged_audio_path(&id).unwrap().is_file());
        drop(engine);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cancel_preserves_audio_and_marks_database() {
        let (root, storage, database, id) = prepared();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let engine = TranscriptionEngine::new(
            database.clone(),
            storage.clone(),
            Arc::new(FakeRunner {
                seen,
                wait_cancel: true,
                fail: false,
            }),
        );
        engine.start(&id, None).unwrap();
        engine.cancel(&id).unwrap();
        assert_eq!(wait_finished(&engine, &id), "cancelled");
        assert!(storage.get_merged_audio_path(&id).unwrap().exists());
        assert!(!storage.get_transcript_path(&id).unwrap().exists());
        let saved = meetings::get(&database.connect().unwrap(), &id)
            .unwrap()
            .unwrap();
        assert_eq!(saved.transcription_status, "cancelled");
        assert_eq!(saved.status, "completed");
        drop(engine);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_transcription_preserves_audio_and_can_be_retried() {
        let (root, storage, database, id) = prepared();
        let failed = TranscriptionEngine::new(
            database.clone(),
            storage.clone(),
            Arc::new(FakeRunner {
                seen: Arc::new(Mutex::new(Vec::new())),
                wait_cancel: false,
                fail: true,
            }),
        );
        failed.start(&id, None).unwrap();
        assert_eq!(wait_finished(&failed, &id), "failed");
        assert!(storage.get_merged_audio_path(&id).unwrap().exists());
        assert!(!storage.get_transcript_path(&id).unwrap().exists());
        assert_eq!(
            meetings::get(&database.connect().unwrap(), &id)
                .unwrap()
                .unwrap()
                .transcription_status,
            "failed"
        );
        drop(failed);
        let retry = TranscriptionEngine::new(
            database.clone(),
            storage.clone(),
            Arc::new(FakeRunner {
                seen: Arc::new(Mutex::new(Vec::new())),
                wait_cancel: false,
                fail: false,
            }),
        );
        retry.start(&id, Some(usize::MAX)).unwrap();
        assert_eq!(wait_finished(&retry, &id), "completed");
        assert_eq!(
            fs::read_to_string(storage.get_transcript_path(&id).unwrap()).unwrap(),
            "Transcrição local curta"
        );
        drop(retry);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn transcription_keeps_partial_recording_status_and_valid_audio() {
        let (root, storage, database, id) = prepared();
        let connection = database.connect().unwrap();
        let mut meeting = meetings::get(&connection, &id).unwrap().unwrap();
        meeting.status = "failed_partial".to_owned();
        meetings::update(&connection, &meeting).unwrap();
        drop(connection);
        let engine = TranscriptionEngine::new(
            database.clone(),
            storage.clone(),
            Arc::new(FakeRunner {
                seen: Arc::new(Mutex::new(Vec::new())),
                wait_cancel: false,
                fail: false,
            }),
        );
        engine.start(&id, None).unwrap();
        assert_eq!(wait_finished(&engine, &id), "completed");
        let saved = meetings::get(&database.connect().unwrap(), &id)
            .unwrap()
            .unwrap();
        assert_eq!(saved.status, "failed_partial");
        assert_eq!(saved.transcription_status, "completed");
        assert!(storage.get_merged_audio_path(&id).unwrap().exists());
        drop(engine);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn diarization_failure_preserves_completed_transcript_and_audio() {
        let (root, storage, database, id) = prepared();
        let engine = TranscriptionEngine::new(
            database.clone(),
            storage.clone(),
            Arc::new(FakeRunner {
                seen: Arc::new(Mutex::new(Vec::new())),
                wait_cancel: false,
                fail: false,
            }),
        )
        .with_processor(Arc::new(FakeProcessor { fail: true }));
        engine.start(&id, None).unwrap();
        assert_eq!(wait_finished(&engine, &id), "completed");
        let connection = database.connect().unwrap();
        assert_eq!(
            crate::database::segments::read(&connection, &id)
                .unwrap()
                .status,
            "failed"
        );
        assert_eq!(
            meetings::get(&connection, &id)
                .unwrap()
                .unwrap()
                .transcription_status,
            "completed"
        );
        assert_eq!(
            fs::read_to_string(storage.get_transcript_path(&id).unwrap()).unwrap(),
            "Transcrição local curta"
        );
        assert!(storage.get_merged_audio_path(&id).unwrap().exists());
        drop(connection);
        drop(engine);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn segments_are_persisted_after_text_and_old_meeting_can_be_processed_explicitly() {
        let (root, storage, database, id) = prepared();
        let engine = TranscriptionEngine::new(
            database.clone(),
            storage.clone(),
            Arc::new(FakeRunner {
                seen: Arc::new(Mutex::new(Vec::new())),
                wait_cancel: false,
                fail: false,
            }),
        )
        .with_processor(Arc::new(FakeProcessor { fail: false }));
        engine.start(&id, None).unwrap();
        assert_eq!(wait_finished(&engine, &id), "completed");
        assert_eq!(
            crate::database::segments::read(&database.connect().unwrap(), &id)
                .unwrap()
                .segments[0]
                .diarization_label,
            "Você"
        );
        drop(engine);
        let connection = database.connect().unwrap();
        connection
            .execute("DELETE FROM transcript_segments WHERE meeting_id=?1", [&id])
            .unwrap();
        connection
            .execute("DELETE FROM meeting_diarization WHERE meeting_id=?1", [&id])
            .unwrap();
        drop(connection);
        let engine = TranscriptionEngine::new(
            database.clone(),
            storage.clone(),
            Arc::new(FakeRunner {
                seen: Arc::new(Mutex::new(Vec::new())),
                wait_cancel: false,
                fail: true,
            }),
        )
        .with_processor(Arc::new(FakeProcessor { fail: false }));
        engine.start_diarization(&id, None).unwrap();
        assert_eq!(wait_finished(&engine, &id), "completed");
        assert_eq!(
            crate::database::segments::read(&database.connect().unwrap(), &id)
                .unwrap()
                .segments
                .len(),
            1
        );
        drop(engine);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cancellation_of_diarization_and_restart_keep_completed_text() {
        struct WaitingProcessor;
        impl super::SegmentProcessor for WaitingProcessor {
            fn run(
                &self,
                _storage: &StorageManager,
                _meeting: &crate::database::Meeting,
                _runner: &dyn TranscriptionRunner,
                _threads: usize,
                cancel: &AtomicBool,
                progress: &mut dyn FnMut(&str, u8),
            ) -> Result<Vec<crate::database::segments::NewSegment>, TranscriptionFailure>
            {
                progress("diarization", 25);
                while !cancel.load(Ordering::Acquire) {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(TranscriptionFailure::Cancelled)
            }
        }
        let (root, storage, database, id) = prepared();
        let engine = TranscriptionEngine::new(
            database.clone(),
            storage.clone(),
            Arc::new(FakeRunner {
                seen: Arc::new(Mutex::new(Vec::new())),
                wait_cancel: false,
                fail: false,
            }),
        )
        .with_processor(Arc::new(WaitingProcessor));
        engine.start(&id, None).unwrap();
        for _ in 0..1000 {
            if engine.get_state(&id).unwrap().stage == "diarization" {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(engine.get_state(&id).unwrap().stage, "diarization");
        assert!(engine.is_processing().unwrap());
        assert!(engine.start_diarization(&id, None).is_err());
        engine.cancel(&id).unwrap();
        assert_eq!(wait_finished(&engine, &id), "completed");
        drop(engine);
        let reopened = Database::initialize(&storage).unwrap();
        let connection = reopened.connect().unwrap();
        assert_eq!(
            crate::database::segments::read(&connection, &id)
                .unwrap()
                .status,
            "cancelled"
        );
        let meeting = meetings::get(&connection, &id).unwrap().unwrap();
        assert_eq!(meeting.transcription_status, "completed");
        assert_eq!(
            meeting.transcription.as_deref(),
            Some("Transcrição local curta")
        );
        crate::database::segments::update_state(
            &connection,
            &id,
            "processing",
            "diarization",
            50,
            None,
        )
        .unwrap();
        drop(connection);
        let recovered = TranscriptionEngine::new(
            reopened.clone(),
            storage.clone(),
            Arc::new(FakeRunner {
                seen: Arc::new(Mutex::new(Vec::new())),
                wait_cancel: false,
                fail: false,
            }),
        );
        recovered.recover_interrupted().unwrap();
        assert_eq!(
            crate::database::segments::read(&reopened.connect().unwrap(), &id)
                .unwrap()
                .stage,
            "interrupted"
        );
        assert_eq!(
            fs::read_to_string(storage.get_transcript_path(&id).unwrap()).unwrap(),
            "Transcrição local curta"
        );
        drop(recovered);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn real_source_pipeline_preserves_local_and_two_remote_speakers_after_reopening_database() {
        use crate::{
            audio::wav::WavWriter,
            transcription::{pipeline::NativePipeline, SherpaCli},
        };
        let (root, storage, database, id) = prepared();
        let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        for (file, path) in [
            ("jfk.wav", storage.get_microphone_path(&id).unwrap()),
            (
                "two-speakers.wav",
                storage.get_system_audio_path(&id).unwrap(),
            ),
        ] {
            let bytes = fs::read(fixtures.join(file)).unwrap();
            let mut chunk = 12;
            while &bytes[chunk..chunk + 4] != b"data" {
                let size =
                    u32::from_le_bytes(bytes[chunk + 4..chunk + 8].try_into().unwrap()) as usize;
                chunk += 8 + size + size % 2;
            }
            let size = u32::from_le_bytes(bytes[chunk + 4..chunk + 8].try_into().unwrap()) as usize;
            let mut writer = WavWriter::create(&path).unwrap();
            let pcm: Vec<_> = bytes[chunk + 8..chunk + 8 + size]
                .as_chunks::<2>()
                .0
                .iter()
                .flat_map(|sample| sample.iter().copied().cycle().take(6))
                .collect();
            writer.write_samples(&pcm).unwrap();
            writer.finish().unwrap();
        }
        let original_mic = fs::read(storage.get_microphone_path(&id).unwrap()).unwrap();
        let original_system = fs::read(storage.get_system_audio_path(&id).unwrap()).unwrap();
        let connection = database.connect().unwrap();
        let mut meeting = meetings::get(&connection, &id).unwrap().unwrap();
        meeting.language = "en".into();
        meeting.system_audio_enabled = true;
        meeting.transcription_status = "completed".into();
        meeting.transcription = Some("Texto tradicional preservado".into());
        meetings::update(&connection, &meeting).unwrap();
        fs::write(
            storage.get_transcript_path(&id).unwrap(),
            "Texto tradicional preservado",
        )
        .unwrap();
        drop(connection);
        let resources = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources");
        let engine = TranscriptionEngine::new(
            database.clone(),
            storage.clone(),
            Arc::new(WhisperCli::new(
                resources.join("whisper/whisper-cli.exe"),
                resources.join("whisper/ggml-base-q5_1.bin"),
            )),
        )
        .with_processor(Arc::new(NativePipeline::new(Arc::new(SherpaCli::new(
            resources.join("diarization"),
        )))));
        engine.start_diarization(&id, Some(2)).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(180);
        while engine.is_processing().unwrap() {
            assert!(
                std::time::Instant::now() < deadline,
                "source pipeline timed out"
            );
            thread::sleep(Duration::from_millis(50));
        }
        drop(engine);
        let reopened = Database::initialize(&storage).unwrap();
        let identified =
            crate::database::segments::read(&reopened.connect().unwrap(), &id).unwrap();
        assert_eq!(identified.status, "completed", "{:?}", identified.error);
        for label in ["Você", "Participante 1", "Participante 2"] {
            assert!(
                identified
                    .segments
                    .iter()
                    .any(|segment| segment.diarization_label == label),
                "missing {label}: {:?}",
                identified.segments
            );
        }
        assert!(identified
            .segments
            .iter()
            .all(|segment| segment.end_ms > segment.start_ms && !segment.text.trim().is_empty()));
        assert_eq!(
            fs::read_to_string(storage.get_transcript_path(&id).unwrap()).unwrap(),
            "Texto tradicional preservado"
        );
        assert_eq!(
            fs::read(storage.get_microphone_path(&id).unwrap()).unwrap(),
            original_mic
        );
        assert_eq!(
            fs::read(storage.get_system_audio_path(&id).unwrap()).unwrap(),
            original_system
        );
        assert!(
            !fs::read_dir(storage.existing_meeting_directory(&id).unwrap())
                .unwrap()
                .any(|entry| entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with("postprocess-"))
        );
        fs::remove_dir_all(root).unwrap();
    }

    struct FakeProcessor {
        fail: bool,
    }
    impl super::SegmentProcessor for FakeProcessor {
        fn run(
            &self,
            storage: &StorageManager,
            meeting: &crate::database::Meeting,
            _runner: &dyn TranscriptionRunner,
            _threads: usize,
            _cancel: &AtomicBool,
            progress: &mut dyn FnMut(&str, u8),
        ) -> Result<Vec<crate::database::segments::NewSegment>, TranscriptionFailure> {
            assert!(storage.get_transcript_path(&meeting.id).unwrap().is_file());
            progress("diarization", 50);
            if self.fail {
                return Err(TranscriptionFailure::Failed(
                    "simulated diarization failure".into(),
                ));
            }
            Ok(vec![crate::database::segments::NewSegment {
                label: "Você".into(),
                start_ms: 30,
                end_ms: 200,
                text: "Olá".into(),
                confidence: None,
                source: "microphone".into(),
            }])
        }
    }

    #[test]
    fn limits_requested_threads_to_half_of_cpus_and_four() {
        assert!(thread_limit(Some(0)).is_err());
        assert_eq!(
            thread_limit(Some(usize::MAX)).unwrap(),
            thread_limit(None).unwrap()
        );
        assert!((1..=4).contains(&thread_limit(None).unwrap()));
    }

    #[test]
    fn refuses_unfinished_meeting_and_recovers_interrupted_state() {
        let (root, storage, database, id) = prepared();
        let connection = database.connect().unwrap();
        let mut meeting = meetings::get(&connection, &id).unwrap().unwrap();
        meeting.finished_at = None;
        meeting.status = "recording".to_owned();
        meetings::update(&connection, &meeting).unwrap();
        drop(connection);
        let engine = TranscriptionEngine::new(
            database.clone(),
            storage,
            Arc::new(FakeRunner {
                seen: Arc::new(Mutex::new(Vec::new())),
                wait_cancel: false,
                fail: false,
            }),
        );
        assert!(engine.start(&id, None).is_err());
        let connection = database.connect().unwrap();
        meeting.finished_at = Some("2026-09-30T12:00:00Z".to_owned());
        meeting.status = "processing".to_owned();
        meeting.transcription_status = "processing".to_owned();
        meetings::update(&connection, &meeting).unwrap();
        drop(connection);
        engine.recover_interrupted().unwrap();
        let saved = meetings::get(&database.connect().unwrap(), &id)
            .unwrap()
            .unwrap();
        assert_eq!(saved.transcription_status, "failed");
        assert_eq!(saved.status, "failed_partial");
        drop(engine);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn recovery_commits_completed_text_if_file_was_saved_before_shutdown() {
        let (root, storage, database, id) = prepared();
        let connection = database.connect().unwrap();
        let mut meeting = meetings::get(&connection, &id).unwrap().unwrap();
        meeting.status = "processing".to_owned();
        meeting.recording_status = Some("completed".to_owned());
        meeting.transcription_status = "processing".to_owned();
        meetings::update(&connection, &meeting).unwrap();
        fs::write(
            storage.get_transcript_path(&id).unwrap(),
            "Texto salvo antes da interrupção",
        )
        .unwrap();
        drop(connection);
        let engine = TranscriptionEngine::new(
            database.clone(),
            storage,
            Arc::new(FakeRunner {
                seen: Arc::new(Mutex::new(Vec::new())),
                wait_cancel: false,
                fail: false,
            }),
        );
        engine.recover_interrupted().unwrap();
        let saved = meetings::get(&database.connect().unwrap(), &id)
            .unwrap()
            .unwrap();
        assert_eq!(saved.status, "completed");
        assert_eq!(saved.transcription_status, "completed");
        assert_eq!(
            saved.transcription.as_deref(),
            Some("Texto salvo antes da interrupção")
        );
        assert!(saved.recording_status.is_none());
        drop(engine);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn real_whisper_transcribes_short_local_wav() {
        let (root, storage, database, id) = prepared();
        let connection = database.connect().unwrap();
        let mut meeting = meetings::get(&connection, &id).unwrap().unwrap();
        meeting.language = "en".to_owned();
        meetings::update(&connection, &meeting).unwrap();
        drop(connection);
        let resources = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/whisper");
        let engine = TranscriptionEngine::new(
            database.clone(),
            storage.clone(),
            Arc::new(WhisperCli::new(
                resources.join("whisper-cli.exe"),
                resources.join("ggml-base-q5_1.bin"),
            )),
        );
        engine.start(&id, Some(2)).unwrap();
        assert_eq!(wait_finished(&engine, &id), "completed");
        let text = meetings::get(&database.connect().unwrap(), &id)
            .unwrap()
            .unwrap()
            .transcription
            .unwrap();
        assert!(
            text.contains("fellow Americans"),
            "unexpected transcript: {text}"
        );
        assert_eq!(
            fs::read_to_string(storage.get_transcript_path(&id).unwrap()).unwrap(),
            text
        );
        drop(engine);
        fs::remove_dir_all(root).unwrap();
    }
}
