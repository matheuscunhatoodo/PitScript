use super::{
    engine::TranscriptionState,
    pipeline::SegmentProcessor,
    whisper::{TranscriptionFailure, TranscriptionRunner},
};
use crate::{
    database::{segments, Database, Meeting},
    storage::StorageManager,
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

pub struct Context {
    pub database: Database,
    pub storage: StorageManager,
    pub processor: Arc<dyn SegmentProcessor>,
    pub runner: Arc<dyn TranscriptionRunner>,
    pub meeting: Meeting,
    pub threads: usize,
    pub cancel: Arc<AtomicBool>,
    pub state: Arc<Mutex<TranscriptionState>>,
    pub progress_base: u8,
}
impl Context {
    pub fn run(self) {
        use crate::storage::diagnostics::Value;
        self.storage.log().event(
            "INFO",
            "diarization_started",
            &[
                ("meeting", Value::token(&self.meeting.id)),
                ("threads", Value::Count(self.threads as u64)),
            ],
        );
        let mut last_progress = 0;
        let mut last_stage = String::new();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut connection = self
                .database
                .connect()
                .map_err(|error| TranscriptionFailure::Failed(error.to_string()))?;
            segments::update_state(
                &connection,
                &self.meeting.id,
                "processing",
                "preparing_sources",
                0,
                None,
            )
            .map_err(|error| TranscriptionFailure::Failed(error.to_string()))?;
            let segments = self.processor.run(
                &self.storage,
                &self.meeting,
                self.runner.as_ref(),
                self.threads,
                &self.cancel,
                &mut |stage, progress| {
                    if let Ok(mut state) = self.state.lock() {
                        state.stage = stage.into();
                        state.progress = state
                            .progress
                            .max(
                                self.progress_base
                                    + ((u16::from(progress)
                                        * (100 - u16::from(self.progress_base)))
                                        / 100) as u8,
                            )
                            .min(99);
                    }
                    if progress >= last_progress + 5 || stage != last_stage {
                        let _ = segments::update_state(
                            &connection,
                            &self.meeting.id,
                            "processing",
                            stage,
                            progress.min(99),
                            None,
                        );
                        last_progress = progress;
                        last_stage = stage.into();
                    }
                },
            )?;
            if self.cancel.load(Ordering::Acquire) {
                return Err(TranscriptionFailure::Cancelled);
            }
            segments::replace(&mut connection, &self.meeting.id, &segments)
                .map_err(|error| TranscriptionFailure::Failed(error.to_string()))
        }))
        .unwrap_or_else(|_| {
            Err(TranscriptionFailure::Failed(
                "Falha no worker de diarização; o texto tradicional foi preservado.".into(),
            ))
        });
        let (stage, error) = match result {
            Ok(()) => ("completed", None),
            Err(TranscriptionFailure::Cancelled) => (
                "cancelled",
                Some("Diarização cancelada; a transcrição tradicional foi preservada.".to_owned()),
            ),
            Err(TranscriptionFailure::Failed(error)) => ("failed", Some(error)),
        };
        self.storage.log().event(
            "INFO",
            "diarization_finished",
            &[
                ("meeting", Value::token(&self.meeting.id)),
                ("status", Value::status(stage)),
            ],
        );
        if let Some(error) = &error {
            self.storage
                .log()
                .failure("diarization_failed", Some(&self.meeting.id), error);
        }
        if stage != "completed" {
            if let Ok(connection) = self.database.connect() {
                let _ = segments::update_state(
                    &connection,
                    &self.meeting.id,
                    stage,
                    "diarization",
                    last_progress,
                    error.as_deref(),
                );
            }
        }
        if let Ok(mut state) = self.state.lock() {
            // Whisper already completed. A secondary failure must not mark its text failed.
            state.status = "completed".into();
            state.progress = 100;
            state.stage = format!("diarization_{stage}");
            state.error = error;
        }
    }
}
