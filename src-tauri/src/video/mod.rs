mod capture;
mod encoder;
#[cfg(test)]
mod native_tests;
mod targets;
use serde::Serialize;
use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::sync_channel,
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
pub(crate) use targets::{list_sources, select_source, VideoSource};
use tauri::{AppHandle, Emitter};

pub(crate) const WIDTH: u32 = 1280;
pub(crate) const HEIGHT: u32 = 720;
pub(crate) const FPS: u64 = 15;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct VideoConfig {
    pub width: u32,
    pub height: u32,
    pub fps: u64,
}
impl Default for VideoConfig {
    fn default() -> Self {
        Self {
            width: WIDTH,
            height: HEIGHT,
            fps: FPS,
        }
    }
}
impl VideoConfig {
    pub(crate) fn from_settings(resolution: &str, fps: u32) -> Result<Self, String> {
        if ![5, 10, 15, 30].contains(&fps) {
            return Err("FPS de vídeo inválido.".into());
        }
        let (width, height) = match resolution {
            "480p" => (854, 480),
            "720p" => (1280, 720),
            "1080p" => (1920, 1080),
            _ => return Err("Resolução de vídeo inválida.".into()),
        };
        Ok(Self {
            width,
            height,
            fps: u64::from(fps),
        })
    }
}

#[cfg(test)]
pub(crate) fn fit_rect(width: u32, height: u32) -> Result<(i32, i32, i32, i32), String> {
    fit_rect_for(width, height, VideoConfig::default())
}
pub(crate) fn fit_rect_for(
    width: u32,
    height: u32,
    config: VideoConfig,
) -> Result<(i32, i32, i32, i32), String> {
    if width == 0 || height == 0 {
        return Err("A fonte de vídeo está sem área visível.".into());
    }
    let scale = (config.width as f64 / width as f64).min(config.height as f64 / height as f64);
    let w = ((width as f64 * scale) as u32 & !1).max(2);
    let h = ((height as f64 * scale) as u32 & !1).max(2);
    let x = (((config.width - w) / 2) & !1) as i32;
    let y = (((config.height - h) / 2) & !1) as i32;
    Ok((x, y, x + w as i32, y + h as i32))
}
#[cfg(test)]
pub(crate) fn next_frame_index(previous: Option<u64>, elapsed: Duration) -> u64 {
    frame_index_for(previous, elapsed, FPS)
}
pub(crate) fn frame_index_for(previous: Option<u64>, elapsed: Duration, fps: u64) -> u64 {
    let now = (elapsed.as_nanos() * fps as u128 / 1_000_000_000).min(u64::MAX as u128) as u64;
    previous.map_or(now, |index| now.max(index.saturating_add(1)))
}
pub(crate) fn valid_mp4(path: &Path) -> bool {
    fn check(path: &Path) -> std::io::Result<bool> {
        let mut file = File::open(path)?;
        let length = file.metadata()?.len();
        let mut position = 0_u64;
        let mut found = [false; 3];
        while position.checked_add(8).is_some_and(|end| end <= length) {
            let mut header = [0_u8; 8];
            file.read_exact(&mut header)?;
            let mut size = u64::from(u32::from_be_bytes(header[..4].try_into().unwrap()));
            let mut minimum = 8;
            if size == 1 {
                let mut extended = [0; 8];
                file.read_exact(&mut extended)?;
                size = u64::from_be_bytes(extended);
                minimum = 16;
            } else if size == 0 {
                size = length - position;
            }
            let Some(end) = position.checked_add(size) else {
                return Ok(false);
            };
            if size <= minimum || end > length {
                return Ok(false);
            }
            match &header[4..8] {
                b"ftyp" => found[0] = true,
                b"mdat" => found[1] = true,
                b"moov" => found[2] = true,
                _ => {}
            }
            position = end;
            file.seek(SeekFrom::Start(position))?;
        }
        Ok(position == length && found.into_iter().all(|present| present))
    }
    check(path).unwrap_or(false)
}

#[derive(Clone, Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VideoState {
    pub meeting_id: String,
    pub status: String,
    pub frames_written: u64,
    pub skipped_frames: u64,
    pub duration_ms: u64,
    pub encoder: String,
    pub hardware_accelerated: bool,
    pub finalized: bool,
    pub error: Option<String>,
    pub finalization_error: Option<String>,
}

