use std::{
    fs::{self, OpenOptions},
    io::ErrorKind,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::sync_channel,
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use crate::storage::diagnostics::Value;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::{
    audio::{
        loopback::{self, OutputDevice},
        microphone::{self, InputDevice},
        mixer,
    },
    database::{meetings, Database, Meeting, NewMeeting},
    storage::StorageManager,
    transcription::{
        pipeline::NativePipeline, SherpaCli, TranscriptionEngine, TranscriptionState, WhisperCli,
    },
    video::{self, VideoRecorder, VideoState},
};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingState {
    pub meeting_id: Option<String>,
    pub status: String,
    pub bytes_written: u32,
    pub error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioRecordingStates {
    pub microphone: Option<RecordingState>,
    pub system: Option<RecordingState>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartMeetingInput {
    pub title: String,
    pub microphone_enabled: bool,
    pub system_audio_enabled: bool,
    pub video_enabled: bool,
    pub microphone_device_id: Option<String>,
    pub output_device_id: Option<String>,
    #[serde(default)]
    pub video_source_id: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingRecordingState {
    pub active: bool,
    pub meeting: Option<Meeting>,
    pub microphone: Option<RecordingState>,
    pub system: Option<RecordingState>,
    pub video: Option<VideoState>,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Clone)]
struct MeetingSession {
    id: String,
    microphone_enabled: bool,
    system_audio_enabled: bool,
    microphone_start_error: Option<String>,
    system_start_error: Option<String>,
    video_enabled: bool,
    video_start_error: Option<String>,
    starting: bool,
    started: Instant,
    warnings: Vec<String>,
}

impl Default for RecordingState {
    fn default() -> Self {
        Self {
            meeting_id: None,
            status: "idle".to_owned(),
            bytes_written: 0,
            error: None,
        }
    }
}

struct ActiveRecording {
    meeting_id: String,
    stop: Arc<AtomicBool>,
    worker: JoinHandle<RecordingState>,
    state: Arc<Mutex<RecordingState>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Source {
    Microphone,
    System,
}

#[derive(Default)]
struct CaptureCoordination {
    microphone_running: AtomicBool,
    system_running: AtomicBool,
    video_running: Arc<AtomicBool>,
    finalize_lock: Mutex<()>,
}

impl CaptureCoordination {
    fn running(&self, source: Source) -> &AtomicBool {
        match source {
            Source::Microphone => &self.microphone_running,
            Source::System => &self.system_running,
        }
    }
}

#[derive(Default)]
struct ManagerState {
    active_microphone: Option<ActiveRecording>,
    active_system: Option<ActiveRecording>,
    last_microphone: RecordingState,
    last_system: RecordingState,
    active_video: Option<VideoRecorder>,
    last_video: Option<VideoState>,
    current_meeting: Option<MeetingSession>,
    last_meeting: Option<MeetingSession>,
}

impl ManagerState {
    fn active(&self, source: Source) -> &Option<ActiveRecording> {
        match source {
            Source::Microphone => &self.active_microphone,
            Source::System => &self.active_system,
        }
    }
    fn active_mut(&mut self, source: Source) -> &mut Option<ActiveRecording> {
        match source {
            Source::Microphone => &mut self.active_microphone,
            Source::System => &mut self.active_system,
        }
    }
    fn last(&self, source: Source) -> &RecordingState {
        match source {
            Source::Microphone => &self.last_microphone,
            Source::System => &self.last_system,
        }
    }
    fn last_mut(&mut self, source: Source) -> &mut RecordingState {
        match source {
            Source::Microphone => &mut self.last_microphone,
            Source::System => &mut self.last_system,
        }
    }
}

pub struct MeetingManager {
    database: Database,
    storage: StorageManager,
    state: Mutex<ManagerState>,
    coordination: Arc<CaptureCoordination>,
    lifecycle: Mutex<()>,
    transcription: TranscriptionEngine,
    shutting_down: AtomicBool,
    whisper_resources: PathBuf,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ApplicationActivity {
    pub recording: bool,
    pub elapsed_seconds: u64,
    pub partial_failure: bool,
    pub processing: bool,
}

impl MeetingManager {
    pub(crate) fn get_settings(&self) -> Result<crate::settings::SettingsInfo, String> {
        use crate::settings::{model_name, model_path, ModelOption, SettingsInfo};
        let loaded =
            crate::database::settings::read(&self.database.connect().map_err(|e| e.to_string())?)?;
        let settings = loaded.settings;
        let mut warnings: Vec<String> = loaded.warning.into_iter().collect();
        let effective = self.storage.recordings_root();
        if settings
            .recordings_directory
            .as_deref()
            .is_some_and(|path| Path::new(path) != effective)
        {
            warnings.push("Diretório configurado indisponível; novas gravações usam o diretório padrão. As reuniões antigas mantêm sua localização.".into());
        }
        let models: Vec<_> = ["tiny-q5_1", "base-q5_1", "small-q5_1"]
            .into_iter()
            .map(|id| ModelOption {
                id: id.into(),
                name: model_name(id).into(),
                available: model_path(id, &self.storage.models_path(), &self.whisper_resources)
                    .is_ok(),
            })
            .collect();
        if models
            .iter()
            .any(|model| model.id == settings.transcription_model && !model.available)
        {
            warnings.push("O modelo escolhido não está disponível localmente. As gravações continuam disponíveis para transcrever depois de instalar o arquivo.".into());
        }
        if let Some(id) = settings.microphone_device_id.as_deref() {
            match self.list_input_devices() {
                Ok(devices) if !devices.iter().any(|device|device.id==id)=>warnings.push("Microfone configurado indisponível; será usado o padrão do Windows ou outro dispositivo ativo.".into()),
                Err(_)=>warnings.push("Não foi possível verificar o microfone configurado; ele será validado ao iniciar a reunião.".into()),
                _=>{},
            }
        }
        if let Some(id) = settings.output_device_id.as_deref() {
            match self.list_output_devices() {
                Ok(devices) if !devices.iter().any(|device|device.id==id)=>warnings.push("Saída configurada indisponível; será usado o padrão do Windows ou outro dispositivo ativo.".into()),
                Err(_)=>warnings.push("Não foi possível verificar a saída configurada; ela será validada ao iniciar a reunião.".into()),
                _=>{},
            }
        }
        let activity = self.activity()?;
        Ok(SettingsInfo {
            settings,
            effective_recordings_directory: effective.to_string_lossy().into_owned(),
            models,
            warnings,
            busy: activity.recording
                || activity.processing
                || self
                    .state
                    .lock()
                    .map_err(|e| e.to_string())?
                    .current_meeting
                    .is_some(),
        })
    }
    pub(crate) fn save_settings(
        &self,
        mut settings: crate::settings::AppSettings,
    ) -> Result<crate::settings::SettingsInfo, String> {
        let _lifecycle = self.lifecycle.lock().map_err(|e| e.to_string())?;
        self.ensure_running()?;
        let activity = self.activity()?;
        if activity.recording
            || activity.processing
            || self
                .state
                .lock()
                .map_err(|e| e.to_string())?
                .current_meeting
                .is_some()
        {
            return Err(
                "Finalize a gravação e o processamento antes de alterar configurações.".into(),
            );
        }
        settings.validate()?;
        if settings.transcription_model != "base-q5_1" {
            crate::settings::model_path(
                &settings.transcription_model,
                &self.storage.models_path(),
                &self.whisper_resources,
            )?;
        }
        let path = settings
            .recordings_directory
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| self.storage.default_recordings_root());
        let prepared = self
            .storage
            .prepare_recordings_root(&path)
            .map_err(|e| e.to_string())?;
        if settings.recordings_directory.is_some() {
            settings.recordings_directory = Some(prepared.to_string_lossy().into_owned());
        }
        crate::database::settings::save(
            &self.database.connect().map_err(|e| e.to_string())?,
            &settings,
        )?;
        self.storage.set_recordings_root(prepared);
        self.get_settings()
    }
    pub(crate) fn activity(&self) -> Result<ApplicationActivity, String> {
        let state = self
            .state
            .try_lock()
            .map_err(|_| "Meeting lifecycle is busy".to_owned())?;
        let session = state.current_meeting.as_ref();
        let partial_failure = session.is_some_and(|session| {
            session.microphone_start_error.is_some()
                || session.system_start_error.is_some()
                || session.video_start_error.is_some()
        }) || [Source::Microphone, Source::System].iter().any(|source| {
            state.active(*source).as_ref().is_some_and(|active| {
                active
                    .state
                    .try_lock()
                    .is_ok_and(|state| state.error.is_some())
            })
        }) || state
            .active_video
            .as_ref()
            .is_some_and(|video| video.snapshot().error.is_some());
        Ok(ApplicationActivity {
            recording: session.is_some(),
            elapsed_seconds: session.map_or(0, |session| session.started.elapsed().as_secs()),
            partial_failure,
            processing: self.transcription.is_processing()?,
        })
    }

    fn ensure_running(&self) -> Result<(), String> {
        if self.shutting_down.load(Ordering::Acquire) {
            Err("O aplicativo está encerrando; novos trabalhos estão bloqueados.".into())
        } else {
            Ok(())
        }
    }

    pub(crate) fn shutdown(&self, confirmed: bool) -> Result<(), String> {
        let _lifecycle = self.lifecycle.lock().map_err(|error| error.to_string())?;
        let recording = self
            .state
            .lock()
            .map_err(|error| error.to_string())?
            .current_meeting
            .is_some();
        if !confirmed && (recording || self.transcription.is_processing()?) {
            return Err("Confirme a finalização da gravação ou o cancelamento do processamento antes de sair.".into());
        }
        self.shutting_down.store(true, Ordering::Release);
        let result = (|| {
            let finished = if recording {
                self.stop_meeting_locked()?
            } else {
                self.get_recording_state()?
            };
            self.validate_saved_files(&finished)?;
            self.transcription.shutdown()
        })();
        if result.is_err() {
            self.shutting_down.store(false, Ordering::Release);
        }
        result
    }
    #[cfg(test)]
    pub fn new(database: Database, storage: StorageManager) -> Self {
        let resources = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/whisper");
        Self::with_whisper_resources(database, storage, resources)
    }

    pub fn with_whisper_resources(
        database: Database,
        storage: StorageManager,
        resources: PathBuf,
    ) -> Self {
        let runner = Arc::new(WhisperCli::new(
            resources.join("whisper-cli.exe"),
            resources.join("ggml-base-q5_1.bin"),
        ));
        let diarization_resources = resources.parent().unwrap_or(&resources).join("diarization");
        let processor = Arc::new(NativePipeline::new(Arc::new(SherpaCli::new(
            diarization_resources,
        ))));
        let transcription = TranscriptionEngine::new(database.clone(), storage.clone(), runner)
            .with_processor(processor);
        Self {
            database,
            storage,
            state: Mutex::new(ManagerState::default()),
            coordination: Arc::new(CaptureCoordination::default()),
            lifecycle: Mutex::new(()),
            transcription,
            shutting_down: AtomicBool::new(false),
            whisper_resources: resources,
        }
    }

    pub(crate) fn recover_transcription(&self) -> Result<(), String> {
        self.transcription.recover_interrupted()
    }

    pub(crate) fn diarize_meeting(
        &self,
        id: &str,
        threads: Option<usize>,
    ) -> Result<crate::database::segments::DiarizationState, String> {
        let _lifecycle = self.lifecycle.lock().map_err(|error| error.to_string())?;
        self.ensure_running()?;
        if self
            .state
            .lock()
            .map_err(|error| error.to_string())?
            .current_meeting
            .is_some()
        {
            return Err("Finalize a gravação antes de identificar participantes.".into());
        }
        self.transcription.start_diarization(id, threads)
    }

    pub(crate) fn transcribe_meeting(
        &self,
        id: &str,
        threads: Option<usize>,
    ) -> Result<TranscriptionState, String> {
        let _lifecycle = self.lifecycle.lock().map_err(|error| error.to_string())?;
        self.ensure_running()?;
        if self
            .state
            .lock()
            .map_err(|error| error.to_string())?
            .current_meeting
            .is_some()
        {
            return Err("Stop recording before transcription".to_owned());
        }
        if self.transcription.is_processing()? {
            return Err("Another post-processing job is active".into());
        }
        let connection = self.database.connect().map_err(|error| error.to_string())?;
        let mut meeting = meetings::get(&connection, id)
            .map_err(|error| error.to_string())?
            .ok_or("Meeting not found")?;
        if meeting.finished_at.is_some()
            && matches!(meeting.status.as_str(), "completed" | "failed_partial")
            && meeting.transcription_status != "completed"
        {
            let merged = self
                .storage
                .get_merged_audio_path(id)
                .map_err(|error| error.to_string())?;
            if meeting.merged_audio_path.is_none() || !merged.is_file() {
                let microphone = self
                    .storage
                    .get_microphone_path(id)
                    .map_err(|error| error.to_string())?;
                let system = self
                    .storage
                    .get_system_audio_path(id)
                    .map_err(|error| error.to_string())?;
                // A transient write failure must be retryable from the preserved sources.
                // prepare_audio refuses to replace an existing invalid file or directory.
                mixer::prepare_audio(&microphone, &system, &merged).inspect_err(|error| {
                    self.storage
                        .log()
                        .failure("audio_prepare_failed", Some(id), error);
                })?;
                meeting.merged_audio_path = Some(merged.to_string_lossy().into_owned());
                meetings::update(&connection, &meeting).map_err(|error| error.to_string())?;
            }
        }
        drop(connection);
        self.transcription.start(id, threads)
    }

    pub(crate) fn cancel_transcription(&self, id: &str) -> Result<TranscriptionState, String> {
        self.transcription.cancel(id)
    }

    pub(crate) fn get_transcription_state(&self, id: &str) -> Result<TranscriptionState, String> {
        self.transcription.get_state(id)
    }

    pub fn list_input_devices(&self) -> Result<Vec<InputDevice>, String> {
        microphone::list_input_devices()
            .inspect(|devices| {
                self.storage.log().event(
                    "INFO",
                    "wasapi_inputs_enumerated",
                    &[
                        ("count", Value::Count(devices.len() as u64)),
                        (
                            "defaults",
                            Value::Count(
                                devices.iter().filter(|device| device.is_default).count() as u64
                            ),
                        ),
                    ],
                )
            })
            .inspect_err(|error| {
                self.storage
                    .log()
                    .failure("wasapi_inputs_failed", None, error)
            })
    }

    pub fn list_output_devices(&self) -> Result<Vec<OutputDevice>, String> {
        loopback::list_output_devices()
            .inspect(|devices| {
                self.storage.log().event(
                    "INFO",
                    "wasapi_outputs_enumerated",
                    &[
                        ("count", Value::Count(devices.len() as u64)),
                        (
                            "defaults",
                            Value::Count(
                                devices.iter().filter(|device| device.is_default).count() as u64
                            ),
                        ),
                    ],
                )
            })
            .inspect_err(|error| {
                self.storage
                    .log()
                    .failure("wasapi_outputs_failed", None, error)
            })
    }

    pub fn start_meeting(
        &self,
        input: StartMeetingInput,
        app: AppHandle,
    ) -> Result<MeetingRecordingState, String> {
        let _lifecycle = self.lifecycle.lock().map_err(|e| e.to_string())?;
        self.ensure_running()?;
        let preferences =
            crate::database::settings::load(&self.database.connect().map_err(|e| e.to_string())?)?;
        validate_start_input(&input)?;
        let mut warnings = Vec::new();
        let microphone_id = if input.microphone_enabled {
            self.list_input_devices()
                .and_then(|devices| {
                    select_preferred_device(
                        input.microphone_device_id.as_deref(),
                        preferences.microphone_device_id.as_deref(),
                        devices
                            .iter()
                            .map(|device| (device.id.as_str(), device.is_default)),
                        "microphone",
                    )
                    .map(|(id, fallback)| {
                        if fallback {
                            self.log_device_fallback("microphone");
                            warnings.push("Microfone configurado indisponível; esta reunião usa outro microfone ativo do Windows. Confira o dispositivo antes de continuar.".into());
                        }
                        id
                    })
                })
                .map(Some)
        } else {
            Ok(None)
        };
        let output_id = if input.system_audio_enabled {
            self.list_output_devices()
                .and_then(|devices| {
                    select_preferred_device(
                        input.output_device_id.as_deref(),
                        preferences.output_device_id.as_deref(),
                        devices
                            .iter()
                            .map(|device| (device.id.as_str(), device.is_default)),
                        "output",
                    )
                    .map(|(id, fallback)| {
                        if fallback {
                            self.log_device_fallback("output");
                            warnings.push("Saída configurada indisponível; esta reunião captura outra saída ativa do Windows. Confira onde o áudio está sendo reproduzido.".into());
                        }
                        id
                    })
                })
                .map(Some)
        } else {
            Ok(None)
        };
        let microphone_error = microphone_id.as_ref().err().cloned();
        let system_error = output_id.as_ref().err().cloned();
        self.start_meeting_with_errors_locked(
            input,
            microphone_id.unwrap_or(None),
            output_id.unwrap_or(None),
            (microphone_error, system_error, warnings),
            |source, meeting_id, device_id| {
                self.start_source(source, meeting_id, device_id, app.clone())
            },
            |id, source_id, origin| {
                let source = video::select_source(source_id)?;
                let config = video::VideoConfig::from_settings(
                    &preferences.video_resolution,
                    preferences.video_fps,
                )?;
                VideoRecorder::start_configured(
                    self.storage.get_video_path(id).map_err(|e| e.to_string())?,
                    id.to_owned(),
                    (source, origin, config),
                    Some(app.clone()),
                    self.storage.log_path(),
                    self.coordination.video_running.clone(),
                )
            },
        )
    }

    fn log_device_fallback(&self, source: &str) {
        self.storage.log().event(
            "WARN",
            "device_fallback",
            &[(
                "source",
                Value::Static(if source == "microphone" {
                    "microphone"
                } else {
                    "output"
                }),
            )],
        );
    }

    #[cfg(test)]
    fn start_meeting_with_errors<F, V>(
        &self,
        input: StartMeetingInput,
        microphone_id: Option<String>,
        output_id: Option<String>,
        source_errors: (Option<String>, Option<String>),
        start: F,
        start_video: V,
    ) -> Result<MeetingRecordingState, String>
    where
        F: FnMut(Source, String, String) -> Result<RecordingState, String>,
        V: FnOnce(&str, &str, Instant) -> Result<VideoRecorder, String>,
    {
        let _lifecycle = self.lifecycle.lock().map_err(|e| e.to_string())?;
        self.start_meeting_with_errors_locked(
            input,
            microphone_id,
            output_id,
            (source_errors.0, source_errors.1, Vec::new()),
            start,
            start_video,
        )
    }

    #[cfg(test)]
    fn start_meeting_with<F>(
        &self,
        input: StartMeetingInput,
        microphone_id: Option<String>,
        output_id: Option<String>,
        start: F,
    ) -> Result<MeetingRecordingState, String>
    where
        F: FnMut(Source, String, String) -> Result<RecordingState, String>,
    {
        self.start_meeting_with_errors(
            input,
            microphone_id,
            output_id,
            (None, None),
            start,
            |_, _, _| Err("No native video in this audio test".into()),
        )
    }

    fn start_meeting_with_errors_locked<F, V>(
        &self,
        input: StartMeetingInput,
        microphone_id: Option<String>,
        output_id: Option<String>,
        source_errors: (Option<String>, Option<String>, Vec<String>),
        mut start: F,
        start_video: V,
    ) -> Result<MeetingRecordingState, String>
    where
        F: FnMut(Source, String, String) -> Result<RecordingState, String>,
        V: FnOnce(&str, &str, Instant) -> Result<VideoRecorder, String>,
    {
        validate_start_input(&input)?;
        self.ensure_running()?;
        let (microphone_error, system_error, warnings) = source_errors;
        if self.transcription.is_processing()? {
            return Err(
                "Wait for transcription to finish or cancel it before recording".to_owned(),
            );
        }
        {
            let manager = self.state.lock().map_err(|error| error.to_string())?;
            if manager.current_meeting.is_some() {
                return Err(
                    "A meeting is already active; stop it before starting another".to_owned(),
                );
            }
        }
        if input.microphone_enabled != (microphone_id.is_some() || microphone_error.is_some())
            || input.system_audio_enabled != (output_id.is_some() || system_error.is_some())
        {
            return Err("An enabled audio source requires a selected device".to_owned());
        }
        let meeting = create_meeting_with_directory(
            &self.database,
            &self.storage,
            &NewMeeting {
                title: input.title,
                microphone_enabled: input.microphone_enabled,
                system_audio_enabled: input.system_audio_enabled,
                video_enabled: input.video_enabled,
            },
        )?;
        let mut session = MeetingSession {
            id: meeting.id.clone(),
            microphone_enabled: input.microphone_enabled,
            system_audio_enabled: input.system_audio_enabled,
            microphone_start_error: microphone_error,
            system_start_error: system_error,
            video_enabled: input.video_enabled,
            video_start_error: None,
            starting: true,
            started: Instant::now(),
            warnings,
        };
        self.state
            .lock()
            .map_err(|error| error.to_string())?
            .current_meeting = Some(session.clone());
        let mut started = 0;
        if let Some(device_id) = microphone_id {
            self.storage.log().event(
                "INFO",
                "wasapi_microphone_selected",
                &[
                    ("meeting", Value::token(&meeting.id)),
                    ("device", Value::token(&device_id)),
                ],
            );
            match start(Source::Microphone, meeting.id.clone(), device_id) {
                Ok(_) => started += 1,
                Err(error) => session.microphone_start_error = Some(error),
            }
        }
        if let Some(device_id) = output_id {
            self.storage.log().event(
                "INFO",
                "wasapi_output_selected",
                &[
                    ("meeting", Value::token(&meeting.id)),
                    ("device", Value::token(&device_id)),
                ],
            );
            match start(Source::System, meeting.id.clone(), device_id) {
                Ok(_) => started += 1,
                Err(error) => session.system_start_error = Some(error),
            }
        }
        if input.video_enabled {
            match start_video(
                &meeting.id,
                input.video_source_id.as_deref().unwrap(),
                session.started,
            ) {
                Ok(video) => {
                    let mut state = self.state.lock().map_err(|e| e.to_string())?;
                    state.last_video = None;
                    state.active_video = Some(video);
                    started += 1;
                }
                Err(error) => session.video_start_error = Some(error),
            }
        }
        session.starting = false;
        for (event, error) in [
            (
                "wasapi_microphone_start_failed",
                session.microphone_start_error.as_deref(),
            ),
            (
                "wasapi_loopback_start_failed",
                session.system_start_error.as_deref(),
            ),
            ("video_start_failed", session.video_start_error.as_deref()),
        ] {
            if let Some(error) = error {
                self.storage.log().failure(event, Some(&session.id), error);
            }
        }
        self.storage.log().event(
            "INFO",
            "recording_started",
            &[
                ("meeting", Value::token(&session.id)),
                ("active_sources", Value::Count(started)),
                ("microphone", Value::Flag(session.microphone_enabled)),
                ("loopback", Value::Flag(session.system_audio_enabled)),
                ("video", Value::Flag(session.video_enabled)),
            ],
        );
        self.state
            .lock()
            .map_err(|error| error.to_string())?
            .current_meeting = Some(session.clone());
        if started == 0 {
            let sources = self.stop_audio_sources()?;
            let result = self.finish_session(&session, sources)?;
            let mut manager = self.state.lock().map_err(|error| error.to_string())?;
            manager.last_meeting = Some(session);
            manager.current_meeting = None;
            return Ok(result);
        }
        let running_status = if session.microphone_start_error.is_some()
            || session.system_start_error.is_some()
            || session.video_start_error.is_some()
        {
            "failed_partial"
        } else {
            "recording"
        };
        if let Err(error) = update_meeting_status(&self.database, &meeting.id, running_status) {
            self.storage
                .log()
                .failure("recording_metadata_failed", Some(&meeting.id), &error);
            let sources = self.stop_audio_sources()?;
            let _ = self.finish_session(&session, sources);
            let mut manager = self
                .state
                .lock()
                .map_err(|lock_error| lock_error.to_string())?;
            manager.last_meeting = Some(session);
            manager.current_meeting = None;
            return Err(error);
        }
        self.get_recording_state()
    }

    #[cfg(test)]
    pub fn stop_meeting(&self) -> Result<MeetingRecordingState, String> {
        let _lifecycle = self.lifecycle.lock().map_err(|error| error.to_string())?;
        self.stop_meeting_locked()
    }

    pub fn stop_meeting_and_transcribe(
        &self,
        threads: Option<usize>,
    ) -> Result<MeetingRecordingState, String> {
        let _lifecycle = self.lifecycle.lock().map_err(|error| error.to_string())?;
        self.ensure_running()?;
        let mut result = self.stop_meeting_locked()?;
        if let Some(meeting) = result
            .meeting
            .as_ref()
            .filter(|meeting| meeting.transcription_status == "pending")
        {
            let id = meeting.id.clone();
            match self.transcription.start(&id, threads) {
                Ok(_) => {
                    let connection = self.database.connect().map_err(|error| error.to_string())?;
                    result.meeting =
                        meetings::get(&connection, &id).map_err(|error| error.to_string())?;
                }
                Err(error) => {
                    self.storage
                        .log()
                        .failure("transcription_start_failed", Some(&id), &error);
                    result
                        .errors
                        .push(format!("Transcription could not start: {error}"));
                    let connection = self.database.connect().map_err(|error| error.to_string())?;
                    if let Some(mut meeting) =
                        meetings::get(&connection, &id).map_err(|error| error.to_string())?
                    {
                        meeting.transcription_status = "failed".to_owned();
                        result.meeting = meetings::update(&connection, &meeting)
                            .map_err(|error| error.to_string())?;
                    }
                }
            }
        }
        Ok(result)
    }

    fn stop_meeting_locked(&self) -> Result<MeetingRecordingState, String> {
        let session = self
            .state
            .lock()
            .map_err(|error| error.to_string())?
            .current_meeting
            .clone()
            .ok_or_else(|| "No meeting is active".to_owned())?;
        let sources = self.stop_audio_sources()?;
        let result = self
            .finish_session(&session, sources)
            .inspect_err(|error| {
                self.storage
                    .log()
                    .failure("recording_finalize_failed", Some(&session.id), error)
            })?;
        let mut manager = self.state.lock().map_err(|error| error.to_string())?;
        manager.last_meeting = Some(session);
        manager.current_meeting = None;
        Ok(result)
    }

    fn finish_session(
        &self,
        session: &MeetingSession,
        sources: AudioRecordingStates,
    ) -> Result<MeetingRecordingState, String> {
        let previous_microphone = self.get_source_state(Source::Microphone)?;
        let previous_system = self.get_source_state(Source::System)?;
        let microphone = source_result(
            &session.id,
            session.microphone_enabled,
            session.microphone_start_error.as_deref(),
            false,
            sources.microphone.or_else(|| {
                (previous_microphone.meeting_id.as_deref() == Some(session.id.as_str()))
                    .then_some(previous_microphone)
            }),
        );
        let system = source_result(
            &session.id,
            session.system_audio_enabled,
            session.system_start_error.as_deref(),
            false,
            sources.system.or_else(|| {
                (previous_system.meeting_id.as_deref() == Some(session.id.as_str()))
                    .then_some(previous_system)
            }),
        );
        let video = self.video_state(session)?;
        let connection = self.database.connect().map_err(|error| error.to_string())?;
        let mut meeting = meetings::get(&connection, &session.id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "Meeting not found".to_owned())?;
        let mut valid_sources = 0;
        let mut all_succeeded =
            session.microphone_start_error.is_none() && session.system_start_error.is_none();
        let mut max_bytes = 0_u32;
        for (kind, enabled, source) in [
            (
                Source::Microphone,
                session.microphone_enabled,
                microphone.as_ref(),
            ),
            (
                Source::System,
                session.system_audio_enabled,
                system.as_ref(),
            ),
        ] {
            if !enabled {
                continue;
            }
            let path = match kind {
                Source::Microphone => self.storage.get_microphone_path(&session.id),
                Source::System => self.storage.get_system_audio_path(&session.id),
            }
            .map_err(|error| error.to_string())?;
            let bytes = mixer::valid_source_bytes(&path);
            if bytes > 0 {
                valid_sources += 1;
                max_bytes = max_bytes.max(bytes);
                match kind {
                    Source::Microphone => {
                        meeting.microphone_path = Some(path.to_string_lossy().into_owned())
                    }
                    Source::System => {
                        meeting.system_audio_path = Some(path.to_string_lossy().into_owned())
                    }
                }
            }
            all_succeeded &= source.is_some_and(|state| state.status == "completed" && bytes > 0);
        }
        let valid_audio_sources = valid_sources;
        let mut duration_ms = u64::from(max_bytes) * 1000 / 96_000;
        if session.video_enabled {
            let path = self
                .storage
                .get_video_path(&session.id)
                .map_err(|e| e.to_string())?;
            let valid = video::valid_mp4(&path);
            if valid {
                valid_sources += 1;
                meeting.video_path = Some(path.to_string_lossy().into_owned());
                duration_ms = duration_ms.max(video.as_ref().map_or(0, |state| state.duration_ms));
            }
            all_succeeded &= valid
                && video
                    .as_ref()
                    .is_some_and(|state| state.status == "completed" && state.finalized);
        }
        meeting.duration_seconds = (duration_ms / 1000) as i64;
        meeting.status = if all_succeeded {
            "completed"
        } else if valid_sources > 0 {
            "failed_partial"
        } else {
            "failed"
        }
        .to_owned();
        meeting.transcription_status = if valid_audio_sources > 0 {
            "pending"
        } else {
            "failed"
        }
        .to_owned();
        meeting.finished_at = Some(
            connection
                .query_row("SELECT strftime('%Y-%m-%dT%H:%M:%fZ', 'now')", [], |row| {
                    row.get(0)
                })
                .map_err(|error| error.to_string())?,
        );
        let mut meeting = meetings::update(&connection, &meeting)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "Meeting not found during finalization".to_owned())?;
        let mut errors = collect_errors(&microphone, &system);
        if let Some(error) = video.as_ref().and_then(|state| state.error.clone()) {
            errors.push(error);
        }
        if valid_audio_sources > 0 {
            let microphone_path = self
                .storage
                .get_microphone_path(&session.id)
                .map_err(|error| error.to_string())?;
            let system_path = self
                .storage
                .get_system_audio_path(&session.id)
                .map_err(|error| error.to_string())?;
            let merged_path = self
                .storage
                .get_merged_audio_path(&session.id)
                .map_err(|error| error.to_string())?;
            match mixer::prepare_audio(&microphone_path, &system_path, &merged_path) {
                Ok(report) => {
                    debug_assert!(report.bytes_written > 0);
                    meeting.merged_audio_path = Some(merged_path.to_string_lossy().into_owned());
                    errors.extend(report.warnings);
                }
                Err(error) => {
                    self.storage
                        .log()
                        .failure("audio_prepare_failed", Some(&session.id), &error);
                    meeting.transcription_status = "failed".to_owned();
                    if meeting.status == "completed" {
                        meeting.status = "failed_partial".to_owned();
                    }
                    errors.push(format!("Audio preparation failed: {error}"));
                }
            }
            meeting = meetings::update(&connection, &meeting)
                .map_err(|error| error.to_string())?
                .ok_or_else(|| "Meeting not found during audio preparation".to_owned())?;
        }
        self.storage.log().event(
            "INFO",
            "recording_finished",
            &[
                ("meeting", Value::token(&session.id)),
                ("status", Value::status(&meeting.status)),
                ("duration_ms", Value::Count(duration_ms)),
                ("valid_sources", Value::Count(valid_sources)),
            ],
        );
        Ok(MeetingRecordingState {
            active: false,
            meeting: Some(meeting),
            errors,
            warnings: session.warnings.clone(),
            microphone,
            system,
            video,
        })
    }

    fn start_source(
        &self,
        source: Source,
        meeting_id: String,
        device_id: String,
        app: AppHandle,
    ) -> Result<RecordingState, String> {
        let mut manager = self.state.lock().map_err(|error| error.to_string())?;
        if manager
            .active(source)
            .as_ref()
            .is_some_and(|active| !active.worker.is_finished())
        {
            return Err("This audio source is already recording".to_owned());
        }
        let other = if source == Source::Microphone {
            Source::System
        } else {
            Source::Microphone
        };
        if manager
            .active(other)
            .as_ref()
            .is_some_and(|active| !active.worker.is_finished() && active.meeting_id != meeting_id)
        {
            return Err("The other audio source is recording a different meeting".to_owned());
        }
        if let Some(active) = manager.active_mut(source).take() {
            *manager.last_mut(source) = active
                .worker
                .join()
                .map_err(|_| "Capture thread panicked".to_owned())?;
        }
        let path = prepare_recording(&self.database, &self.storage, &meeting_id, source)?;
        update_meeting_status(&self.database, &meeting_id, "recording")?;
        let current = RecordingState {
            meeting_id: Some(meeting_id.clone()),
            status: "recording".to_owned(),
            ..RecordingState::default()
        };
        let shared = Arc::new(Mutex::new(current.clone()));
        let stop = Arc::new(AtomicBool::new(false));
        let (sender, receiver) = sync_channel(1);
        let worker_stop = stop.clone();
        let worker_state = shared.clone();
        let worker_database = self.database.clone();
        let worker_storage = self.storage.clone();
        let worker_coordination = self.coordination.clone();
        let worker_app = app.clone();
        let worker_id = meeting_id.clone();
        let timeline_origin = manager
            .current_meeting
            .as_ref()
            .filter(|session| session.video_enabled && session.id == meeting_id)
            .map(|session| session.started);
        self.coordination
            .running(source)
            .store(true, Ordering::Release);
        let worker = thread::Builder::new()
            .name(
                match source {
                    Source::Microphone => "microphone-capture",
                    Source::System => "system-loopback",
                }
                .to_owned(),
            )
            .spawn(move || {
                let result = match source {
                    Source::Microphone => {
                        if let Some(origin) = timeline_origin {
                            microphone::capture_on_timeline(
                                &device_id,
                                &path,
                                &worker_stop,
                                &sender,
                                Some(origin),
                            )
                        } else {
                            microphone::capture(&device_id, &path, &worker_stop, &sender)
                        }
                    }
                    Source::System => {
                        if let Some(origin) = timeline_origin {
                            loopback::capture_on_timeline(
                                &device_id,
                                &path,
                                &worker_stop,
                                &sender,
                                Some(origin),
                            )
                        } else {
                            loopback::capture(&device_id, &path, &worker_stop, &sender)
                        }
                    }
                };
                let final_state = finalize_recording(
                    &worker_database,
                    &worker_id,
                    &path,
                    result,
                    source,
                    &worker_coordination,
                );
                log_capture(&worker_storage, source, &worker_id, &final_state);
                if let Ok(mut state) = worker_state.lock() {
                    *state = final_state.clone();
                }
                let _ = worker_app.emit(event_name(source), &final_state);
                final_state
            })
            .map_err(|error| {
                self.coordination
                    .running(source)
                    .store(false, Ordering::Release);
                error.to_string()
            })?;
        *manager.active_mut(source) = Some(ActiveRecording {
            meeting_id,
            stop,
            worker,
            state: shared,
        });
        drop(manager);
        match receiver.recv_timeout(Duration::from_secs(10)) {
            Ok(Ok(())) => {
                log_capture(
                    &self.storage,
                    source,
                    current.meeting_id.as_deref().unwrap_or(""),
                    &current,
                );
                let _ = app.emit(event_name(source), &current);
                Ok(current)
            }
            Ok(Err(error)) => Err(error),
            Err(error) => {
                if let Ok(manager) = self.state.lock() {
                    if let Some(active) = manager.active(source).as_ref() {
                        active.stop.store(true, Ordering::Release);
                    }
                }
                Err(format!("Capture did not start: {error}"))
            }
        }
    }

    pub fn stop_audio_sources(&self) -> Result<AudioRecordingStates, String> {
        let mut manager = self.state.lock().map_err(|error| error.to_string())?;
        if let Some(video) = &manager.active_video {
            video.stop.store(true, Ordering::Release);
        }
        for source in [Source::Microphone, Source::System] {
            if let Some(active) = manager.active(source).as_ref() {
                active.stop.store(true, Ordering::Release);
            }
        }
        let mut states = AudioRecordingStates {
            microphone: None,
            system: None,
        };
        for source in [Source::Microphone, Source::System] {
            if let Some(active) = manager.active_mut(source).take() {
                let id = active.meeting_id.clone();
                let final_state = match active.worker.join() {
                    Ok(state) => state,
                    Err(_) => {
                        let path = match source {
                            Source::Microphone => self.storage.get_microphone_path(&id),
                            Source::System => self.storage.get_system_audio_path(&id),
                        }
                        .map_err(|error| error.to_string())?;
                        finalize_recording(
                            &self.database,
                            &id,
                            &path,
                            Err("Capture thread panicked".to_owned()),
                            source,
                            &self.coordination,
                        )
                    }
                };
                if let Some(error) = &final_state.error {
                    self.storage.log().failure(
                        if source == Source::Microphone {
                            "wasapi_microphone_failed"
                        } else {
                            "wasapi_loopback_failed"
                        },
                        Some(&id),
                        error,
                    );
                }
                *manager.last_mut(source) = final_state.clone();
                match source {
                    Source::Microphone => states.microphone = Some(final_state),
                    Source::System => states.system = Some(final_state),
                }
            }
        }
        if let Some(mut video) = manager.active_video.take() {
            manager.last_video = Some(video.finish());
        }
        Ok(states)
    }

    pub fn get_recording_state(&self) -> Result<MeetingRecordingState, String> {
        let (session, active) = {
            let manager = self.state.lock().map_err(|error| error.to_string())?;
            (
                manager
                    .current_meeting
                    .clone()
                    .or_else(|| manager.last_meeting.clone()),
                manager.current_meeting.is_some(),
            )
        };
        let Some(session) = session else {
            return Ok(MeetingRecordingState {
                active: false,
                meeting: None,
                microphone: None,
                system: None,
                video: None,
                errors: Vec::new(),
                warnings: Vec::new(),
            });
        };
        let connection = self.database.connect().map_err(|error| error.to_string())?;
        let meeting = meetings::get(&connection, &session.id).map_err(|error| error.to_string())?;
        let mic = self.get_source_state(Source::Microphone)?;
        let sys = self.get_source_state(Source::System)?;
        let microphone = source_result(
            &session.id,
            session.microphone_enabled,
            session.microphone_start_error.as_deref(),
            session.starting,
            (mic.meeting_id.as_deref() == Some(session.id.as_str())).then_some(mic),
        );
        let system = source_result(
            &session.id,
            session.system_audio_enabled,
            session.system_start_error.as_deref(),
            session.starting,
            (sys.meeting_id.as_deref() == Some(session.id.as_str())).then_some(sys),
        );
        let video = self.video_state(&session)?;
        let mut errors = collect_errors(&microphone, &system);
        if let Some(error) = video.as_ref().and_then(|state| state.error.clone()) {
            errors.push(error);
        }
        Ok(MeetingRecordingState {
            active,
            meeting,
            errors,
            warnings: session.warnings,
            microphone,
            system,
            video,
        })
    }

    fn video_state(&self, session: &MeetingSession) -> Result<Option<VideoState>, String> {
        if !session.video_enabled {
            return Ok(None);
        }
        let state = self.state.lock().map_err(|e| e.to_string())?;
        let video = state
            .active_video
            .as_ref()
            .map(VideoRecorder::snapshot)
            .or_else(|| {
                state
                    .last_video
                    .clone()
                    .filter(|video| video.meeting_id == session.id)
            });
        Ok(Some(video.unwrap_or_else(|| {
            VideoState {
                meeting_id: session.id.clone(),
                status: if session.starting {
                    "starting"
                } else {
                    "failed"
                }
                .into(),
                error: session.video_start_error.clone(),
                ..Default::default()
            }
        })))
    }

    fn validate_saved_files(&self, result: &MeetingRecordingState) -> Result<(), String> {
        use std::io::Read;
        let Some(meeting) = &result.meeting else {
            return Ok(());
        };
        for (source, path) in [
            (result.microphone.as_ref(), meeting.microphone_path.as_ref()),
            (result.system.as_ref(), meeting.system_audio_path.as_ref()),
        ] {
            if source.is_some_and(|state| state.bytes_written > 0) && path.is_none() {
                return Err("Áudio capturado ainda não possui um WAV válido salvo.".into());
            }
            if let Some(path) = path {
                let mut file = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(path)
                    .map_err(|e| e.to_string())?;
                let mut header = [0_u8; 44];
                file.read_exact(&mut header).map_err(|e| e.to_string())?;
                let bytes = u32::from_le_bytes(header[40..44].try_into().unwrap());
                if &header[..4] != b"RIFF"
                    || &header[8..12] != b"WAVE"
                    || file.metadata().map_err(|e| e.to_string())?.len() != 44 + u64::from(bytes)
                {
                    return Err(
                        "WAV incompleto: a saída foi bloqueada para preservar os arquivos.".into(),
                    );
                }
                file.sync_all()
                    .map_err(|e| format!("Falha de flush do WAV: {e}"))?;
            }
        }
        if let Some(video) = &result.video {
            if let Some(error) = &video.finalization_error {
                return Err(format!("Falha ao finalizar vídeo: {error}"));
            }
        }
        if let Some(path) = &meeting.video_path {
            if !video::valid_mp4(Path::new(path)) {
                return Err("MP4 ainda não foi finalizado; a saída foi bloqueada.".into());
            }
            OpenOptions::new()
                .write(true)
                .open(path)
                .and_then(|file| file.sync_all())
                .map_err(|e| format!("Falha de flush do MP4: {e}"))?;
        }
        Ok(())
    }

    fn get_source_state(&self, source: Source) -> Result<RecordingState, String> {
        let manager = self.state.lock().map_err(|error| error.to_string())?;
        if let Some(active) = manager.active(source).as_ref() {
            return active
                .state
                .lock()
                .map(|state| state.clone())
                .map_err(|error| error.to_string());
        }
        Ok(manager.last(source).clone())
    }

    pub fn delete_meeting(&self, meeting_id: &str) -> Result<bool, String> {
        let _lifecycle = self.lifecycle.lock().map_err(|error| error.to_string())?;
        if self.transcription.is_processing_meeting(meeting_id)? {
            return Err("Cancel transcription before deleting this meeting".to_owned());
        }
        let manager = self.state.lock().map_err(|error| error.to_string())?;
        if manager
            .current_meeting
            .as_ref()
            .is_some_and(|session| session.id == meeting_id)
        {
            return Err("Stop the meeting before deleting it".to_owned());
        }
        if [Source::Microphone, Source::System].iter().any(|source| {
            manager.active(*source).as_ref().is_some_and(|active| {
                active.meeting_id == meeting_id && !active.worker.is_finished()
            })
        }) {
            return Err("Stop audio recording before deleting this meeting".to_owned());
        }
        let connection = self.database.connect().map_err(|error| error.to_string())?;
        if meetings::get(&connection, meeting_id)
            .map_err(|error| error.to_string())?
            .is_none()
        {
            return Ok(false);
        }
        self.storage
            .delete_meeting_files(meeting_id)
            .map_err(|error| error.to_string())?;
        let deleted =
            meetings::delete(&connection, meeting_id).map_err(|error| error.to_string())?;
        drop(connection);
        drop(manager);
        if deleted {
            let mut manager = self.state.lock().map_err(|error| error.to_string())?;
            if manager
                .last_meeting
                .as_ref()
                .is_some_and(|session| session.id == meeting_id)
            {
                manager.last_meeting = None;
            }
        }
        Ok(deleted)
    }
}

impl Drop for MeetingManager {
    fn drop(&mut self) {
        if let Ok(mut state) = self.state.lock() {
            if let Some(video) = &state.active_video {
                video.stop.store(true, Ordering::Release);
            }
            for source in [Source::Microphone, Source::System] {
                if let Some(active) = state.active(source).as_ref() {
                    active.stop.store(true, Ordering::Release);
                }
            }
            if let Some(mut video) = state.active_video.take() {
                let _ = video.finish();
            }
            for source in [Source::Microphone, Source::System] {
                if let Some(active) = state.active_mut(source).take() {
                    let _ = active.worker.join();
                }
            }
        }
    }
}

fn validate_start_input(input: &StartMeetingInput) -> Result<(), String> {
    if input.title.trim().is_empty() {
        return Err("O nome da reunião não pode ficar vazio.".to_owned());
    }
    if !input.microphone_enabled && !input.system_audio_enabled && !input.video_enabled {
        return Err("Enable at least one recording source".to_owned());
    }
    if input.video_enabled
        && input
            .video_source_id
            .as_deref()
            .is_none_or(|id| id.trim().is_empty())
    {
        return Err("Selecione uma janela ou monitor antes de gravar.".to_owned());
    }
    Ok(())
}

fn select_device_id<'a>(
    requested: Option<&str>,
    devices: impl Iterator<Item = (&'a str, bool)>,
    label: &str,
) -> Result<String, String> {
    let devices: Vec<_> = devices.collect();
    if let Some(id) = requested {
        return devices
            .iter()
            .find(|(candidate, _)| *candidate == id)
            .map(|(id, _)| (*id).to_owned())
            .ok_or_else(|| format!("Selected {label} device is not active"));
    }
    devices
        .iter()
        .find(|(_, is_default)| *is_default)
        .or_else(|| devices.first())
        .map(|(id, _)| (*id).to_owned())
        .ok_or_else(|| format!("No active {label} device is available"))
}

fn select_preferred_device<'a>(
    requested: Option<&str>,
    configured: Option<&str>,
    devices: impl Iterator<Item = (&'a str, bool)>,
    label: &str,
) -> Result<(String, bool), String> {
    let devices: Vec<_> = devices.collect();
    if requested.is_some() {
        return select_device_id(requested, devices.into_iter(), label).map(|id| (id, false));
    }
    if let Some(id) = configured {
        if devices.iter().any(|(candidate, _)| *candidate == id) {
            return Ok((id.into(), false));
        }
    }
    select_device_id(None, devices.into_iter(), label).map(|id| (id, configured.is_some()))
}

fn source_result(
    meeting_id: &str,
    enabled: bool,
    start_error: Option<&str>,
    starting: bool,
    state: Option<RecordingState>,
) -> Option<RecordingState> {
    if !enabled {
        return None;
    }
    if let Some(mut state) = state {
        if let Some(error) = start_error {
            state.error = Some(match state.error {
                Some(existing) => format!("{error}; {existing}"),
                None => error.to_owned(),
            });
        }
        return Some(state);
    }
    Some(RecordingState {
        meeting_id: Some(meeting_id.to_owned()),
        status: if starting && start_error.is_none() {
            "starting"
        } else {
            "failed"
        }
        .to_owned(),
        bytes_written: 0,
        error: start_error
            .map(str::to_owned)
            .or_else(|| (!starting).then(|| "Audio source did not start".to_owned())),
    })
}

fn collect_errors(
    microphone: &Option<RecordingState>,
    system: &Option<RecordingState>,
) -> Vec<String> {
    [microphone.as_ref(), system.as_ref()]
        .into_iter()
        .filter_map(|state| state.and_then(|state| state.error.clone()))
        .collect()
}

pub(crate) fn create_meeting_with_directory(
    database: &Database,
    storage: &StorageManager,
    input: &NewMeeting,
) -> Result<Meeting, String> {
    let mut connection = database.connect().map_err(|error| error.to_string())?;
    let transaction = connection
        .transaction()
        .map_err(|error| error.to_string())?;
    let mut meeting = meetings::create(&transaction, input).map_err(|error| error.to_string())?;
    let preferences = crate::database::settings::load(&transaction)?;
    if preferences.language != meeting.language {
        meeting.language = preferences.language;
        meetings::update(&transaction, &meeting).map_err(|e| e.to_string())?;
    }
    let directory = storage
        .create_meeting_directory(&meeting.id)
        .map_err(|error| error.to_string())?;
    crate::database::locations::save(&transaction, &meeting.id, &directory)
        .map_err(|e| e.to_string())?;
    transaction.commit().map_err(|error| error.to_string())?;
    Ok(meeting)
}

fn update_meeting_status(database: &Database, id: &str, status: &str) -> Result<(), String> {
    let connection = database.connect().map_err(|error| error.to_string())?;
    let mut meeting = meetings::get(&connection, id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Meeting not found".to_owned())?;
    meeting.finished_at = None;
    if meeting.status == "failed" && status == "recording" {
        meeting.status = "failed_partial".to_owned();
    } else if meeting.status != "failed_partial" {
        meeting.status = status.to_owned();
    }
    meetings::update(&connection, &meeting).map_err(|error| error.to_string())?;
    Ok(())
}

fn finalize_recording(
    database: &Database,
    id: &str,
    path: &Path,
    result: Result<u32, String>,
    source: Source,
    coordination: &CaptureCoordination,
) -> RecordingState {
    coordination.running(source).store(false, Ordering::Release);
    let _guard = coordination
        .finalize_lock
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let bytes = result
        .as_ref()
        .copied()
        .unwrap_or_else(|_| mixer::valid_source_bytes(path));
    let mut state = RecordingState {
        meeting_id: Some(id.to_owned()),
        status: if result.is_ok() {
            "completed".to_owned()
        } else if bytes > 0 {
            "failed_partial".to_owned()
        } else {
            "failed".to_owned()
        },
        bytes_written: bytes,
        error: result.err(),
    };
    let persist = (|| -> Result<(), String> {
        let connection = database.connect().map_err(|error| error.to_string())?;
        let mut meeting = meetings::get(&connection, id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "Meeting not found".to_owned())?;
        let other = if source == Source::Microphone {
            Source::System
        } else {
            Source::Microphone
        };
        let other_running = coordination.running(other).load(Ordering::Acquire)
            || coordination.video_running.load(Ordering::Acquire);
        meeting.status = if meeting.status == "failed_partial"
            || (meeting.status == "failed" && state.status == "completed")
            || state.status == "failed_partial"
            || (state.status == "failed" && other_running)
        {
            "failed_partial".to_owned()
        } else if other_running {
            "recording".to_owned()
        } else if state.status == "failed"
            && (meeting.microphone_path.is_some() || meeting.system_audio_path.is_some())
        {
            "failed_partial".to_owned()
        } else {
            state.status.clone()
        };
        meeting.duration_seconds = meeting
            .duration_seconds
            .max(i64::from(bytes) / (48_000 * 2));
        if bytes > 0 {
            match source {
                Source::Microphone => {
                    meeting.microphone_path = Some(path.to_string_lossy().into_owned())
                }
                Source::System => {
                    meeting.system_audio_path = Some(path.to_string_lossy().into_owned())
                }
            }
        }
        if !other_running {
            meeting.finished_at = Some(
                connection
                    .query_row("SELECT strftime('%Y-%m-%dT%H:%M:%fZ', 'now')", [], |row| {
                        row.get(0)
                    })
                    .map_err(|error| error.to_string())?,
            );
        }
        meetings::update(&connection, &meeting).map_err(|error| error.to_string())?;
        Ok(())
    })();
    if let Err(error) = persist {
        state.status = if bytes > 0 {
            "failed_partial"
        } else {
            "failed"
        }
        .to_owned();
        state.error = Some(match state.error {
            Some(existing) => format!("{existing}; metadata: {error}"),
            None => format!("Could not save recording metadata: {error}"),
        });
    }
    state
}

fn prepare_recording(
    database: &Database,
    storage: &StorageManager,
    meeting_id: &str,
    source: Source,
) -> Result<PathBuf, String> {
    let connection = database.connect().map_err(|error| error.to_string())?;
    let meeting = meetings::get(&connection, meeting_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Meeting not found".to_owned())?;
    if !(match source {
        Source::Microphone => meeting.microphone_enabled,
        Source::System => meeting.system_audio_enabled,
    }) {
        return Err("Audio source is disabled for this meeting".to_owned());
    }
    storage
        .create_meeting_directory(meeting_id)
        .map_err(|error| error.to_string())?;
    let path = match source {
        Source::Microphone => storage.get_microphone_path(meeting_id),
        Source::System => storage.get_system_audio_path(meeting_id),
    }
    .map_err(|error| error.to_string())?;
    match fs::symlink_metadata(&path) {
        Ok(_) => Err("Audio recording already exists".to_owned()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(path),
        Err(error) => Err(error.to_string()),
    }
}

fn event_name(source: Source) -> &'static str {
    match source {
        Source::Microphone => "recording-status",
        Source::System => "system-recording-status",
    }
}

fn log_capture(storage: &StorageManager, source: Source, id: &str, state: &RecordingState) {
    let label = match source {
        Source::Microphone => "microphone",
        Source::System => "system",
    };
    storage.log().event(
        "INFO",
        "capture_state",
        &[
            ("source", Value::Static(label)),
            ("meeting", Value::token(id)),
            ("status", Value::status(&state.status)),
            ("bytes", Value::Count(u64::from(state.bytes_written))),
        ],
    );
    if let Some(error) = &state.error {
        storage.log().failure(
            if source == Source::Microphone {
                "wasapi_microphone_failed"
            } else {
                "wasapi_loopback_failed"
            },
            Some(id),
            error,
        );
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc, Mutex,
        },
        thread,
        time::{SystemTime, UNIX_EPOCH},
    };

    use crate::{
        audio::wav::WavWriter,
        database::{meetings, Database, NewMeeting},
        storage::StorageManager,
    };

    use super::{
        finalize_recording, prepare_recording, select_device_id, ActiveRecording,
        AudioRecordingStates, CaptureCoordination, MeetingManager, RecordingState, Source,
        StartMeetingInput,
    };

    fn root() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "meeting-recorder-mic-manager-{}-{nanos}",
            std::process::id()
        ))
    }

    fn start_input(microphone: bool, system: bool) -> StartMeetingInput {
        StartMeetingInput {
            title: "Reunião de teste".to_owned(),
            microphone_enabled: microphone,
            system_audio_enabled: system,
            video_enabled: false,
            microphone_device_id: None,
            output_device_id: None,
            video_source_id: None,
        }
    }

    #[test]
    fn configured_device_falls_back_but_explicit_selection_keeps_previous_contract() {
        let devices = [
            ("first", false),
            ("windows-default", true),
            ("preferred", false),
        ];
        assert_eq!(
            super::select_preferred_device(None, Some("preferred"), devices.into_iter(), "mic")
                .unwrap(),
            ("preferred".into(), false)
        );
        assert_eq!(
            super::select_preferred_device(None, Some("removed"), devices.into_iter(), "mic")
                .unwrap(),
            ("windows-default".into(), true)
        );
        assert_eq!(
            super::select_preferred_device(
                Some("first"),
                Some("preferred"),
                devices.into_iter(),
                "mic"
            )
            .unwrap(),
            ("first".into(), false)
        );
        assert!(super::select_preferred_device(
            Some("removed"),
            Some("preferred"),
            devices.into_iter(),
            "mic"
        )
        .is_err());
        assert!(
            super::select_preferred_device(None, Some("removed"), std::iter::empty(), "mic")
                .is_err()
        );
    }

    #[test]
    fn capture_warnings_are_visible_without_turning_fallback_into_partial_failure() {
        let root = root();
        let storage = StorageManager::initialize_in(&root).unwrap();
        let database = Database::initialize(&storage).unwrap();
        let manager = MeetingManager::new(database, storage.clone());
        let state = manager
            .start_meeting_with(
                start_input(true, false),
                Some("mic".into()),
                None,
                |source, id, _| fake_capture(&manager, &storage, source, id, 9600),
            )
            .unwrap();
        assert_eq!(
            serde_json::to_value(&state).unwrap()["warnings"],
            serde_json::json!([])
        );
        assert!(state.errors.is_empty());
        assert_eq!(
            manager.stop_meeting().unwrap().meeting.unwrap().status,
            "completed"
        );
        let warning = "Microfone configurado indisponível; usando padrão do Windows.".to_owned();
        let warned = {
            let _guard = manager.lifecycle.lock().unwrap();
            manager
                .start_meeting_with_errors_locked(
                    start_input(true, false),
                    Some("mic".into()),
                    None,
                    (None, None, vec![warning.clone()]),
                    |source, id, _| fake_capture(&manager, &storage, source, id, 9600),
                    |_, _, _| Err("video disabled".into()),
                )
                .unwrap()
        };
        assert_eq!(warned.warnings, vec![warning.clone()]);
        assert!(warned.errors.is_empty());
        assert_eq!(
            manager.get_recording_state().unwrap().warnings,
            vec![warning.clone()]
        );
        let stopped = manager.stop_meeting().unwrap();
        assert_eq!(stopped.warnings, vec![warning]);
        assert_eq!(stopped.meeting.unwrap().status, "completed");
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn settings_cannot_change_during_post_processing() {
        struct BlockingRunner;
        impl crate::transcription::TranscriptionRunner for BlockingRunner {
            fn run(
                &self,
                _: &std::path::Path,
                _: &std::path::Path,
                _: &str,
                _: usize,
                cancel: &AtomicBool,
                _: &mut dyn FnMut(u8),
            ) -> Result<String, crate::transcription::TranscriptionFailure> {
                while !cancel.load(Ordering::Acquire) {
                    thread::sleep(std::time::Duration::from_millis(5));
                }
                Err(crate::transcription::TranscriptionFailure::Cancelled)
            }
        }
        let root = root();
        let storage = StorageManager::initialize_in(&root).unwrap();
        let database = Database::initialize(&storage).unwrap();
        let mut manager = MeetingManager::new(database.clone(), storage.clone());
        let prefs = crate::settings::AppSettings {
            language: "en".into(),
            max_threads: 1,
            ..Default::default()
        };
        manager.save_settings(prefs.clone()).unwrap();
        let conn = database.connect().unwrap();
        let mut meeting = super::create_meeting_with_directory(
            &database,
            &storage,
            &crate::database::NewMeeting {
                title: "Post processing".into(),
                microphone_enabled: true,
                system_audio_enabled: false,
                video_enabled: false,
            },
        )
        .unwrap();
        let audio = storage.get_merged_audio_path(&meeting.id).unwrap();
        fs::copy(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/jfk.wav"),
            &audio,
        )
        .unwrap();
        meeting.merged_audio_path = Some(audio.to_string_lossy().into_owned());
        meeting.finished_at = Some("2026-10-02T12:00:00Z".into());
        meeting.status = "completed".into();
        meetings::update(&conn, &meeting).unwrap();
        manager.transcription = crate::transcription::TranscriptionEngine::new(
            database.clone(),
            storage.clone(),
            Arc::new(BlockingRunner),
        );
        manager.transcription.start(&meeting.id, None).unwrap();
        assert!(manager.get_settings().unwrap().busy);
        assert!(manager.save_settings(Default::default()).is_err());
        assert_eq!(crate::database::settings::load(&conn).unwrap(), prefs);
        manager.transcription.shutdown().unwrap();
        assert!(!manager.get_settings().unwrap().busy);
        drop(conn);
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_settings_commit_does_not_change_effective_root_or_saved_preferences() {
        let root = root();
        let storage = StorageManager::initialize_in(&root).unwrap();
        let database = Database::initialize(&storage).unwrap();
        let manager = MeetingManager::new(database.clone(), storage.clone());
        manager.save_settings(Default::default()).unwrap();
        let conn = database.connect().unwrap();
        conn.execute_batch("CREATE TRIGGER deny_settings BEFORE INSERT ON app_settings BEGIN SELECT RAISE(ABORT,'simulated disk failure'); END;").unwrap();
        let requested = crate::settings::AppSettings {
            recordings_directory: Some(root.join("alternative").to_string_lossy().into_owned()),
            language: "en".into(),
            ..Default::default()
        };
        assert!(manager.save_settings(requested).is_err());
        assert_eq!(storage.recordings_root(), storage.default_recordings_root());
        assert_eq!(
            crate::database::settings::load(&conn).unwrap(),
            Default::default()
        );
        drop(conn);
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn settings_persist_and_cannot_change_while_a_meeting_is_active() {
        let root = root();
        let storage = StorageManager::initialize_in(&root).unwrap();
        let database = Database::initialize(&storage).unwrap();
        let manager = MeetingManager::new(database.clone(), storage.clone());
        let preferences = crate::settings::AppSettings {
            language: "es".into(),
            max_threads: 1,
            ..Default::default()
        };
        manager.save_settings(preferences.clone()).unwrap();
        assert_eq!(manager.get_settings().unwrap().settings, preferences);
        manager
            .start_meeting_with(
                start_input(true, false),
                Some("mic".into()),
                None,
                |source, id, _| fake_capture(&manager, &storage, source, id, 9600),
            )
            .unwrap();
        assert!(manager.save_settings(Default::default()).is_err());
        assert_eq!(
            crate::database::settings::load(&database.connect().unwrap()).unwrap(),
            preferences
        );
        manager.stop_meeting().unwrap();
        drop(manager);
        let reopened =
            MeetingManager::new(Database::initialize(&storage).unwrap(), storage.clone());
        assert_eq!(reopened.get_settings().unwrap().settings, preferences);
        drop(reopened);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn shutdown_validation_rejects_unflushed_audio_instead_of_ignoring_partial_errors() {
        let root = root();
        let storage = StorageManager::initialize_in(&root).unwrap();
        let database = Database::initialize(&storage).unwrap();
        let manager = MeetingManager::new(database, storage.clone());
        manager
            .start_meeting_with(
                start_input(true, false),
                Some("mic".into()),
                None,
                |source, id, _| fake_capture(&manager, &storage, source, id, 9600),
            )
            .unwrap();
        let stopped = manager.stop_meeting().unwrap();
        let id = stopped.meeting.as_ref().unwrap().id.clone();
        assert!(manager.validate_saved_files(&stopped).is_ok());
        let path = storage.get_microphone_path(&id).unwrap();
        let mut bytes = fs::read(&path).unwrap();
        bytes[40..44].copy_from_slice(&0_u32.to_le_bytes());
        fs::write(&path, bytes).unwrap();
        assert!(manager.validate_saved_files(&stopped).is_err());
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn optional_video_worker_is_not_started_and_video_failure_preserves_audio() {
        let root = root();
        let storage = StorageManager::initialize_in(&root).unwrap();
        let database = Database::initialize(&storage).unwrap();
        let manager = MeetingManager::new(database, storage.clone());
        manager
            .start_meeting_with_errors(
                start_input(true, false),
                Some("mic".into()),
                None,
                (None, None),
                |source, id, _| fake_capture(&manager, &storage, source, id, 96_000),
                |_, _, _| panic!("Disabled video must not allocate native resources"),
            )
            .unwrap();
        assert!(manager.state.lock().unwrap().active_video.is_none());
        manager.stop_meeting().unwrap();
        let mut input = start_input(true, false);
        input.video_enabled = true;
        input.video_source_id = Some("window:999:0".into());
        let started = manager
            .start_meeting_with_errors(
                input,
                Some("mic".into()),
                None,
                (None, None),
                |source, id, _| fake_capture(&manager, &storage, source, id, 96_000),
                |_, _, _| Err("Capture device unavailable".into()),
            )
            .unwrap();
        assert!(started.active);
        assert_eq!(started.meeting.as_ref().unwrap().status, "failed_partial");
        let stopped = manager.stop_meeting().unwrap();
        assert_eq!(stopped.meeting.as_ref().unwrap().status, "failed_partial");
        assert!(stopped.meeting.as_ref().unwrap().microphone_path.is_some());
        assert!(stopped.meeting.as_ref().unwrap().video_path.is_none());
        assert!(stopped
            .errors
            .iter()
            .any(|error| error.contains("Capture device unavailable")));
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn video_only_stop_flushes_worker_and_persists_mp4_path_and_duration() {
        let root = root();
        let storage = StorageManager::initialize_in(&root).unwrap();
        let database = Database::initialize(&storage).unwrap();
        let manager = MeetingManager::new(database.clone(), storage.clone());
        let mut input = start_input(false, false);
        input.video_enabled = true;
        input.video_source_id = Some("monitor:1".into());
        let started = manager
            .start_meeting_with_errors(
                input,
                None,
                None,
                (None, None),
                |_, _, _| panic!("Audio disabled"),
                |id, _, _| {
                    Ok(crate::video::VideoRecorder::fake(
                        storage.get_video_path(id).unwrap(),
                        id.to_owned(),
                    ))
                },
            )
            .unwrap();
        let id = started.meeting.unwrap().id;
        assert!(started.active);
        assert!(started.video.is_some());
        manager.shutdown(true).unwrap();
        let stopped = manager.get_recording_state().unwrap();
        assert!(!stopped.active);
        assert!(stopped.video.unwrap().finalized);
        assert!(manager.state.lock().unwrap().active_video.is_none());
        let saved = meetings::get(&database.connect().unwrap(), &id)
            .unwrap()
            .unwrap();
        assert_eq!(saved.status, "completed");
        assert_eq!(saved.duration_seconds, 2);
        assert!(saved.video_path.is_some());
        assert!(saved.merged_audio_path.is_none());
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn video_configuration_is_optional_and_video_only_is_valid() {
        let legacy: StartMeetingInput = serde_json::from_str(r#"{"title":"Old","microphoneEnabled":true,"systemAudioEnabled":false,"videoEnabled":false}"#).unwrap();
        assert!(super::validate_start_input(&legacy).is_ok());
        let mut input = start_input(false, false);
        input.video_enabled = true;
        // A source is required even for a meeting that has only video.
        assert!(super::validate_start_input(&input).is_err());
        let mut value = serde_json::json!({"title":"Video", "microphoneEnabled":false,"systemAudioEnabled":false,"videoEnabled":true,"videoSourceId":"monitor:123"});
        let video: StartMeetingInput = serde_json::from_value(value.take()).unwrap();
        assert!(super::validate_start_input(&video).is_ok());
    }

    #[test]
    fn shutdown_requires_confirmation_then_flushes_both_sources_and_blocks_new_work() {
        let root = root();
        let storage = StorageManager::initialize_in(&root).unwrap();
        let database = Database::initialize(&storage).unwrap();
        let manager = MeetingManager::new(database.clone(), storage.clone());
        let result = manager
            .start_meeting_with(
                start_input(true, true),
                Some("mic".into()),
                Some("system".into()),
                |source, id, _| fake_capture(&manager, &storage, source, id, 9600),
            )
            .unwrap();
        let id = result.meeting.unwrap().id;
        assert!(manager.shutdown(false).is_err());
        assert!(manager.activity().unwrap().recording);
        assert!(manager.get_recording_state().unwrap().active);
        manager.shutdown(true).unwrap();
        assert!(!manager.activity().unwrap().recording);
        assert!(!manager.get_recording_state().unwrap().active);
        for path in [
            storage.get_microphone_path(&id).unwrap(),
            storage.get_system_audio_path(&id).unwrap(),
        ] {
            let wav = fs::read(path).unwrap();
            assert_eq!(&wav[..4], b"RIFF");
            assert_eq!(u32::from_le_bytes(wav[40..44].try_into().unwrap()), 9600);
        }
        let saved = meetings::get(&database.connect().unwrap(), &id)
            .unwrap()
            .unwrap();
        assert!(saved.finished_at.is_some());
        assert_eq!(saved.status, "completed");
        assert_eq!(saved.transcription_status, "pending");
        assert!(manager
            .start_meeting_with(
                start_input(true, false),
                Some("mic".into()),
                None,
                |_, _, _| panic!("capture after shutdown")
            )
            .is_err());
        assert!(manager.transcribe_meeting(&id, None).is_err());
        assert!(manager.diarize_meeting(&id, None).is_err());
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    fn fake_capture(
        manager: &MeetingManager,
        storage: &StorageManager,
        source: Source,
        meeting_id: String,
        bytes: usize,
    ) -> Result<RecordingState, String> {
        let path = match source {
            Source::Microphone => storage.get_microphone_path(&meeting_id),
            Source::System => storage.get_system_audio_path(&meeting_id),
        }
        .unwrap();
        assert!(path.parent().unwrap().is_dir());
        let current = RecordingState {
            meeting_id: Some(meeting_id.clone()),
            status: "recording".to_owned(),
            bytes_written: 0,
            error: None,
        };
        let shared = Arc::new(Mutex::new(current.clone()));
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let worker_id = meeting_id.clone();
        let worker = thread::spawn(move || {
            while !thread_stop.load(Ordering::Acquire) {
                thread::yield_now();
            }
            let mut wav = WavWriter::create(&path).unwrap();
            wav.write_samples(&vec![1_u8; bytes]).unwrap();
            RecordingState {
                meeting_id: Some(worker_id),
                status: "completed".to_owned(),
                bytes_written: wav.finish().unwrap(),
                error: None,
            }
        });
        *manager.state.lock().unwrap().active_mut(source) = Some(ActiveRecording {
            meeting_id,
            stop,
            worker,
            state: shared,
        });
        Ok(current)
    }

    #[test]
    fn diagnostics_record_partial_capture_failures_without_sensitive_context() {
        let root = root();
        let storage = StorageManager::initialize_in(&root).unwrap();
        let database = Database::initialize(&storage).unwrap();
        let manager = MeetingManager::new(database, storage.clone());
        let mut input = start_input(true, true);
        input.title = "Título secreto da reunião".into();
        input.video_enabled = true;
        input.video_source_id = Some("window:secret-window-title".into());
        manager
            .start_meeting_with_errors(
                input,
                None,
                Some("secret-device-name".into()),
                (
                    Some("device disconnected 0x88890004 C:\\Users\\Pessoa\\segredo".into()),
                    None,
                ),
                |source, id, _| fake_capture(&manager, &storage, source, id, 96_000),
                |_, _, _| Err("video HRESULT 0x80070005 secret-window-title".into()),
            )
            .unwrap();
        let result = manager.stop_meeting().unwrap();
        assert_eq!(result.meeting.unwrap().status, "failed_partial");
        let log = fs::read_to_string(storage.log_path()).unwrap();
        for event in [
            "recording_started",
            "recording_finished",
            "wasapi_microphone_start_failed",
            "video_start_failed",
        ] {
            assert!(log.contains(event), "missing {event}");
        }
        assert!(log.contains("native_code=0x88890004"));
        assert!(log.contains("native_code=0x80070005"));
        for secret in [
            "Título",
            "secret-window-title",
            "secret-device-name",
            "segredo",
            "Pessoa",
        ] {
            assert!(!log.contains(secret));
        }
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cannot_transcribe_while_a_meeting_is_recording() {
        let root = root();
        let storage = StorageManager::initialize_in(&root).unwrap();
        let database = Database::initialize(&storage).unwrap();
        let manager = MeetingManager::new(database.clone(), storage.clone());
        let started = manager
            .start_meeting_with(
                start_input(true, false),
                Some("mic".to_owned()),
                None,
                |source, id, _| fake_capture(&manager, &storage, source, id, 96_000),
            )
            .unwrap();
        let id = started.meeting.unwrap().id;
        assert!(manager.transcribe_meeting(&id, None).is_err());
        assert_eq!(
            meetings::get(&database.connect().unwrap(), &id)
                .unwrap()
                .unwrap()
                .transcription_status,
            "pending"
        );
        manager.stop_meeting().unwrap();
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn validates_configuration_before_creating_a_record() {
        let root = root();
        let storage = StorageManager::initialize_in(&root).unwrap();
        let database = Database::initialize(&storage).unwrap();
        let manager = MeetingManager::new(database.clone(), storage);
        let mut empty_title = start_input(true, false);
        empty_title.title = "  ".to_owned();
        let mut video = start_input(true, false);
        video.video_enabled = true;
        for input in [empty_title, video, start_input(false, false)] {
            assert!(manager
                .start_meeting_with(input, Some("mic".to_owned()), None, |_, _, _| panic!(
                    "capture should not start"
                ))
                .is_err());
        }
        let connection = database.connect().unwrap();
        assert!(meetings::list(&connection).unwrap().is_empty());
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn selects_default_or_explicit_device_and_rejects_missing_id() {
        let devices = [("one", false), ("two", true)];
        assert_eq!(
            select_device_id(None, devices.into_iter(), "output").unwrap(),
            "two"
        );
        assert_eq!(
            select_device_id(Some("one"), devices.into_iter(), "output").unwrap(),
            "one"
        );
        assert!(select_device_id(Some("missing"), devices.into_iter(), "output").is_err());
        assert!(select_device_id(None, std::iter::empty(), "output").is_err());
    }

    #[test]
    fn starts_and_stops_both_sources_and_persists_max_duration() {
        let root = root();
        let storage = StorageManager::initialize_in(&root).unwrap();
        let database = Database::initialize(&storage).unwrap();
        let manager = MeetingManager::new(database.clone(), storage.clone());
        let started = manager
            .start_meeting_with(
                start_input(true, true),
                Some("mic".to_owned()),
                Some("out".to_owned()),
                |source, id, _| {
                    fake_capture(
                        &manager,
                        &storage,
                        source,
                        id,
                        if source == Source::Microphone {
                            96_000
                        } else {
                            192_000
                        },
                    )
                },
            )
            .unwrap();
        let id = started.meeting.unwrap().id;
        assert!(started.active);
        assert_eq!(
            manager
                .get_recording_state()
                .unwrap()
                .meeting
                .unwrap()
                .status,
            "recording"
        );
        assert!(manager
            .start_meeting_with(
                start_input(true, false),
                Some("mic".to_owned()),
                None,
                |_, _, _| panic!("second meeting must be rejected")
            )
            .is_err());
        assert!(manager.delete_meeting(&id).is_err());
        let stopped = manager.stop_meeting().unwrap();
        assert!(!stopped.active);
        let meeting = stopped.meeting.unwrap();
        assert_eq!(meeting.status, "completed");
        assert_eq!(meeting.duration_seconds, 2);
        assert_eq!(meeting.transcription_status, "pending");
        assert!(meeting.finished_at.is_some());
        assert!(meeting.microphone_path.is_some());
        assert!(meeting.system_audio_path.is_some());
        assert!(meeting.merged_audio_path.is_some());
        let connection = database.connect().unwrap();
        assert_eq!(meetings::get(&connection, &id).unwrap().unwrap(), meeting);
        let session = manager.state.lock().unwrap().last_meeting.clone().unwrap();
        let retried = manager
            .finish_session(
                &session,
                AudioRecordingStates {
                    microphone: None,
                    system: None,
                },
            )
            .unwrap();
        assert_eq!(retried.meeting.unwrap().status, "completed");
        assert!(manager.stop_meeting().is_err());
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn complete_lifecycle_transcribes_locally_and_reopens_persisted_meeting_after_partial_failure()
    {
        // The device boundary uses a public speech fixture. Orchestration, WAV preparation,
        // whisper.cpp, local speaker attribution, SQLite and exports use the real modules.
        for fail_system in [false, true] {
            let root = root();
            let storage = StorageManager::initialize_in(&root).unwrap();
            let database = Database::initialize(&storage).unwrap();
            crate::database::settings::save(
                &database.connect().unwrap(),
                &crate::settings::AppSettings {
                    language: "en".into(),
                    max_threads: 2,
                    ..Default::default()
                },
            )
            .unwrap();
            let manager = MeetingManager::new(database.clone(), storage.clone());
            let fixture =
                fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/jfk.wav"))
                    .unwrap();
            let mut chunk = 12;
            while &fixture[chunk..chunk + 4] != b"data" {
                let size =
                    u32::from_le_bytes(fixture[chunk + 4..chunk + 8].try_into().unwrap()) as usize;
                chunk += 8 + size + size % 2;
            }
            let size =
                u32::from_le_bytes(fixture[chunk + 4..chunk + 8].try_into().unwrap()) as usize;
            let samples: Vec<u8> = fixture[chunk + 8..chunk + 8 + size]
                .as_chunks::<2>()
                .0
                .iter()
                .flat_map(|sample| sample.iter().copied().cycle().take(6))
                .collect();
            let started = manager
                .start_meeting_with(
                    start_input(true, fail_system),
                    Some("fixture-mic".into()),
                    fail_system.then(|| "unavailable-output".into()),
                    |source, id, _| {
                        if source == Source::System {
                            return Err("simulated output device loss".into());
                        }
                        let path = storage.get_microphone_path(&id).unwrap();
                        let mut wav = WavWriter::create(&path).unwrap();
                        wav.write_samples(&samples).unwrap();
                        let state = RecordingState {
                            meeting_id: Some(id.clone()),
                            status: "recording".into(),
                            bytes_written: samples.len() as u32,
                            error: None,
                        };
                        let shared = Arc::new(Mutex::new(state.clone()));
                        let stop = Arc::new(AtomicBool::new(false));
                        let worker_stop = stop.clone();
                        let worker = thread::spawn(move || {
                            while !worker_stop.load(Ordering::Acquire) {
                                thread::sleep(std::time::Duration::from_millis(10));
                            }
                            RecordingState {
                                meeting_id: Some(id.clone()),
                                status: "completed".into(),
                                bytes_written: wav.finish().unwrap(),
                                error: None,
                            }
                        });
                        *manager.state.lock().unwrap().active_mut(source) = Some(ActiveRecording {
                            meeting_id: state.meeting_id.clone().unwrap(),
                            stop,
                            worker,
                            state: shared,
                        });
                        Ok(state)
                    },
                )
                .unwrap();
            let id = started.meeting.unwrap().id;
            assert!(started.active);
            assert!(!manager.transcription.is_processing().unwrap());
            assert!(manager.transcribe_meeting(&id, None).is_err());
            assert!(!storage.get_transcript_path(&id).unwrap().exists());
            let stopped = manager.stop_meeting_and_transcribe(Some(2)).unwrap();
            assert!(!stopped.active);
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
            while manager.transcription.is_processing().unwrap() {
                assert!(
                    std::time::Instant::now() < deadline,
                    "local pipeline timed out"
                );
                thread::sleep(std::time::Duration::from_millis(25));
            }
            assert_eq!(
                manager.get_transcription_state(&id).unwrap().status,
                "completed"
            );
            drop(manager);
            drop(database);
            let reopened_storage = StorageManager::initialize_in(&root).unwrap();
            let reopened = Database::initialize(&reopened_storage).unwrap();
            let connection = reopened.connect().unwrap();
            let saved = meetings::get(&connection, &id).unwrap().unwrap();
            assert_eq!(
                saved.status,
                if fail_system {
                    "failed_partial"
                } else {
                    "completed"
                }
            );
            assert_eq!(saved.transcription_status, "completed");
            assert!(saved.duration_seconds >= 10);
            assert!(saved
                .transcription
                .as_ref()
                .unwrap()
                .to_lowercase()
                .contains("country"));
            assert_eq!(
                fs::read_to_string(reopened_storage.get_transcript_path(&id).unwrap()).unwrap(),
                saved.transcription.clone().unwrap()
            );
            assert!(crate::audio::mixer::inspect_output(
                &reopened_storage.get_merged_audio_path(&id).unwrap()
            )
            .is_ok());
            assert!(
                crate::audio::mixer::valid_source_bytes(
                    &reopened_storage.get_microphone_path(&id).unwrap()
                ) > 0
            );
            let identified = crate::database::segments::read(&connection, &id).unwrap();
            assert_eq!(identified.status, "completed");
            assert!(!identified.segments.is_empty());
            assert!(identified
                .segments
                .iter()
                .all(|segment| segment.diarization_label == "Você"));
            let destination = root.join("exportação-ação.txt");
            super::super::details::export_transcript(
                &reopened,
                &reopened_storage,
                &id,
                &destination,
            )
            .unwrap();
            assert!(fs::read_to_string(destination).unwrap().contains("Você"));
            drop(connection);
            drop(reopened);
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn microphone_start_failure_keeps_system_recording() {
        let root = root();
        let storage = StorageManager::initialize_in(&root).unwrap();
        let database = Database::initialize(&storage).unwrap();
        let manager = MeetingManager::new(database.clone(), storage.clone());
        let started = manager
            .start_meeting_with(
                start_input(true, true),
                Some("mic".to_owned()),
                Some("out".to_owned()),
                |source, id, _| {
                    if source == Source::Microphone {
                        Err("microphone unavailable".to_owned())
                    } else {
                        fake_capture(&manager, &storage, source, id, 96_000)
                    }
                },
            )
            .unwrap();
        assert!(started.active);
        assert_eq!(started.meeting.as_ref().unwrap().status, "failed_partial");
        assert!(started
            .errors
            .iter()
            .any(|error| error.contains("microphone unavailable")));
        let stopped = manager.stop_meeting().unwrap();
        let meeting = stopped.meeting.unwrap();
        assert_eq!(meeting.status, "failed_partial");
        assert!(meeting.microphone_path.is_none());
        assert!(meeting.system_audio_path.is_some());
        assert!(meeting.merged_audio_path.is_some());
        assert_eq!(meeting.duration_seconds, 1);
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unavailable_microphone_device_does_not_block_system_capture() {
        let root = root();
        let storage = StorageManager::initialize_in(&root).unwrap();
        let database = Database::initialize(&storage).unwrap();
        let manager = MeetingManager::new(database, storage.clone());
        let started = manager
            .start_meeting_with_errors(
                start_input(true, true),
                None,
                Some("out".to_owned()),
                (
                    Some("No active microphone device is available".to_owned()),
                    None,
                ),
                |source, id, _| {
                    assert!(source == Source::System);
                    fake_capture(&manager, &storage, source, id, 96_000)
                },
                |_, _, _| panic!("Video is disabled"),
            )
            .unwrap();
        assert!(started.active);
        assert_eq!(started.meeting.as_ref().unwrap().status, "failed_partial");
        assert!(started
            .errors
            .iter()
            .any(|error| error.contains("No active microphone")));
        let stopped = manager.stop_meeting().unwrap();
        let meeting = stopped.meeting.unwrap();
        assert_eq!(meeting.status, "failed_partial");
        assert!(meeting.microphone_path.is_none());
        assert!(meeting.system_audio_path.is_some());
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn merge_failure_keeps_raw_audio_and_records_partial_status() {
        let root = root();
        let storage = StorageManager::initialize_in(&root).unwrap();
        let database = Database::initialize(&storage).unwrap();
        let manager = MeetingManager::new(database, storage.clone());
        let started = manager
            .start_meeting_with(
                start_input(true, false),
                Some("mic".to_owned()),
                None,
                |source, id, _| fake_capture(&manager, &storage, source, id, 96_000),
            )
            .unwrap();
        let id = started.meeting.unwrap().id;
        let merged = storage.get_merged_audio_path(&id).unwrap();
        fs::write(&merged, b"existing damaged merged audio").unwrap();
        let stopped = manager.stop_meeting().unwrap();
        let meeting = stopped.meeting.unwrap();
        assert_eq!(meeting.status, "failed_partial");
        assert_eq!(meeting.transcription_status, "failed");
        assert!(meeting.microphone_path.is_some());
        assert!(meeting.merged_audio_path.is_none());
        assert!(storage.get_microphone_path(&id).unwrap().exists());
        assert_eq!(fs::read(&merged).unwrap(), b"existing damaged merged audio");
        assert!(stopped
            .errors
            .iter()
            .any(|error| error.contains("Audio preparation failed")));
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn retry_transcription_prepares_preserved_sources_after_file_failure() {
        struct SuccessRunner;
        impl crate::transcription::TranscriptionRunner for SuccessRunner {
            fn run(
                &self,
                _: &std::path::Path,
                _: &std::path::Path,
                _: &str,
                _: usize,
                _: &AtomicBool,
                _: &mut dyn FnMut(u8),
            ) -> Result<String, crate::transcription::TranscriptionFailure> {
                Ok("Áudio preservado após erro de arquivo".into())
            }
        }
        let root = root();
        let storage = StorageManager::initialize_in(&root).unwrap();
        let database = Database::initialize(&storage).unwrap();
        let mut manager = MeetingManager::new(database.clone(), storage.clone());
        manager.transcription = crate::transcription::TranscriptionEngine::new(
            database.clone(),
            storage.clone(),
            Arc::new(SuccessRunner),
        );
        let started = manager
            .start_meeting_with(
                start_input(true, false),
                Some("mic".into()),
                None,
                |source, id, _| fake_capture(&manager, &storage, source, id, 96_000),
            )
            .unwrap();
        let id = started.meeting.unwrap().id;
        let merged = storage.get_merged_audio_path(&id).unwrap();
        fs::create_dir(&merged).unwrap();
        let stopped = manager.stop_meeting().unwrap();
        assert_eq!(stopped.meeting.unwrap().transcription_status, "failed");
        let microphone = storage.get_microphone_path(&id).unwrap();
        let original = fs::read(&microphone).unwrap();
        assert!(manager.transcribe_meeting(&id, None).is_err());
        assert!(merged.is_dir());
        fs::remove_dir(&merged).unwrap();
        manager.transcribe_meeting(&id, None).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while manager.transcription.is_processing().unwrap() {
            assert!(std::time::Instant::now() < deadline);
            thread::sleep(std::time::Duration::from_millis(10));
        }
        let saved = meetings::get(&database.connect().unwrap(), &id)
            .unwrap()
            .unwrap();
        assert_eq!(saved.transcription_status, "completed");
        assert_eq!(saved.status, "failed_partial");
        assert_eq!(
            saved.merged_audio_path.as_deref(),
            Some(merged.to_string_lossy().as_ref())
        );
        assert_eq!(fs::read(&microphone).unwrap(), original);
        assert!(crate::audio::mixer::inspect_output(&merged).is_ok());
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn panicking_microphone_worker_preserves_both_wav_files() {
        let root = root();
        let storage = StorageManager::initialize_in(&root).unwrap();
        let database = Database::initialize(&storage).unwrap();
        let manager = MeetingManager::new(database, storage.clone());
        let started = manager
            .start_meeting_with(
                start_input(true, true),
                Some("mic".to_owned()),
                Some("out".to_owned()),
                |source, id, _| {
                    if source == Source::System {
                        return fake_capture(&manager, &storage, source, id, 96_000);
                    }
                    let path = storage.get_microphone_path(&id).unwrap();
                    let current = RecordingState {
                        meeting_id: Some(id.clone()),
                        status: "recording".to_owned(),
                        ..RecordingState::default()
                    };
                    let shared = Arc::new(Mutex::new(current.clone()));
                    let stop = Arc::new(AtomicBool::new(false));
                    let thread_stop = stop.clone();
                    let worker = thread::spawn(move || {
                        while !thread_stop.load(Ordering::Acquire) {
                            thread::yield_now();
                        }
                        let mut wav = WavWriter::create(&path).unwrap();
                        wav.write_samples(&vec![1_u8; 96_000]).unwrap();
                        wav.finish().unwrap();
                        panic!("simulated capture crash");
                    });
                    *manager.state.lock().unwrap().active_mut(source) = Some(ActiveRecording {
                        meeting_id: id,
                        stop,
                        worker,
                        state: shared,
                    });
                    Ok(current)
                },
            )
            .unwrap();
        assert!(started.active);
        let stopped = manager.stop_meeting().unwrap();
        let meeting = stopped.meeting.unwrap();
        assert_eq!(meeting.status, "failed_partial");
        assert!(meeting.microphone_path.is_some());
        assert!(meeting.system_audio_path.is_some());
        assert_eq!(meeting.duration_seconds, 1);
        assert!(stopped
            .errors
            .iter()
            .any(|error| error.contains("panicked")));
        drop(manager);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn all_sources_failing_preserves_failed_meeting_and_allows_next_one() {
        let root = root();
        let storage = StorageManager::initialize_in(&root).unwrap();
        let database = Database::initialize(&storage).unwrap();
        let manager = MeetingManager::new(database.clone(), storage);
        let failed = manager
            .start_meeting_with(
                start_input(true, true),
                Some("mic".to_owned()),
                Some("out".to_owned()),
                |_, _, _| Err("device unavailable".to_owned()),
            )
            .unwrap();
        assert!(!failed.active);
        assert_eq!(failed.meeting.as_ref().unwrap().status, "failed");
        assert!(failed.meeting.as_ref().unwrap().finished_at.is_some());
        assert!(manager
            .start_meeting_with(
                start_input(true, false),
                Some("mic".to_owned()),
                None,
                |_, _, _| Err("still unavailable".to_owned())
            )
            .is_ok());
        let connection = database.connect().unwrap();
        assert_eq!(meetings::list(&connection).unwrap().len(), 2);
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_missing_or_disabled_meetings_without_creating_wav() {
        let root = root();
        let storage = StorageManager::initialize_in(&root).unwrap();
        let database = Database::initialize(&storage).unwrap();
        assert!(prepare_recording(&database, &storage, "missing", Source::Microphone).is_err());
        let connection = database.connect().unwrap();
        let disabled = meetings::create(
            &connection,
            &NewMeeting {
                title: "Sem mic".to_owned(),
                microphone_enabled: false,
                system_audio_enabled: false,
                video_enabled: false,
            },
        )
        .unwrap();
        assert!(prepare_recording(&database, &storage, &disabled.id, Source::Microphone).is_err());
        assert!(!storage.get_microphone_path(&disabled.id).unwrap().exists());
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn existing_wav_is_preserved_before_capture_starts() {
        let root = root();
        let storage = StorageManager::initialize_in(&root).unwrap();
        let database = Database::initialize(&storage).unwrap();
        let connection = database.connect().unwrap();
        let meeting = meetings::create(
            &connection,
            &NewMeeting {
                title: "Com mic".to_owned(),
                microphone_enabled: true,
                system_audio_enabled: false,
                video_enabled: false,
            },
        )
        .unwrap();
        storage.create_meeting_directory(&meeting.id).unwrap();
        let wav = storage.get_microphone_path(&meeting.id).unwrap();
        fs::write(&wav, b"existing").unwrap();
        assert!(prepare_recording(&database, &storage, &meeting.id, Source::Microphone).is_err());
        assert_eq!(fs::read(wav).unwrap(), b"existing");
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn device_failure_keeps_valid_audio_and_persists_partial_status() {
        let root = root();
        let storage = StorageManager::initialize_in(&root).unwrap();
        let database = Database::initialize(&storage).unwrap();
        let connection = database.connect().unwrap();
        let meeting = meetings::create(
            &connection,
            &NewMeeting {
                title: "Interrompida".to_owned(),
                microphone_enabled: true,
                system_audio_enabled: false,
                video_enabled: false,
            },
        )
        .unwrap();
        let path = prepare_recording(&database, &storage, &meeting.id, Source::Microphone).unwrap();
        let mut writer = WavWriter::create(&path).unwrap();
        writer.write_samples(&vec![1_u8; 96_000]).unwrap();
        drop(writer);

        let state = finalize_recording(
            &database,
            &meeting.id,
            &path,
            Err("device disconnected".to_owned()),
            Source::Microphone,
            &CaptureCoordination::default(),
        );
        assert_eq!(state.status, "failed_partial");
        assert_eq!(state.bytes_written, 96_000);
        assert_eq!(state.error.as_deref(), Some("device disconnected"));
        let saved = meetings::get(&connection, &meeting.id).unwrap().unwrap();
        assert_eq!(saved.status, "failed_partial");
        assert_eq!(saved.duration_seconds, 1);
        assert_eq!(
            saved.microphone_path.as_deref(),
            Some(path.to_string_lossy().as_ref())
        );
        assert_eq!(fs::metadata(&path).unwrap().len(), 96_044);
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn active_recording_cannot_be_deleted() {
        let root = root();
        let storage = StorageManager::initialize_in(&root).unwrap();
        let database = Database::initialize(&storage).unwrap();
        let connection = database.connect().unwrap();
        let meeting = meetings::create(
            &connection,
            &NewMeeting {
                title: "Em andamento".to_owned(),
                microphone_enabled: true,
                system_audio_enabled: false,
                video_enabled: false,
            },
        )
        .unwrap();
        let path = prepare_recording(&database, &storage, &meeting.id, Source::Microphone).unwrap();
        fs::write(&path, b"valid bytes").unwrap();
        let manager = MeetingManager::new(database.clone(), storage.clone());
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let worker = thread::spawn(move || {
            while !worker_stop.load(Ordering::Acquire) {
                thread::sleep(std::time::Duration::from_millis(10));
            }
            RecordingState::default()
        });
        manager.state.lock().unwrap().active_microphone = Some(ActiveRecording {
            meeting_id: meeting.id.clone(),
            stop,
            worker,
            state: Arc::new(Mutex::new(RecordingState::default())),
        });

        assert!(manager.delete_meeting(&meeting.id).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"valid bytes");
        assert!(meetings::get(&connection, &meeting.id).unwrap().is_some());
        manager.stop_audio_sources().unwrap();
        assert!(manager.delete_meeting(&meeting.id).unwrap());
        assert!(!path.exists());
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn system_path_and_partial_failure_keep_microphone_metadata() {
        let root = root();
        let storage = StorageManager::initialize_in(&root).unwrap();
        let database = Database::initialize(&storage).unwrap();
        let connection = database.connect().unwrap();
        let meeting = meetings::create(
            &connection,
            &NewMeeting {
                title: "Duas fontes".to_owned(),
                microphone_enabled: true,
                system_audio_enabled: true,
                video_enabled: false,
            },
        )
        .unwrap();
        let mic_path =
            prepare_recording(&database, &storage, &meeting.id, Source::Microphone).unwrap();
        let sys_path = prepare_recording(&database, &storage, &meeting.id, Source::System).unwrap();
        assert_ne!(mic_path, sys_path);
        for path in [&mic_path, &sys_path] {
            let mut writer = WavWriter::create(path).unwrap();
            writer.write_samples(&vec![1_u8; 96_000]).unwrap();
            writer.finish().unwrap();
        }
        let coordination = CaptureCoordination::default();
        coordination.system_running.store(true, Ordering::Release);
        finalize_recording(
            &database,
            &meeting.id,
            &mic_path,
            Err("mic disconnected".to_owned()),
            Source::Microphone,
            &coordination,
        );
        let intermediate = meetings::get(&connection, &meeting.id).unwrap().unwrap();
        assert_eq!(intermediate.status, "failed_partial");
        assert!(intermediate.finished_at.is_none());
        assert_eq!(
            intermediate.microphone_path.as_deref(),
            Some(mic_path.to_string_lossy().as_ref())
        );
        finalize_recording(
            &database,
            &meeting.id,
            &sys_path,
            Ok(96_000),
            Source::System,
            &coordination,
        );
        let saved = meetings::get(&connection, &meeting.id).unwrap().unwrap();
        assert_eq!(saved.status, "failed_partial");
        assert!(saved.finished_at.is_some());
        assert_eq!(
            saved.microphone_path.as_deref(),
            Some(mic_path.to_string_lossy().as_ref())
        );
        assert_eq!(
            saved.system_audio_path.as_deref(),
            Some(sys_path.to_string_lossy().as_ref())
        );
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_disabled_system_audio() {
        let root = root();
        let storage = StorageManager::initialize_in(&root).unwrap();
        let database = Database::initialize(&storage).unwrap();
        let connection = database.connect().unwrap();
        let meeting = meetings::create(
            &connection,
            &NewMeeting {
                title: "Sem sistema".to_owned(),
                microphone_enabled: true,
                system_audio_enabled: false,
                video_enabled: false,
            },
        )
        .unwrap();
        assert!(prepare_recording(&database, &storage, &meeting.id, Source::System).is_err());
        assert!(!storage.get_system_audio_path(&meeting.id).unwrap().exists());
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn stopping_both_sources_signals_both_before_joining() {
        let root = root();
        let storage = StorageManager::initialize_in(&root).unwrap();
        let database = Database::initialize(&storage).unwrap();
        let manager = MeetingManager::new(database, storage);
        let mic_stop = Arc::new(AtomicBool::new(false));
        let sys_stop = Arc::new(AtomicBool::new(false));
        let mic_thread_stop = mic_stop.clone();
        let sys_observed_by_mic = sys_stop.clone();
        let mic_worker = thread::spawn(move || {
            while !mic_thread_stop.load(Ordering::Acquire) {
                thread::yield_now();
            }
            assert!(sys_observed_by_mic.load(Ordering::Acquire));
            RecordingState::default()
        });
        let sys_thread_stop = sys_stop.clone();
        let sys_worker = thread::spawn(move || {
            while !sys_thread_stop.load(Ordering::Acquire) {
                thread::yield_now();
            }
            RecordingState::default()
        });
        let mut state = manager.state.lock().unwrap();
        state.active_microphone = Some(ActiveRecording {
            meeting_id: "meeting-1".to_owned(),
            stop: mic_stop,
            worker: mic_worker,
            state: Arc::new(Mutex::new(RecordingState::default())),
        });
        state.active_system = Some(ActiveRecording {
            meeting_id: "meeting-1".to_owned(),
            stop: sys_stop,
            worker: sys_worker,
            state: Arc::new(Mutex::new(RecordingState::default())),
        });
        drop(state);
        let result = manager.stop_audio_sources().unwrap();
        assert!(result.microphone.is_some());
        assert!(result.system.is_some());
        fs::remove_dir_all(root).unwrap();
    }
}
