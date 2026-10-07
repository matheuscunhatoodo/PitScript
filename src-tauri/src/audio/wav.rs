use std::{
    fs::{File, OpenOptions},
    io::{self, ErrorKind, Seek, SeekFrom, Write},
    path::Path,
};

pub const SAMPLE_RATE: u32 = 48_000;
pub const CHANNELS: u16 = 1;
pub const BITS_PER_SAMPLE: u16 = 16;

pub struct WavWriter {
    file: File,
    sample_rate: u32,
    bytes_written: u32,
    bytes_since_checkpoint: u32,
    finished: bool,
}

impl WavWriter {
    pub fn create(path: &Path) -> io::Result<Self> {
        Self::create_with_sample_rate(path, SAMPLE_RATE)
    }

    pub fn create_with_sample_rate(path: &Path, sample_rate: u32) -> io::Result<Self> {
        if sample_rate == 0 || sample_rate > u32::MAX / 2 {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "Invalid sample rate",
            ));
        }
        let file = OpenOptions::new().write(true).create_new(true).open(path)?;
        let mut writer = Self {
            file,
            sample_rate,
            bytes_written: 0,
            bytes_since_checkpoint: 0,
            finished: false,
        };
        writer.checkpoint()?;
        Ok(writer)
    }

    pub fn write_samples(&mut self, samples: &[u8]) -> io::Result<()> {
        if !samples.len().is_multiple_of(2) {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "Incomplete PCM sample",
            ));
        }
        let count = u32::try_from(samples.len())
            .map_err(|_| io::Error::new(ErrorKind::InvalidInput, "WAV data exceeds RIFF limit"))?;
        if self
            .bytes_written
            .checked_add(count)
            .is_none_or(|size| size > u32::MAX - 36)
        {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "WAV data exceeds RIFF limit",
            ));
        }
        self.file.write_all(samples)?;
        self.bytes_written += count;
        self.bytes_since_checkpoint = self.bytes_since_checkpoint.saturating_add(count);
        if self.bytes_since_checkpoint
            >= self.sample_rate * u32::from(CHANNELS) * u32::from(BITS_PER_SAMPLE / 8)
        {
            self.checkpoint()?;
        }
        Ok(())
    }

    pub fn finish(mut self) -> io::Result<u32> {
        self.checkpoint()?;
        self.file.sync_all()?;
        self.finished = true;
        Ok(self.bytes_written)
    }

    fn checkpoint(&mut self) -> io::Result<()> {
        let mut header = [0_u8; 44];
        header[0..4].copy_from_slice(b"RIFF");
        header[4..8].copy_from_slice(&(36 + self.bytes_written).to_le_bytes());
        header[8..16].copy_from_slice(b"WAVEfmt ");
        header[16..20].copy_from_slice(&16_u32.to_le_bytes());
        header[20..22].copy_from_slice(&1_u16.to_le_bytes());
        header[22..24].copy_from_slice(&CHANNELS.to_le_bytes());
        header[24..28].copy_from_slice(&self.sample_rate.to_le_bytes());
        let byte_rate = self.sample_rate * u32::from(CHANNELS) * u32::from(BITS_PER_SAMPLE / 8);
        header[28..32].copy_from_slice(&byte_rate.to_le_bytes());
        header[32..34].copy_from_slice(&(CHANNELS * BITS_PER_SAMPLE / 8).to_le_bytes());
        header[34..36].copy_from_slice(&BITS_PER_SAMPLE.to_le_bytes());
        header[36..40].copy_from_slice(b"data");
        header[40..44].copy_from_slice(&self.bytes_written.to_le_bytes());
        self.file.seek(SeekFrom::Start(0))?;
        self.file.write_all(&header)?;
        self.file.flush()?;
        self.file.seek(SeekFrom::End(0))?;
        self.bytes_since_checkpoint = 0;
        Ok(())
    }
}

impl Drop for WavWriter {
    fn drop(&mut self) {
        if !self.finished {
            let _ = self.checkpoint();
            let _ = self.file.sync_all();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::WavWriter;

    fn path() -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "meeting-recorder-wav-{}-{nanos}.wav",
            std::process::id()
        ))
    }

    #[test]
    fn writes_valid_pcm_header_and_samples() {
        let path = path();
        let mut wav = WavWriter::create(&path).unwrap();
        wav.write_samples(&[0, 0, 0xff, 0x7f]).unwrap();
        assert_eq!(wav.finish().unwrap(), 4);
        let data = fs::read(&path).unwrap();
        assert_eq!(&data[0..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(data[4..8].try_into().unwrap()), 40);
        assert_eq!(&data[8..16], b"WAVEfmt ");
        assert_eq!(u16::from_le_bytes(data[20..22].try_into().unwrap()), 1);
        assert_eq!(u16::from_le_bytes(data[22..24].try_into().unwrap()), 1);
        assert_eq!(u32::from_le_bytes(data[24..28].try_into().unwrap()), 48_000);
        assert_eq!(u16::from_le_bytes(data[34..36].try_into().unwrap()), 16);
        assert_eq!(&data[36..40], b"data");
        assert_eq!(u32::from_le_bytes(data[40..44].try_into().unwrap()), 4);
        assert_eq!(&data[44..], &[0, 0, 0xff, 0x7f]);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn writes_a_16khz_pcm_file_for_prepared_audio() {
        let path = path();
        let mut wav = WavWriter::create_with_sample_rate(&path, 16_000).unwrap();
        wav.write_samples(&[0, 0, 1, 0]).unwrap();
        wav.finish().unwrap();
        let data = fs::read(&path).unwrap();
        assert_eq!(u32::from_le_bytes(data[24..28].try_into().unwrap()), 16_000);
        assert_eq!(u32::from_le_bytes(data[28..32].try_into().unwrap()), 32_000);
        assert_eq!(u32::from_le_bytes(data[40..44].try_into().unwrap()), 4);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn dropping_writer_keeps_a_playable_partial_wav() {
        let path = path();
        {
            let mut wav = WavWriter::create(&path).unwrap();
            wav.write_samples(&[1, 0, 2, 0]).unwrap();
        }
        let data = fs::read(&path).unwrap();
        assert_eq!(u32::from_le_bytes(data[4..8].try_into().unwrap()), 40);
        assert_eq!(u32::from_le_bytes(data[40..44].try_into().unwrap()), 4);
        assert_eq!(&data[44..], &[1, 0, 2, 0]);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn existing_recording_is_never_truncated() {
        let path = path();
        fs::write(&path, b"existing").unwrap();
        assert!(WavWriter::create(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"existing");
        fs::remove_file(path).unwrap();
    }
}