pub(crate) struct VideoRecorder {
    pub stop: Arc<AtomicBool>,
    pub state: Arc<Mutex<VideoState>>,
    worker: Option<JoinHandle<VideoState>>,
    running: Arc<AtomicBool>,
}
impl VideoRecorder {
    #[cfg(test)]
    pub(crate) fn fake(path: PathBuf, id: String) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let state = Arc::new(Mutex::new(VideoState {
            meeting_id: id,
            status: "recording".into(),
            ..Default::default()
        }));
        let worker_stop = stop.clone();
        let worker_state = state.clone();
        let worker = thread::spawn(move || {
            while !worker_stop.load(Ordering::Acquire) {
                thread::sleep(Duration::from_millis(5));
            }
            let mut bytes = Vec::new();
            for kind in [b"ftyp", b"mdat", b"moov"] {
                bytes.extend_from_slice(&16_u32.to_be_bytes());
                bytes.extend_from_slice(kind);
                bytes.extend_from_slice(&[0; 8]);
            }
            std::fs::write(path, bytes).unwrap();
            let mut result = worker_state.lock().unwrap().clone();
            result.status = "completed".into();
            result.finalized = true;
            result.duration_ms = 2100;
            result.frames_written = 32;
            result
        });
        Self {
            stop,
            state,
            worker: Some(worker),
            running: Arc::new(AtomicBool::new(true)),
        }
    }
    #[cfg(test)]
    pub(crate) fn start(
        path: PathBuf,
        id: String,
        source: VideoSource,
        origin: Instant,
        app: Option<AppHandle>,
        log_path: PathBuf,
        running: Arc<AtomicBool>,
    ) -> Result<Self, String> {
        Self::start_configured(
            path,
            id,
            (source, origin, VideoConfig::default()),
            app,
            log_path,
            running,
        )
    }
    pub(crate) fn start_configured(
        path: PathBuf,
        id: String,
        selection: (VideoSource, Instant, VideoConfig),
        app: Option<AppHandle>,
        log_path: PathBuf,
        running: Arc<AtomicBool>,
    ) -> Result<Self, String> {
        let (source, origin, config) = selection;
        use crate::storage::diagnostics::{DiagnosticLog, Value};
        let log = DiagnosticLog::new(log_path);
        log.event(
            "INFO",
            "video_start_requested",
            &[
                ("meeting", Value::token(&id)),
                ("source", Value::token(&source.id)),
                ("fps", Value::Count(config.fps)),
            ],
        );
        // Never let the sink writer overwrite another recording or follow an existing link.
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .inspect_err(|error| log.io_failure("video_file_failed", error))
            .map_err(|e| e.to_string())?;
        let stop = Arc::new(AtomicBool::new(false));
        let state = Arc::new(Mutex::new(VideoState {
            meeting_id: id,
            status: "starting".into(),
            ..Default::default()
        }));
        let worker_stop = stop.clone();
        let worker_state = state.clone();
        let worker_running = running.clone();
        running.store(true, Ordering::Release);
        let (ready, receiver) = sync_channel(1);
        let worker = thread::Builder::new()
            .name("video-capture-encoder".into())
            .spawn(move || {
                let result = capture::record(
                    &path,
                    &source,
                    &worker_stop,
                    origin,
                    &ready,
                    &worker_state,
                    config,
                );
                let mut finished = worker_state
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .clone();
                if let Err(error) = result {
                    finished.error = Some(error.clone());
                    let _ = ready.try_send(Err(error));
                }
                finished.finalized = finished.finalization_error.is_none()
                    && finished.frames_written > 0
                    && valid_mp4(&path);
                if finished.finalized {
                    if let Err(error) = OpenOptions::new()
                        .write(true)
                        .open(&path)
                        .and_then(|file| file.sync_all())
                    {
                        finished.finalization_error = Some(error.to_string());
                        finished.finalized = false;
                    }
                }
                finished.status = if finished.error.is_none() && finished.finalized {
                    "completed"
                } else if finished.finalized {
                    "failed_partial"
                } else {
                    "failed"
                }
                .into();
                log.event(
                    "INFO",
                    "video_finished",
                    &[
                        ("meeting", Value::token(&finished.meeting_id)),
                        ("status", Value::status(&finished.status)),
                        ("hardware", Value::Flag(finished.hardware_accelerated)),
                        ("frames", Value::Count(finished.frames_written)),
                        ("skipped", Value::Count(finished.skipped_frames)),
                        ("duration_ms", Value::Count(finished.duration_ms)),
                    ],
                );
                if let Some(error) = &finished.error {
                    log.failure("video_capture_failed", Some(&finished.meeting_id), error);
                }
                if let Some(error) = &finished.finalization_error {
                    log.failure("video_finalize_failed", Some(&finished.meeting_id), error);
                }
                *worker_state.lock().unwrap_or_else(|e| e.into_inner()) = finished.clone();
                worker_running.store(false, Ordering::Release);
                if let Some(app) = app {
                    let _ = app.emit("video-recording-status", &finished);
                }
                finished
            })
            .map_err(|e| {
                running.store(false, Ordering::Release);
                e.to_string()
            })?;
        let mut recorder = Self {
            stop,
            state,
            worker: Some(worker),
            running,
        };
        match receiver.recv_timeout(Duration::from_secs(15)) {
            Ok(Ok(())) => Ok(recorder),
            other => {
                recorder.stop.store(true, Ordering::Release);
                let final_state = recorder.finish();
                Err(final_state
                    .error
                    .unwrap_or_else(|| format!("O vídeo não iniciou: {other:?}")))
            }
        }
    }
    pub(crate) fn snapshot(&self) -> VideoState {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
    pub(crate) fn finish(&mut self) -> VideoState {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            match worker.join() {
                Ok(state) => {
                    self.running.store(false, Ordering::Release);
                    return state;
                }
                Err(_) => {
                    let mut state = self.snapshot();
                    state.status = "failed".into();
                    state.error = Some("Worker de vídeo interrompido.".into());
                    state.finalization_error = state.error.clone();
                    self.running.store(false, Ordering::Release);
                    return state;
                }
            }
        }
        self.snapshot()
    }
}
impl Drop for VideoRecorder {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn video_profile_uses_selected_resolution_and_fps_and_rejects_invalid_values() {
        let low = VideoConfig::from_settings("480p", 10).unwrap();
        assert_eq!((low.width, low.height, low.fps), (854, 480, 10));
        assert_eq!(
            frame_index_for(Some(9), Duration::from_secs(2), low.fps),
            20
        );
        let (left, top, right, bottom) = fit_rect_for(1024, 1024, low).unwrap();
        assert_eq!(right - left, bottom - top);
        assert!(left >= 0 && top >= 0 && right <= 854 && bottom <= 480);
        let high = VideoConfig::from_settings("1080p", 30).unwrap();
        assert_eq!((high.width, high.height, high.fps), (1920, 1080, 30));
        assert_eq!(
            VideoConfig::from_settings("720p", 15).unwrap(),
            VideoConfig::default()
        );
        assert!(VideoConfig::from_settings("4k", 15).is_err());
        assert!(VideoConfig::from_settings("720p", 0).is_err());
    }
    #[test]
    fn letterbox_preserves_wide_portrait_and_square_sources_and_rejects_empty() {
        assert_eq!(fit_rect(1920, 1080).unwrap(), (0, 0, 1280, 720));
        assert_eq!(fit_rect(1024, 1024).unwrap(), (280, 0, 1000, 720));
        assert_eq!(fit_rect(720, 1280).unwrap(), (438, 0, 842, 720));
        assert!(fit_rect(0, 720).is_err());
    }
    #[test]
    fn timestamps_follow_wall_clock_instead_of_compressing_dropped_frames() {
        assert_eq!(next_frame_index(None, Duration::ZERO), 0);
        assert_eq!(next_frame_index(Some(14), Duration::from_secs(2)), 30);
        assert_eq!(next_frame_index(Some(30), Duration::from_millis(2001)), 31);
    }
    #[test]
    fn mp4_requires_finalized_movie_and_media_boxes() {
        let root = std::env::temp_dir().join(format!("video-container-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("test.mp4");
        let mut bytes = Vec::new();
        for kind in [b"ftyp", b"mdat", b"moov"] {
            bytes.extend_from_slice(&16_u32.to_be_bytes());
            bytes.extend_from_slice(kind);
            bytes.extend_from_slice(&[0; 8]);
        }
        std::fs::write(&path, &bytes).unwrap();
        assert!(valid_mp4(&path));
        std::fs::write(&path, &bytes[..32]).unwrap();
        assert!(!valid_mp4(&path));
        std::fs::write(&path, [0; 48]).unwrap();
        assert!(!valid_mp4(&path));
        std::fs::remove_dir_all(root).unwrap();
    }
}
