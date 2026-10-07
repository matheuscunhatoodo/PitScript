use std::{
    fs,
    io::{self, BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

#[derive(Debug)]
pub(crate) enum TranscriptionFailure {
    Cancelled,
    Failed(String),
}

pub(crate) trait TranscriptionRunner: Send + Sync {
    fn configured(
        &self,
        _model: &str,
        _models: &Path,
    ) -> Result<Option<std::sync::Arc<dyn TranscriptionRunner>>, String> {
        Ok(None)
    }
    fn run_timed(
        &self,
        _input: &Path,
        _output_base: &Path,
        _language: &str,
        _threads: usize,
        _cancel: &AtomicBool,
        _progress: &mut dyn FnMut(u8),
    ) -> Result<Vec<super::alignment::TimedText>, TranscriptionFailure> {
        Err(TranscriptionFailure::Failed(
            "Timed transcription is unavailable".into(),
        ))
    }
    fn run(
        &self,
        input: &Path,
        output_base: &Path,
        language: &str,
        threads: usize,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(u8),
    ) -> Result<String, TranscriptionFailure>;
}

#[derive(Clone)]
pub(crate) struct WhisperCli {
    executable: PathBuf,
    model: PathBuf,
}

impl WhisperCli {
    pub(crate) fn with_selected_model(&self, id: &str, models: &Path) -> Result<Self, String> {
        let resources = self.model.parent().unwrap_or_else(|| Path::new("."));
        let model = crate::settings::model_path(id, models, resources)?;
        Ok(Self::new(self.executable.clone(), model))
    }
    pub(crate) fn new(executable: PathBuf, model: PathBuf) -> Self {
        Self { executable, model }
    }

    pub(crate) fn command(
        &self,
        input: &Path,
        output_base: &Path,
        language: &str,
        threads: usize,
    ) -> Command {
        let mut command = Command::new(&self.executable);
        command
            .arg("--model")
            .arg(&self.model)
            .arg("--file")
            .arg(input)
            .arg("--language")
            .arg(language)
            .arg("--threads")
            .arg(threads.to_string())
            .arg("--processors")
            .arg("1")
            .arg("--output-txt")
            .arg("--output-file")
            .arg(output_base)
            .arg("--print-progress")
            .arg("--no-gpu")
            .current_dir(self.executable.parent().unwrap_or_else(|| Path::new(".")))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }
}

impl TranscriptionRunner for WhisperCli {
    fn configured(
        &self,
        model: &str,
        models: &Path,
    ) -> Result<Option<std::sync::Arc<dyn TranscriptionRunner>>, String> {
        Ok(Some(std::sync::Arc::new(
            self.with_selected_model(model, models)?,
        )))
    }
    fn run_timed(
        &self,
        input: &Path,
        output_base: &Path,
        language: &str,
        threads: usize,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(u8),
    ) -> Result<Vec<super::alignment::TimedText>, TranscriptionFailure> {
        let path = output_base.with_extension("json");
        if path.exists() || output_base.with_extension("txt").exists() {
            return Err(TranscriptionFailure::Failed(
                "Timed output already exists".into(),
            ));
        }
        let mut command = self.command(input, output_base, language, threads);
        command.arg("--output-json-full");
        let result = (|| {
            super::process::run(command, cancel, parse_progress, progress)?;
            if fs::metadata(&path)
                .map_err(|error| TranscriptionFailure::Failed(error.to_string()))?
                .len()
                > 64 * 1024 * 1024
            {
                return Err(TranscriptionFailure::Failed(
                    "Whisper timestamp output is too large".into(),
                ));
            }
            let json = fs::read_to_string(&path)
                .map_err(|error| TranscriptionFailure::Failed(error.to_string()))?;
            super::json::parse(&json).map_err(TranscriptionFailure::Failed)
        })();
        let _ = fs::remove_file(path);
        let _ = fs::remove_file(output_base.with_extension("txt"));
        result
    }
    fn run(
        &self,
        input: &Path,
        output_base: &Path,
        language: &str,
        threads: usize,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(u8),
    ) -> Result<String, TranscriptionFailure> {
        for (label, path) in [
            ("whisper-cli.exe", self.executable.as_path()),
            ("modelo Whisper", self.model.as_path()),
            ("merged.wav", input),
        ] {
            if !path.is_file() {
                return Err(TranscriptionFailure::Failed(format!(
                    "Required local {label} is unavailable"
                )));
            }
        }
        let output_path = output_base.with_extension("txt");
        let mut child = self
            .command(input, output_base, language, threads)
            .spawn()
            .map_err(|error| {
                TranscriptionFailure::Failed(format!("Could not start whisper.cpp: {error}"))
            })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            TranscriptionFailure::Failed("Whisper stdout is unavailable".to_owned())
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            TranscriptionFailure::Failed("Whisper stderr is unavailable".to_owned())
        })?;
        let output_reader = thread::spawn(move || {
            let _ = io::copy(&mut BufReader::new(stdout), &mut io::sink());
        });
        let (sender, receiver) = mpsc::channel();
        let error_reader = thread::spawn(move || {
            for line in BufReader::new(stderr).lines() {
                match line {
                    Ok(line) => {
                        if sender.send(line).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });
        let mut diagnostics = Vec::new();
        let status = loop {
            while let Ok(line) = receiver.try_recv() {
                if let Some(percent) = parse_progress(&line) {
                    progress(percent);
                }
                if diagnostics.len() == 6 {
                    diagnostics.remove(0);
                }
                diagnostics.push(line);
            }
            if cancel.load(Ordering::Acquire) {
                let _ = child.kill();
                let _ = child.wait();
                break Err(TranscriptionFailure::Cancelled);
            }
            match child.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) => thread::sleep(Duration::from_millis(50)),
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break Err(TranscriptionFailure::Failed(error.to_string()));
                }
            }
        };
        let _ = output_reader.join();
        let _ = error_reader.join();
        for line in receiver.try_iter() {
            if let Some(percent) = parse_progress(&line) {
                progress(percent);
            }
            if diagnostics.len() == 6 {
                diagnostics.remove(0);
            }
            diagnostics.push(line);
        }
        let result = match status {
            Err(error) => Err(error),
            Ok(exit) if !exit.success() => Err(TranscriptionFailure::Failed(format!(
                "whisper.cpp exited with {exit}: {}",
                diagnostics.join(" | ")
            ))),
            Ok(_) if cancel.load(Ordering::Acquire) => Err(TranscriptionFailure::Cancelled),
            Ok(_) => fs::read_to_string(&output_path)
                .map(|text| text.trim().to_owned())
                .map_err(|error| {
                    TranscriptionFailure::Failed(format!(
                        "Could not read whisper.cpp text output: {error}"
                    ))
                }),
        };
        let _ = fs::remove_file(output_path);
        if result.is_ok() {
            progress(100);
        }
        result
    }
}

