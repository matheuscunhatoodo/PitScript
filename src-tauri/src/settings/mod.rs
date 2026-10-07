use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct AppSettings {
    pub microphone_device_id: Option<String>,
    pub output_device_id: Option<String>,
    pub transcription_model: String,
    pub language: String,
    pub max_threads: usize,
    pub video_resolution: String,
    pub video_fps: u32,
    pub recordings_directory: Option<String>,
}

impl Default for AppSettings {
    fn default() -> Self {
        let cpus = std::thread::available_parallelism().map_or(2, |n| n.get());
        Self {
            microphone_device_id: None,
            output_device_id: None,
            transcription_model: "base-q5_1".into(),
            language: "pt".into(),
            max_threads: (cpus / 2).clamp(1, 4),
            video_resolution: "720p".into(),
            video_fps: 15,
            recordings_directory: None,
        }
    }
}
impl AppSettings {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if !(1..=16).contains(&self.max_threads) {
            return Err("Escolha de 1 a 16 threads.".into());
        }
        if ![5, 10, 15, 30].contains(&self.video_fps) {
            return Err("FPS permitido: 5, 10, 15 ou 30.".into());
        }
        if !["480p", "720p", "1080p"].contains(&self.video_resolution.as_str()) {
            return Err("Resolução permitida: 480p, 720p ou 1080p.".into());
        }
        if !["pt", "en", "es", "fr", "de", "it", "ja", "zh", "auto"]
            .contains(&self.language.as_str())
        {
            return Err("Idioma de transcrição inválido.".into());
        }
        model_file(&self.transcription_model)?;
        for id in [&self.microphone_device_id, &self.output_device_id]
            .into_iter()
            .flatten()
        {
            if id.trim().is_empty() || id.len() > 2048 || id.chars().any(char::is_control) {
                return Err("Identificador de dispositivo inválido.".into());
            }
        }
        if let Some(path) = &self.recordings_directory {
            if path.len() > 4096
                || path.chars().any(char::is_control)
                || !std::path::Path::new(path).is_absolute()
            {
                return Err("Escolha um diretório local absoluto.".into());
            }
        }
        Ok(())
    }

    pub(crate) fn effective_threads(&self, requested: Option<usize>) -> Result<usize, String> {
        if requested == Some(0) {
            return Err("Thread count must be at least one".into());
        }
        let cpus = std::thread::available_parallelism().map_or(2, |n| n.get());
        Ok(requested
            .unwrap_or(self.max_threads)
            .min(self.max_threads)
            .min(cpus)
            .max(1))
    }
}

pub(crate) fn model_file(id: &str) -> Result<&'static str, String> {
    match id {
        "tiny-q5_1" => Ok("ggml-tiny-q5_1.bin"),
        "base-q5_1" => Ok("ggml-base-q5_1.bin"),
        "small-q5_1" => Ok("ggml-small-q5_1.bin"),
        _ => Err("Escolha um modelo multilíngue Q5 suportado.".into()),
    }
}
pub(crate) fn model_name(id: &str) -> &'static str {
    match id {
        "tiny-q5_1" => "Tiny Multilingual Q5",
        "small-q5_1" => "Small Multilingual Q5",
        _ => "Base Multilingual Q5",
    }
}
pub(crate) fn model_path(
    id: &str,
    models: &std::path::Path,
    resources: &std::path::Path,
) -> Result<std::path::PathBuf, String> {
    let filename = model_file(id)?;
    for directory in [models, resources] {
        let path = directory.join(filename);
        if std::fs::symlink_metadata(&path).is_ok_and(|m| {
            #[cfg(windows)]
            let link = {
                use std::os::windows::fs::MetadataExt;
                m.file_attributes() & 0x400 != 0
            };
            #[cfg(not(windows))]
            let link = m.file_type().is_symlink();
            m.is_file() && !link && m.len() > 0
        }) {
            return Ok(path);
        }
    }
    Err(format!(
        "Modelo local {filename} não está disponível. Nenhum download será feito."
    ))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelOption {
    pub id: String,
    pub name: String,
    pub available: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SettingsInfo {
    pub settings: AppSettings,
    pub effective_recordings_directory: String,
    pub models: Vec<ModelOption>,
    pub warnings: Vec<String>,
    pub busy: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_are_safe_and_validation_rejects_out_of_scope_values() {
        let defaults = AppSettings::default();
        assert_eq!(defaults.transcription_model, "base-q5_1");
        assert_eq!(defaults.language, "pt");
        assert!((1..=4).contains(&defaults.max_threads));
        assert_eq!(defaults.video_fps, 15);
        assert_eq!(defaults.video_resolution, "720p");
        assert!(defaults.validate().is_ok());
        for (threads, fps, resolution, language, model) in [
            (0, 15, "720p", "pt", "base-q5_1"),
            (17, 15, "720p", "pt", "base-q5_1"),
            (1, 120, "720p", "pt", "base-q5_1"),
            (1, 15, "4k", "pt", "base-q5_1"),
            (1, 15, "720p", "invalid", "base-q5_1"),
            (1, 15, "720p", "pt", "base.en"),
            (1, 15, "720p", "pt", "../model"),
        ] {
            let prefs = AppSettings {
                max_threads: threads,
                video_fps: fps,
                video_resolution: resolution.into(),
                language: language.into(),
                transcription_model: model.into(),
                ..defaults.clone()
            };
            assert!(prefs.validate().is_err(), "{prefs:?}");
        }
    }
}
