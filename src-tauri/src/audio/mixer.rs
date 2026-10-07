use std::{
    collections::VecDeque,
    fs::{self, File},
    io::{BufReader, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use super::wav::{WavWriter, SAMPLE_RATE};

const OUTPUT_RATE: u32 = 16_000;
const FILTER_HALF_WIDTH: i64 = 15;
const FILTER_TAPS: usize = 31;

struct InputSpec {
    path: PathBuf,
    frames: u64,
    created: Option<SystemTime>,
    offset: u64,
}

struct InputReader {
    file: BufReader<File>,
    remaining: u64,
    offset: u64,
}

impl InputReader {
    fn open(spec: &InputSpec) -> Result<Self, String> {
        let mut file = BufReader::new(File::open(&spec.path).map_err(|error| error.to_string())?);
        file.seek(SeekFrom::Start(44))
            .map_err(|error| error.to_string())?;
        Ok(Self {
            file,
            remaining: spec.frames,
            offset: spec.offset,
        })
    }

    fn sample_at(&mut self, frame: u64) -> Result<f64, String> {
        if frame < self.offset || self.remaining == 0 {
            return Ok(0.0);
        }
        let mut bytes = [0_u8; 2];
        self.file
            .read_exact(&mut bytes)
            .map_err(|error| error.to_string())?;
        self.remaining -= 1;
        Ok(f64::from(i16::from_le_bytes(bytes)))
    }
}

pub struct MergeReport {
    pub bytes_written: u32,
    pub warnings: Vec<String>,
}

pub fn prepare_audio(
    microphone: &Path,
    system: &Path,
    output: &Path,
) -> Result<MergeReport, String> {
    prepare_audio_cancellable(microphone, system, output, &|| false)
}

pub(crate) fn prepare_audio_cancellable(
    microphone: &Path,
    system: &Path,
    output: &Path,
    cancel: &dyn Fn() -> bool,
) -> Result<MergeReport, String> {
    if cancel() {
        return Err("Audio preparation cancelled".into());
    }
    let mut warnings = Vec::new();
    let mut inputs = Vec::new();
    for (label, path) in [("microphone", microphone), ("system", system)] {
        match inspect_input(path) {
            Ok(Some(input)) => inputs.push(input),
            Ok(None) => {}
            Err(error) => warnings.push(format!("{label}.wav skipped: {error}")),
        }
    }
    if inputs.is_empty() {
        return Err(format!(
            "No valid audio source is available. {}",
            warnings.join("; ")
        ));
    }
    let earliest = inputs.iter().filter_map(|input| input.created).min();
    if inputs.len() == 2 {
        if let Some(earliest) = earliest {
            for input in &mut inputs {
                if let Some(created) = input.created {
                    let elapsed = created.duration_since(earliest).unwrap_or_default();
                    if elapsed.as_secs() > 15 {
                        warnings.push("Source timestamps differ by more than 15 seconds; aligning their first samples".to_owned());
                    } else {
                        input.offset = u64::try_from(
                            elapsed.as_nanos() * u128::from(SAMPLE_RATE) / 1_000_000_000,
                        )
                        .map_err(|error| error.to_string())?;
                    }
                }
            }
        }
    }
    let frames = inputs
        .iter()
        .map(|input| input.frames + input.offset)
        .max()
        .unwrap_or(0);
    let output_frames = frames.div_ceil(3);
    if output_frames > u64::from((u32::MAX - 36) / 2) {
        return Err("Prepared WAV exceeds the RIFF size limit".to_owned());
    }
    if output.exists() {
        let existing = inspect_output(output)?;
        return Ok(MergeReport {
            bytes_written: existing,
            warnings,
        });
    }

    let coefficients = low_pass_coefficients();
    let mut peak = 0.0_f64;
    render(&inputs, frames, &coefficients, cancel, |sample| {
        peak = peak.max(sample.abs());
        Ok(())
    })?;
    let gain = if peak > f64::from(i16::MAX) {
        f64::from(i16::MAX) / peak
    } else {
        1.0
    };
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let temporary = output.with_extension(format!("wav.part-{}-{unique}", std::process::id()));
    let result = (|| -> Result<u32, String> {
        let mut writer = WavWriter::create_with_sample_rate(&temporary, OUTPUT_RATE)
            .map_err(|error| error.to_string())?;
        let mut buffer = Vec::with_capacity(8192);
        render(&inputs, frames, &coefficients, cancel, |sample| {
            let scaled = (sample * gain).round().clamp(-32767.0, 32767.0) as i16;
            buffer.extend_from_slice(&scaled.to_le_bytes());
            if buffer.len() >= 8192 {
                writer
                    .write_samples(&buffer)
                    .map_err(|error| error.to_string())?;
                buffer.clear();
            }
            Ok(())
        })?;
        if !buffer.is_empty() {
            writer
                .write_samples(&buffer)
                .map_err(|error| error.to_string())?;
        }
        let bytes = writer.finish().map_err(|error| error.to_string())?;
        if cancel() {
            return Err("Audio preparation cancelled".into());
        }
        fs::rename(&temporary, output).map_err(|error| error.to_string())?;
        Ok(bytes)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map(|bytes_written| MergeReport {
        bytes_written,
        warnings,
    })
}

fn inspect_input(path: &Path) -> Result<Option<InputSpec>, String> {
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    let mut header = [0_u8; 44];
    file.read_exact(&mut header)
        .map_err(|error| error.to_string())?;
    if &header[0..4] != b"RIFF"
        || &header[8..16] != b"WAVEfmt "
        || &header[36..40] != b"data"
        || u32::from_le_bytes(header[16..20].try_into().unwrap()) != 16
        || u16::from_le_bytes(header[20..22].try_into().unwrap()) != 1
        || u16::from_le_bytes(header[22..24].try_into().unwrap()) != 1
        || u32::from_le_bytes(header[24..28].try_into().unwrap()) != SAMPLE_RATE
        || u32::from_le_bytes(header[28..32].try_into().unwrap()) != SAMPLE_RATE * 2
        || u16::from_le_bytes(header[32..34].try_into().unwrap()) != 2
        || u16::from_le_bytes(header[34..36].try_into().unwrap()) != 16
    {
        return Err("unsupported WAV format; expected 48 kHz mono PCM 16-bit".to_owned());
    }
    let data_bytes = u32::from_le_bytes(header[40..44].try_into().unwrap());
    if data_bytes == 0
        || !data_bytes.is_multiple_of(2)
        || metadata.len() < 44 + u64::from(data_bytes)
    {
        return Err("empty or truncated WAV data".to_owned());
    }
    let riff_bytes = u32::from_le_bytes(header[4..8].try_into().unwrap());
    if riff_bytes < 36 + data_bytes {
        return Err("invalid RIFF length".to_owned());
    }
    Ok(Some(InputSpec {
        path: path.to_path_buf(),
        frames: u64::from(data_bytes / 2),
        created: metadata.created().ok(),
        offset: 0,
    }))
}

pub(crate) fn valid_source_bytes(path: &Path) -> u32 {
    inspect_input(path)
        .ok()
        .flatten()
        .and_then(|input| u32::try_from(input.frames * 2).ok())
        .unwrap_or(0)
}

pub(crate) fn source_offsets_ms(microphone: Option<&Path>, system: Option<&Path>) -> (i64, i64) {
    let created =
        |path: Option<&Path>| path.and_then(|path| fs::metadata(path).ok()?.created().ok());
    match (created(microphone), created(system)) {
        (Some(local), Some(remote)) => {
            let origin = local.min(remote);
            let local = local.duration_since(origin).unwrap_or_default();
            let remote = remote.duration_since(origin).unwrap_or_default();
            if local.as_secs() > 15 || remote.as_secs() > 15 {
                (0, 0)
            } else {
                (local.as_millis() as i64, remote.as_millis() as i64)
            }
        }
        _ => (0, 0),
    }
}

pub(crate) fn inspect_output(path: &Path) -> Result<u32, String> {
    let mut file = File::open(path).map_err(|error| error.to_string())?;
    let mut header = [0_u8; 44];
    file.read_exact(&mut header)
        .map_err(|error| error.to_string())?;
    let bytes = u32::from_le_bytes(header[40..44].try_into().unwrap());
    if &header[0..4] != b"RIFF"
        || &header[8..16] != b"WAVEfmt "
        || &header[36..40] != b"data"
        || u32::from_le_bytes(header[16..20].try_into().unwrap()) != 16
        || u16::from_le_bytes(header[20..22].try_into().unwrap()) != 1
        || u16::from_le_bytes(header[22..24].try_into().unwrap()) != 1
        || u32::from_le_bytes(header[24..28].try_into().unwrap()) != OUTPUT_RATE
        || u32::from_le_bytes(header[28..32].try_into().unwrap()) != OUTPUT_RATE * 2
        || u16::from_le_bytes(header[32..34].try_into().unwrap()) != 2
        || u16::from_le_bytes(header[34..36].try_into().unwrap()) != 16
        || bytes == 0
        || !bytes.is_multiple_of(2)
        || u64::from(u32::from_le_bytes(header[4..8].try_into().unwrap())) < 36 + u64::from(bytes)
        || file.metadata().map_err(|error| error.to_string())?.len() < 44 + u64::from(bytes)
    {
        return Err("Existing merged.wav is invalid; it was preserved".to_owned());
    }
    Ok(bytes)
}

fn low_pass_coefficients() -> [f64; FILTER_TAPS] {
    let mut coefficients = [0.0; FILTER_TAPS];
    let cutoff = 7_200.0 / f64::from(SAMPLE_RATE);
    for (index, coefficient) in coefficients.iter_mut().enumerate() {
        let distance = index as i64 - FILTER_HALF_WIDTH;
        let sinc = if distance == 0 {
            2.0 * cutoff
        } else {
            (2.0 * std::f64::consts::PI * cutoff * distance as f64).sin()
                / (std::f64::consts::PI * distance as f64)
        };
        let window = 0.54 - 0.46 * (2.0 * std::f64::consts::PI * index as f64 / 30.0).cos();
        *coefficient = sinc * window;
    }
    let total: f64 = coefficients.iter().sum();
    for coefficient in &mut coefficients {
        *coefficient /= total;
    }
    coefficients
}

fn render(
    specs: &[InputSpec],
    frames: u64,
    coefficients: &[f64; FILTER_TAPS],
    cancel: &dyn Fn() -> bool,
    mut emit: impl FnMut(f64) -> Result<(), String>,
) -> Result<(), String> {
    let mut readers = specs
        .iter()
        .map(InputReader::open)
        .collect::<Result<Vec<_>, _>>()?;
    let mut buffer = VecDeque::with_capacity(FILTER_TAPS + 3);
    let mut first_buffered = 0_u64;
    let mut next_input = 0_u64;
    for output_frame in 0..frames.div_ceil(3) {
        if output_frame.is_multiple_of(1024) && cancel() {
            return Err("Audio preparation cancelled".into());
        }
        let center = output_frame * 3;
        let target = (center + FILTER_HALF_WIDTH as u64).min(frames - 1);
        while next_input <= target {
            let mut mixed = 0.0;
            for reader in &mut readers {
                mixed += reader.sample_at(next_input)?;
            }
            buffer.push_back(mixed);
            next_input += 1;
        }
        let mut sample = 0.0;
        for (tap, coefficient) in coefficients.iter().enumerate() {
            let position = center as i64 + tap as i64 - FILTER_HALF_WIDTH;
            if position >= 0 && (position as u64) < frames {
                sample += buffer[(position as u64 - first_buffered) as usize] * coefficient;
            }
        }
        emit(sample)?;
        let next_start = center
            .saturating_add(3)
            .saturating_sub(FILTER_HALF_WIDTH as u64);
        while first_buffered < next_start && !buffer.is_empty() {
            buffer.pop_front();
            first_buffered += 1;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
        thread,
        time::Duration,
        time::{SystemTime, UNIX_EPOCH},
    };

    use crate::audio::wav::WavWriter;

    use super::prepare_audio;

    fn root() -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "meeting-recorder-merge-{}-{nanos}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn source(path: &Path, samples: &[i16]) {
        let mut wav = WavWriter::create(path).unwrap();
        let bytes: Vec<u8> = samples
            .iter()
            .flat_map(|sample| sample.to_le_bytes())
            .collect();
        wav.write_samples(&bytes).unwrap();
        wav.finish().unwrap();
    }

    fn output_samples(path: &PathBuf) -> Vec<i16> {
        let bytes = fs::read(path).unwrap();
        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(&bytes[8..16], b"WAVEfmt ");
        assert_eq!(u16::from_le_bytes(bytes[20..22].try_into().unwrap()), 1);
        assert_eq!(u16::from_le_bytes(bytes[22..24].try_into().unwrap()), 1);
        assert_eq!(
            u32::from_le_bytes(bytes[24..28].try_into().unwrap()),
            16_000
        );
        assert_eq!(
            u32::from_le_bytes(bytes[28..32].try_into().unwrap()),
            32_000
        );
        assert_eq!(u16::from_le_bytes(bytes[34..36].try_into().unwrap()), 16);
        assert_eq!(
            u32::from_le_bytes(bytes[40..44].try_into().unwrap()) as usize,
            bytes.len() - 44
        );
        bytes[44..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|bytes| i16::from_le_bytes(*bytes))
            .collect()
    }

    fn tone(frequency: f64, frames: usize) -> Vec<i16> {
        (0..frames)
            .map(|frame| {
                (8_000.0 * (2.0 * std::f64::consts::PI * frequency * frame as f64 / 48_000.0).sin())
                    .round() as i16
            })
            .collect()
    }

    fn tone_level(samples: &[i16], frequency: f64) -> f64 {
        let (real, imaginary) =
            samples
                .iter()
                .enumerate()
                .fold((0.0, 0.0), |sum, (index, sample)| {
                    let phase = 2.0 * std::f64::consts::PI * frequency * index as f64 / 16_000.0;
                    (
                        sum.0 + f64::from(*sample) * phase.cos(),
                        sum.1 + f64::from(*sample) * phase.sin(),
                    )
                });
        real.hypot(imaginary) * 2.0 / samples.len() as f64
    }

    #[test]
    fn converts_one_source_to_16khz_without_shortening_it() {
        let root = root();
        fs::create_dir(&root).unwrap();
        let mic = root.join("microphone.wav");
        let system = root.join("system.wav");
        let merged = root.join("merged.wav");
        source(&mic, &vec![12_000; 48_000]);
        let report = prepare_audio(&mic, &system, &merged).unwrap();
        let samples = output_samples(&merged);
        assert_eq!(samples.len(), 16_000);
        assert!((samples[8_000] - 12_000).abs() <= 1);
        assert_eq!(report.bytes_written, 32_000);
        assert_eq!(fs::read(&mic).unwrap().len(), 96_044);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn combines_both_sources_without_clipping_or_truncating_the_longer_one() {
        let root = root();
        fs::create_dir(&root).unwrap();
        let mic = root.join("microphone.wav");
        let system = root.join("system.wav");
        let merged = root.join("merged.wav");
        source(&mic, &vec![24_000; 48_000]);
        source(&system, &vec![24_000; 24_000]);
        prepare_audio(&mic, &system, &merged).unwrap();
        let samples = output_samples(&merged);
        assert_eq!(samples.len(), 16_000);
        assert!(samples[4_000] > 30_000);
        assert!(samples[12_000] > 10_000);
        assert_eq!(
            samples.iter().map(|sample| i32::from(*sample).abs()).max(),
            Some(32_767)
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ignores_a_damaged_source_and_keeps_the_valid_one() {
        let root = root();
        fs::create_dir(&root).unwrap();
        let mic = root.join("microphone.wav");
        let system = root.join("system.wav");
        let merged = root.join("merged.wav");
        fs::write(&mic, b"broken audio").unwrap();
        source(&system, &vec![8_000; 4_800]);
        let report = prepare_audio(&mic, &system, &merged).unwrap();
        assert!(!report.warnings.is_empty());
        assert_eq!(output_samples(&merged).len(), 1_600);
        assert_eq!(fs::read(&mic).unwrap(), b"broken audio");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn merged_audio_contains_both_tones_and_reduces_aliasing() {
        let root = root();
        fs::create_dir(&root).unwrap();
        let mic = root.join("microphone.wav");
        let system = root.join("system.wav");
        let merged = root.join("merged.wav");
        source(&mic, &tone(440.0, 48_000));
        source(&system, &tone(660.0, 48_000));
        prepare_audio(&mic, &system, &merged).unwrap();
        let samples = output_samples(&merged);
        assert!(tone_level(&samples[1_000..15_000], 440.0) > 5_000.0);
        assert!(tone_level(&samples[1_000..15_000], 660.0) > 5_000.0);
        fs::remove_dir_all(&root).unwrap();

        let root = self::root();
        fs::create_dir(&root).unwrap();
        let mic = root.join("microphone.wav");
        let merged = root.join("merged.wav");
        source(&mic, &tone(12_000.0, 48_000));
        prepare_audio(&mic, &root.join("system.wav"), &merged).unwrap();
        let samples = output_samples(&merged);
        assert!(tone_level(&samples[1_000..15_000], 4_000.0) < 500.0);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn existing_merged_file_is_preserved_on_retry() {
        let root = root();
        fs::create_dir(&root).unwrap();
        let mic = root.join("microphone.wav");
        let system = root.join("system.wav");
        let merged = root.join("merged.wav");
        source(&mic, &vec![1_000; 4_800]);
        prepare_audio(&mic, &system, &merged).unwrap();
        let first = fs::read(&merged).unwrap();
        prepare_audio(&mic, &system, &merged).unwrap();
        assert_eq!(fs::read(&merged).unwrap(), first);
        assert_eq!(fs::read(&mic).unwrap().len(), 9_644);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn no_valid_source_does_not_create_merged_audio() {
        let root = root();
        fs::create_dir(&root).unwrap();
        let mic = root.join("microphone.wav");
        let system = root.join("system.wav");
        let merged = root.join("merged.wav");
        fs::write(&mic, b"damaged").unwrap();
        assert!(prepare_audio(&mic, &system, &merged).is_err());
        assert!(!merged.exists());
        assert_eq!(fs::read(&mic).unwrap(), b"damaged");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn later_source_is_aligned_after_its_file_creation_time() {
        let root = root();
        fs::create_dir(&root).unwrap();
        let mic = root.join("microphone.wav");
        let system = root.join("system.wav");
        let merged = root.join("merged.wav");
        source(&mic, &vec![12_000; 48_000]);
        thread::sleep(Duration::from_millis(120));
        source(&system, &vec![12_000; 48_000]);
        prepare_audio(&mic, &system, &merged).unwrap();
        let samples = output_samples(&merged);
        assert!(samples.len() > 17_000);
        assert!(samples[400] < samples[8_000]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cancellation_in_both_conversion_passes_preserves_source_and_removes_partial_output() {
        let root = root();
        fs::create_dir(&root).unwrap();
        let mic = root.join("microphone.wav");
        source(&mic, &vec![1000; 48_000]);
        let original = fs::read(&mic).unwrap();
        for cancel_at in [5, 22] {
            let calls = std::cell::Cell::new(0);
            let output = root.join(format!("result-{cancel_at}.wav"));
            let result =
                super::prepare_audio_cancellable(&mic, &root.join("absent.wav"), &output, &|| {
                    calls.set(calls.get() + 1);
                    calls.get() >= cancel_at
                });
            assert!(
                result.is_err(),
                "conversion ignored cancellation at check {cancel_at}"
            );
            assert!(!output.exists());
            assert_eq!(fs::read(&mic).unwrap(), original);
            assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[ignore = "creates a synthetic file for manual playback inspection"]
    fn manual_playback_fixture() {
        let root = root();
        fs::create_dir(&root).unwrap();
        let mic = root.join("microphone.wav");
        let system = root.join("system.wav");
        let merged = root.join("merged.wav");
        let mut microphone = tone(440.0, 144_000);
        microphone[96_000..].fill(0);
        let mut loopback = tone(660.0, 144_000);
        loopback[..48_000].fill(0);
        source(&mic, &microphone);
        source(&system, &loopback);
        prepare_audio(&mic, &system, &merged).unwrap();
        assert!((48_000..64_000).contains(&output_samples(&merged).len()));
        println!("manual merged WAV: {}", merged.display());
    }
}
