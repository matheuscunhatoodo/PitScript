use std::{io, path::PathBuf};

use super::StorageManager;

impl StorageManager {
    pub fn database_path(&self) -> PathBuf {
        self.root.join("database").join("meeting-recorder.db")
    }

    pub fn log_path(&self) -> PathBuf {
        self.root.join("logs").join("meeting-recorder.log")
    }

    pub fn get_microphone_path(&self, id: &str) -> io::Result<PathBuf> {
        Ok(self.meeting_directory(id)?.join("microphone.wav"))
    }

    pub fn get_system_audio_path(&self, id: &str) -> io::Result<PathBuf> {
        Ok(self.meeting_directory(id)?.join("system.wav"))
    }

    pub fn get_merged_audio_path(&self, id: &str) -> io::Result<PathBuf> {
        Ok(self.meeting_directory(id)?.join("merged.wav"))
    }

    pub fn get_video_path(&self, id: &str) -> io::Result<PathBuf> {
        Ok(self.meeting_directory(id)?.join("video.mp4"))
    }

    pub fn get_transcript_path(&self, id: &str) -> io::Result<PathBuf> {
        Ok(self.meeting_directory(id)?.join("transcript.txt"))
    }
}
