use super::{
    alignment::align,
    sherpa::DiarizationRunner,
    whisper::{TranscriptionFailure, TranscriptionRunner},
};
use crate::{
    audio::mixer,
    database::{segments::NewSegment, Meeting},
    storage::StorageManager,
};
use std::{
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

pub trait SegmentProcessor: Send + Sync {
    fn run(
        &self,
        storage: &StorageManager,
        meeting: &Meeting,
        whisper: &dyn TranscriptionRunner,
        threads: usize,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(&str, u8),
    ) -> Result<Vec<NewSegment>, TranscriptionFailure>;
}
pub struct NativePipeline {
    diarizer: Arc<dyn DiarizationRunner>,
}
impl NativePipeline {
    pub fn new(diarizer: Arc<dyn DiarizationRunner>) -> Self {
        Self { diarizer }
    }
}
struct WorkDirectory(PathBuf);
impl Drop for WorkDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

impl SegmentProcessor for NativePipeline {
    fn run(
        &self,
        storage: &StorageManager,
        meeting: &Meeting,
        whisper: &dyn TranscriptionRunner,
        threads: usize,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(&str, u8),
    ) -> Result<Vec<NewSegment>, TranscriptionFailure> {
        if meeting.finished_at.is_none() || meeting.status == "recording" {
            return Err(TranscriptionFailure::Failed(
                "Finalize a reunião antes de identificar participantes.".into(),
            ));
        }
        let candidates = storage
            .playback_candidates(&meeting.id)
            .map_err(|error| TranscriptionFailure::Failed(error.to_string()))?;
        let source = |name: &str, enabled: bool| {
            candidates
                .iter()
                .find(|path| {
                    enabled
                        && path.file_name().is_some_and(|file| file == name)
                        && mixer::valid_source_bytes(path) > 0
                })
                .map(PathBuf::as_path)
        };
        let microphone = source("microphone.wav", meeting.microphone_enabled);
        let system = source("system.wav", meeting.system_audio_enabled);
        if microphone.is_none() && system.is_none() {
            return Err(TranscriptionFailure::Failed("As fontes originais não estão disponíveis; a transcrição tradicional foi preservada.".into()));
        }
        let offsets = mixer::source_offsets_ms(microphone, system);
        let directory = WorkDirectory(
            storage
                .create_postprocessing_directory(&meeting.id)
                .map_err(|error| TranscriptionFailure::Failed(error.to_string()))?,
        );
        let mut local = Vec::new();
        let mut remote = Vec::new();
        let mut turns = Vec::new();
        let mut prepared_system = None;
        for (name, input, destination, base) in [
            ("microphone", microphone, &mut local, 0),
            ("system", system, &mut remote, 35),
        ] {
            if cancel.load(Ordering::Acquire) {
                return Err(TranscriptionFailure::Cancelled);
            }
            let Some(input) = input else {
                continue;
            };
            progress("preparing_sources", base);
            let output = directory.0.join(format!("{name}.wav"));
            mixer::prepare_audio_cancellable(
                input,
                &directory.0.join("absent.wav"),
                &output,
                &|| cancel.load(Ordering::Acquire),
            )
            .map_err(|error| {
                if cancel.load(Ordering::Acquire) {
                    TranscriptionFailure::Cancelled
                } else {
                    TranscriptionFailure::Failed(error)
                }
            })?;
            *destination = whisper.run_timed(
                &output,
                &directory.0.join(format!("{name}-words")),
                &meeting.language,
                threads,
                cancel,
                &mut |value| {
                    progress(
                        "source_transcription",
                        base + (u16::from(value) * 30 / 100) as u8,
                    )
                },
            )?;
            if name == "system" {
                prepared_system = Some(output);
            }
        }
        if let Some(input) = prepared_system {
            progress("diarization", 70);
            turns = self.diarizer.run(&input, threads, cancel, &mut |value| {
                progress("diarization", 70 + (u16::from(value) * 25 / 100) as u8)
            })?;
        }
        if cancel.load(Ordering::Acquire) {
            return Err(TranscriptionFailure::Cancelled);
        }
        progress("alignment", 96);
        let output = align(&local, &remote, &turns, offsets.0, offsets.1);
        if output.is_empty() {
            return Err(TranscriptionFailure::Failed("Nenhum segmento de fala confiável foi reconhecido; o texto tradicional foi preservado.".into()));
        }
        Ok(output)
    }
}
