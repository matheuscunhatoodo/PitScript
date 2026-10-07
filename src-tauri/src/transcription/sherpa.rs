use super::alignment::SpeakerTurn;
use super::whisper::TranscriptionFailure;
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::AtomicBool,
};

pub trait DiarizationRunner: Send + Sync {
    fn run(
        &self,
        input: &Path,
        threads: usize,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(u8),
    ) -> Result<Vec<SpeakerTurn>, TranscriptionFailure>;
}
pub struct SherpaCli {
    resources: PathBuf,
}
impl SherpaCli {
    pub fn new(resources: PathBuf) -> Self {
        Self { resources }
    }
}
impl DiarizationRunner for SherpaCli {
    fn run(
        &self,
        input: &Path,
        threads: usize,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(u8),
    ) -> Result<Vec<SpeakerTurn>, TranscriptionFailure> {
        let executable = self
            .resources
            .join("sherpa-onnx-offline-speaker-diarization.exe");
        let segmentation = self.resources.join("segmentation.int8.onnx");
        let embedding = self.resources.join("wespeaker_resnet34_lm.onnx");
        if !executable.is_file() || !segmentation.is_file() || !embedding.is_file() {
            return Err(TranscriptionFailure::Failed(
                "Os modelos/runtime locais de diarização não estão disponíveis.".into(),
            ));
        }
        let mut command = Command::new(executable);
        command
            .current_dir(&self.resources)
            .arg(format!(
                "--segmentation.pyannote-model={}",
                segmentation.display()
            ))
            .arg(format!("--embedding.model={}", embedding.display()))
            .arg(format!(
                "--segmentation.num-threads={}",
                threads.clamp(1, 4)
            ))
            .arg(format!("--embedding.num-threads={}", threads.clamp(1, 4)))
            .args([
                "--segmentation.provider=cpu",
                "--embedding.provider=cpu",
                "--clustering.num-clusters=-1",
                "--clustering.cluster-threshold=0.5",
                "--clustering.compute-confidence=true",
                "--print-args=false",
            ])
            .arg(input);
        let output = super::process::run(command, cancel, parse_progress, progress)?;
        output
            .lines()
            .filter_map(|line| parse_turn(line).transpose())
            .collect::<Result<Vec<_>, _>>()
            .map_err(TranscriptionFailure::Failed)
    }
}

pub fn parse_turn(line: &str) -> Result<Option<SpeakerTurn>, String> {
    if !line.contains(" speaker_") {
        return Ok(None);
    }
    let fields: Vec<_> = line.split_whitespace().collect();
    if fields.len() < 4 || fields[1] != "--" {
        return Err("Invalid sherpa segment".into());
    }
    let time = |value: &str| -> Result<i64, String> {
        let seconds = value
            .parse::<f64>()
            .map_err(|_| "Invalid sherpa timestamp")?;
        if !seconds.is_finite() || !(0.0..=172800.0).contains(&seconds) {
            return Err("Invalid sherpa timestamp".into());
        }
        Ok((seconds * 1000.0).round() as i64)
    };
    let start_ms = time(fields[0])?;
    let end_ms = time(fields[2])?;
    let speaker = fields[3]
        .strip_prefix("speaker_")
        .ok_or("Invalid speaker label")?
        .parse::<i32>()
        .map_err(|_| "Invalid speaker label")?;
    let confidence = fields
        .get(4)
        .map(|value| -> Result<Option<f32>, String> {
            let value = value
                .strip_prefix("confidence=")
                .ok_or("Invalid confidence")?;
            // The official CLI reports n/a when clustering has only one group.
            if value == "n/a" {
                return Ok(None);
            }
            let value = value.parse::<f32>().map_err(|_| "Invalid confidence")?;
            if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                return Err("Invalid confidence".into());
            }
            Ok(Some(value))
        })
        .transpose()?
        .flatten();
    if end_ms <= start_ms || speaker < 0 {
        return Err("Invalid sherpa segment".into());
    }
    Ok(Some(SpeakerTurn {
        start_ms,
        end_ms,
        speaker,
        confidence,
    }))
}
pub fn parse_progress(line: &str) -> Option<u8> {
    let value = line
        .strip_prefix("progress ")?
        .strip_suffix('%')?
        .parse::<f32>()
        .ok()?;
    value.is_finite().then(|| value.clamp(0.0, 100.0) as u8)
}

#[cfg(test)]
mod tests {
    #[cfg(windows)]
    #[test]
    fn real_models_detect_one_two_and_three_public_speakers_without_forced_count() {
        use super::{DiarizationRunner, SherpaCli};
        use std::{collections::BTreeSet, path::PathBuf, sync::atomic::AtomicBool};
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let cli = SherpaCli::new(root.join("resources/diarization"));
        for (file, expected) in [
            ("jfk.wav", 1),
            ("two-speakers.wav", 2),
            ("three-speakers.wav", 3),
        ] {
            let mut updates = Vec::new();
            let result = cli
                .run(
                    &root.join("tests/fixtures").join(file),
                    2,
                    &AtomicBool::new(false),
                    &mut |value| updates.push(value),
                )
                .unwrap();
            let speakers: BTreeSet<_> = result.iter().map(|turn| turn.speaker).collect();
            assert_eq!(speakers.len(), expected, "{file}");
            assert!(result.iter().all(|turn| turn.end_ms > turn.start_ms));
            if expected > 1 {
                assert!(result.iter().all(|turn| turn.confidence.is_some()));
            }
            assert!(updates.contains(&100));
        }
    }
    #[test]
    fn parses_native_turns_confidence_progress_and_rejects_invalid_output() {
        let turn = super::parse_turn("1.617 -- 3.271 speaker_00 confidence=0.838")
            .unwrap()
            .unwrap();
        assert_eq!((turn.start_ms, turn.end_ms, turn.speaker), (1617, 3271, 0));
        assert_eq!(turn.confidence, Some(0.838));
        assert_eq!(
            super::parse_turn("0 -- 1 speaker_00 confidence=n/a")
                .unwrap()
                .unwrap()
                .confidence,
            None
        );
        assert!(super::parse_turn("Started").unwrap().is_none());
        assert!(super::parse_turn("3 -- 1 speaker_00 confidence=0.9").is_err());
        assert!(super::parse_turn("0 -- NaN speaker_00 confidence=0.9").is_err());
        assert!(super::parse_turn("0 -- 1 speaker_00 confidence=2").is_err());
        assert_eq!(super::parse_progress("progress 37.30%"), Some(37));
        assert_eq!(super::parse_progress("anything"), None);
    }

    #[cfg(windows)]
    #[test]
    fn cancels_native_inference_after_progress_and_waits_for_process_exit() {
        use super::{DiarizationRunner, SherpaCli};
        use std::{
            path::PathBuf,
            sync::atomic::{AtomicBool, Ordering},
        };
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let cli = SherpaCli::new(root.join("resources/diarization"));
        let cancel = AtomicBool::new(false);
        let mut seen = false;
        let result = cli.run(
            &root.join("tests/fixtures/three-speakers.wav"),
            2,
            &cancel,
            &mut |_| {
                seen = true;
                cancel.store(true, Ordering::Release);
            },
        );
        assert!(seen, "native CLI emitted no progress");
        assert!(matches!(
            result,
            Err(super::TranscriptionFailure::Cancelled)
        ));
    }
}