pub(crate) fn parse_progress(line: &str) -> Option<u8> {
    let value = line.split("progress = ").nth(1)?;
    let digits = value
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>();
    if !value[digits.len()..].starts_with('%') {
        return None;
    }
    digits.parse::<u16>().ok().map(|value| value.min(100) as u8)
}

#[cfg(test)]
mod tests {
    #[test]
    fn selected_model_reconfigures_runner_path_and_rejects_missing_or_english_models() {
        use super::WhisperCli;
        use std::{
            fs,
            path::PathBuf,
            time::{SystemTime, UNIX_EPOCH},
        };
        let root = std::env::temp_dir().join(format!(
            "whisper-settings-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let model = root.join("ggml-tiny-q5_1.bin");
        fs::write(&model, b"synthetic model path test").unwrap();
        let cli = WhisperCli::new(
            PathBuf::from("whisper-cli.exe"),
            PathBuf::from("ggml-base-q5_1.bin"),
        );
        let selected = cli.with_selected_model("tiny-q5_1", &root).unwrap();
        let command = selected.command(&root.join("in.wav"), &root.join("out"), "es", 1);
        let args: Vec<_> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert!(args
            .windows(2)
            .any(|pair| pair == ["--model", model.to_string_lossy().as_ref()]));
        assert!(cli.with_selected_model("base.en", &root).is_err());
        assert!(cli.with_selected_model("small-q5_1", &root).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    use std::path::PathBuf;

    use super::{parse_progress, WhisperCli};

    #[test]
    fn parses_progress_and_caps_cli_values_over_one_hundred() {
        assert_eq!(
            parse_progress("whisper_print_progress_callback: progress = 37%"),
            Some(37)
        );
        assert_eq!(
            parse_progress("whisper_print_progress_callback: progress = 272%"),
            Some(100)
        );
        assert_eq!(parse_progress("unrelated diagnostic"), None);
    }

    #[test]
    fn command_uses_local_model_portuguese_output_and_bounded_threads() {
        let cli = WhisperCli::new(
            PathBuf::from("whisper-cli.exe"),
            PathBuf::from("ggml-base-q5_1.bin"),
        );
        let command = cli.command(
            PathBuf::from("merged.wav").as_path(),
            PathBuf::from("transcript-temp").as_path(),
            "pt",
            2,
        );
        let args: Vec<_> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert!(args.windows(2).any(|pair| pair == ["--threads", "2"]));
        assert!(args.windows(2).any(|pair| pair == ["--language", "pt"]));
        assert!(args
            .windows(2)
            .any(|pair| pair == ["--model", "ggml-base-q5_1.bin"]));
        assert!(args.windows(2).any(|pair| pair == ["--file", "merged.wav"]));
        assert!(args.contains(&"--output-txt".to_owned()));
        assert!(args.contains(&"--print-progress".to_owned()));
    }
}
