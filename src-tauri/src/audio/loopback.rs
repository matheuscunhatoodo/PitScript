use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::SyncSender,
    },
    thread,
    time::{Duration, Instant},
};

use serde::Serialize;
use windows::{
    core::PCWSTR,
    Win32::{
        Media::Audio::{
            eMultimedia, eRender, IAudioCaptureClient, IAudioClient,
            AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY, AUDCLNT_BUFFERFLAGS_SILENT,
            AUDCLNT_BUFFERFLAGS_TIMESTAMP_ERROR, AUDCLNT_SHAREMODE_SHARED,
            AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM, AUDCLNT_STREAMFLAGS_LOOPBACK, DEVICE_STATE_ACTIVE,
            WAVEFORMATEX, WAVE_FORMAT_PCM,
        },
        System::Com::CLSCTX_ALL,
    },
};

use super::{
    microphone::{
        device_enumerator, device_id, device_name, write_silence, ComApartment, StartedClient,
    },
    wav::{WavWriter, BITS_PER_SAMPLE, CHANNELS, SAMPLE_RATE},
};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

pub fn list_output_devices() -> Result<Vec<OutputDevice>, String> {
    let _com = ComApartment::initialize()?;
    let enumerator = device_enumerator()?;
    let collection = unsafe { enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE) }
        .map_err(|error| error.to_string())?;
    let default_id = unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eMultimedia) }
        .ok()
        .and_then(|device| device_id(&device).ok());
    let count = unsafe { collection.GetCount() }.map_err(|error| error.to_string())?;
    let mut devices = Vec::new();
    for index in 0..count {
        let device = unsafe { collection.Item(index) }.map_err(|error| error.to_string())?;
        let id = device_id(&device)?;
        let name = device_name(&device).unwrap_or_else(|_| id.clone());
        devices.push(OutputDevice {
            is_default: default_id.as_deref() == Some(id.as_str()),
            id,
            name,
        });
    }
    Ok(devices)
}

pub fn capture(
    device_id: &str,
    path: &Path,
    stop: &AtomicBool,
    started: &SyncSender<Result<(), String>>,
) -> Result<u32, String> {
    capture_on_timeline(device_id, path, stop, started, None)
}

pub(crate) fn capture_on_timeline(
    device_id: &str,
    path: &Path,
    stop: &AtomicBool,
    started: &SyncSender<Result<(), String>>,
    origin: Option<Instant>,
) -> Result<u32, String> {
    let mut started_sent = false;
    let result = run_capture(device_id, path, stop, started, &mut started_sent, origin);
    if !started_sent {
        let _ = started.send(Err(result
            .as_ref()
            .err()
            .cloned()
            .unwrap_or_else(|| "Loopback did not start".to_owned())));
    }
    result
}

fn run_capture(
    device_id: &str,
    path: &Path,
    stop: &AtomicBool,
    started: &SyncSender<Result<(), String>>,
    started_sent: &mut bool,
    origin: Option<Instant>,
) -> Result<u32, String> {
    let _com = ComApartment::initialize()?;
    let enumerator = device_enumerator()?;
    let wide_id: Vec<u16> = device_id.encode_utf16().chain(std::iter::once(0)).collect();
    let device = unsafe { enumerator.GetDevice(PCWSTR::from_raw(wide_id.as_ptr())) }
        .map_err(|error| error.to_string())?;
    if unsafe { device.GetState() }.map_err(|error| error.to_string())? != DEVICE_STATE_ACTIVE {
        return Err("The selected output device is not active".to_owned());
    }
    let client: IAudioClient =
        unsafe { device.Activate(CLSCTX_ALL, None) }.map_err(|error| error.to_string())?;
    let format = WAVEFORMATEX {
        wFormatTag: WAVE_FORMAT_PCM as u16,
        nChannels: CHANNELS,
        nSamplesPerSec: SAMPLE_RATE,
        nAvgBytesPerSec: SAMPLE_RATE * u32::from(CHANNELS) * u32::from(BITS_PER_SAMPLE / 8),
        nBlockAlign: CHANNELS * BITS_PER_SAMPLE / 8,
        wBitsPerSample: BITS_PER_SAMPLE,
        cbSize: 0,
    };
    unsafe {
        client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            AUDCLNT_STREAMFLAGS_LOOPBACK | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
            2_000_000,
            0,
            &format,
            None,
        )
    }
    .map_err(|error| error.to_string())?;
    let capture_client: IAudioCaptureClient =
        unsafe { client.GetService() }.map_err(|error| error.to_string())?;
    let mut wav = WavWriter::create(path).map_err(|error| error.to_string())?;
    unsafe { client.Start() }.map_err(|error| error.to_string())?;
    let active = StartedClient(client);
    let start_time = origin.unwrap_or_else(Instant::now);
    *started_sent = true;
    let _ = started.send(Ok(()));

    let mut timeline = FrameTimeline::default();
    let mut first_packet_time: Option<(u64, u64)> = None;
    while !stop.load(Ordering::Acquire) {
        let mut received = false;
        loop {
            if stop.load(Ordering::Acquire) {
                break;
            }
            let packet_size =
                unsafe { capture_client.GetNextPacketSize() }.map_err(|error| error.to_string())?;
            if packet_size == 0 {
                break;
            }
            received = true;
            let mut data = std::ptr::null_mut();
            let mut frames = 0;
            let mut flags = 0;
            let mut qpc_time = 0;
            unsafe {
                capture_client.GetBuffer(
                    &mut data,
                    &mut frames,
                    &mut flags,
                    None,
                    Some(&mut qpc_time),
                )
            }
            .map_err(|error| error.to_string())?;
            let position = if flags & (AUDCLNT_BUFFERFLAGS_TIMESTAMP_ERROR.0 as u32) == 0 {
                let origin = first_packet_time.get_or_insert_with(|| {
                    let elapsed_frames =
                        (start_time.elapsed().as_secs_f64() * f64::from(SAMPLE_RATE)) as u64;
                    (qpc_time, elapsed_frames.saturating_sub(u64::from(frames)))
                });
                let delta = qpc_time.saturating_sub(origin.0);
                origin
                    .1
                    .saturating_add(delta.saturating_mul(u64::from(SAMPLE_RATE)) / 10_000_000)
            } else {
                timeline.frames_written
            };
            let result = write_packet(&mut wav, &mut timeline, data, frames, flags, position);
            let release = unsafe { capture_client.ReleaseBuffer(frames) };
            result?;
            release.map_err(|error| error.to_string())?;
        }
        if !received {
            thread::sleep(Duration::from_millis(10));
        }
    }
    let elapsed_frames = (start_time.elapsed().as_secs_f64() * f64::from(SAMPLE_RATE)) as u64;
    drop(active);
    timeline.pad_to(&mut wav, elapsed_frames)?;
    if std::env::var_os("MEETING_RECORDER_LOOPBACK_DIAGNOSTIC").is_some() {
        eprintln!(
            "loopback frames: packets={} payload={} silence={} total={} elapsed={}",
            timeline.packets_seen,
            timeline.payload_frames,
            timeline.silence_frames,
            timeline.frames_written,
            elapsed_frames
        );
    }
    let bytes = wav.finish().map_err(|error| error.to_string())?;
    if timeline.discontinuities > 0 {
        Err(format!(
            "Loopback reported {} audio discontinuities; valid WAV preserved ({bytes} bytes)",
            timeline.discontinuities
        ))
    } else {
        Ok(bytes)
    }
}

#[derive(Default)]
struct FrameTimeline {
    frames_written: u64,
    packets_seen: u64,
    discontinuities: u64,
    payload_frames: u64,
    silence_frames: u64,
}

impl FrameTimeline {
    fn pad_to(&mut self, wav: &mut WavWriter, position: u64) -> Result<(), String> {
        let mut missing = position.saturating_sub(self.frames_written);
        while missing > 0 {
            let frames = missing.min(2048);
            write_silence(wav, usize::try_from(frames).unwrap() * 2)
                .map_err(|error| error.to_string())?;
            self.frames_written += frames;
            self.silence_frames += frames;
            missing -= frames;
        }
        Ok(())
    }
}

fn write_packet(
    wav: &mut WavWriter,
    timeline: &mut FrameTimeline,
    data: *mut u8,
    frames: u32,
    flags: u32,
    position: u64,
) -> Result<(), String> {
    if timeline.frames_written >= u64::from(SAMPLE_RATE / 5)
        && flags & (AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY.0 as u32) != 0
    {
        if std::env::var_os("MEETING_RECORDER_LOOPBACK_DIAGNOSTIC").is_some() {
            eprintln!(
                "loopback discontinuity at packet={} frame={}",
                timeline.packets_seen, timeline.frames_written
            );
        }
        timeline.discontinuities += 1;
    }
    let valid_position = flags & (AUDCLNT_BUFFERFLAGS_TIMESTAMP_ERROR.0 as u32) == 0;
    if valid_position {
        timeline.pad_to(wav, position)?;
    }
    let overlap = if valid_position {
        timeline
            .frames_written
            .saturating_sub(position)
            .min(u64::from(frames))
    } else {
        0
    };
    let kept_frames = u64::from(frames) - overlap;
    let byte_count = usize::try_from(
        kept_frames
            .checked_mul(2)
            .ok_or("Capture buffer is too large")?,
    )
    .map_err(|error| error.to_string())?;
    if byte_count > 0 {
        if flags & (AUDCLNT_BUFFERFLAGS_SILENT.0 as u32) != 0 {
            write_silence(wav, byte_count).map_err(|error| error.to_string())?;
        } else if data.is_null() {
            return Err("Loopback returned a null buffer".to_owned());
        } else {
            let bytes = unsafe {
                std::slice::from_raw_parts(
                    data.add(usize::try_from(overlap).unwrap() * 2),
                    byte_count,
                )
            };
            wav.write_samples(bytes)
                .map_err(|error| error.to_string())?;
        }
        timeline.frames_written += kept_frames;
        timeline.payload_frames += kept_frames;
    }
    timeline.packets_seen += 1;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{capture, list_output_devices, write_packet, FrameTimeline};
    use crate::audio::microphone;
    use crate::audio::wav::WavWriter;
    use std::{
        fs,
        sync::{
            atomic::{AtomicBool, Ordering},
            mpsc::sync_channel,
            Arc,
        },
        thread,
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn fills_gaps_and_ignores_overlap_without_losing_packet_data() {
        let path = std::env::temp_dir().join(format!(
            "loopback-timeline-{}-{}.wav",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut wav = WavWriter::create(&path).unwrap();
        let mut timeline = FrameTimeline::default();
        let mut first = [1_u8, 0, 2, 0];
        write_packet(&mut wav, &mut timeline, first.as_mut_ptr(), 2, 0, 2).unwrap();
        let mut second = [3_u8, 0, 4, 0];
        write_packet(&mut wav, &mut timeline, second.as_mut_ptr(), 2, 0, 3).unwrap();
        assert_eq!(timeline.frames_written, 5);
        wav.finish().unwrap();
        let bytes = fs::read(&path).unwrap();
        assert_eq!(&bytes[44..], &[0, 0, 0, 0, 1, 0, 2, 0, 4, 0]);
        fs::remove_file(path).unwrap();
    }

    #[test]
    #[ignore = "requires active Windows devices and browser audio during capture"]
    fn manual_record_system_and_microphone() {
        let outputs = list_output_devices().expect("enumerate outputs");
        assert!(!outputs.is_empty(), "No active output device");
        for device in &outputs {
            println!(
                "Output: {} | id={} | default={}",
                device.name, device.id, device.is_default
            );
        }
        let inputs = microphone::list_input_devices().expect("enumerate microphones");
        for device in &inputs {
            println!(
                "Input: {} | id={} | default={}",
                device.name, device.id, device.is_default
            );
        }
        let output_id = std::env::var("MEETING_RECORDER_OUTPUT_DEVICE_ID").ok();
        let output = output_id
            .as_deref()
            .map(|id| {
                outputs
                    .iter()
                    .find(|device| device.id == id)
                    .expect("output device not found")
            })
            .unwrap_or_else(|| {
                outputs
                    .iter()
                    .find(|device| device.is_default)
                    .unwrap_or(&outputs[0])
            });
        let input_id = std::env::var("MEETING_RECORDER_MIC_DEVICE_ID").ok();
        let input = input_id
            .as_deref()
            .map(|id| {
                inputs
                    .iter()
                    .find(|device| device.id == id)
                    .expect("input device not found")
            })
            .or_else(|| {
                inputs
                    .iter()
                    .find(|device| device.is_default)
                    .or_else(|| inputs.first())
            });
        let seconds = std::env::var("MEETING_RECORDER_SYSTEM_TEST_SECONDS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(12);
        assert!((1..=1800).contains(&seconds));
        let root = std::env::temp_dir().join(format!(
            "meeting-recorder-loopback-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let mic_worker = input.map(|device| {
            let (tx, rx) = sync_channel(1);
            let id = device.id.clone();
            let path = root.join("microphone.wav");
            let mic_stop = stop.clone();
            let worker = thread::spawn(move || microphone::capture(&id, &path, &mic_stop, &tx));
            (worker, rx)
        });
        if let Some((_, mic_started)) = mic_worker.as_ref() {
            mic_started
                .recv_timeout(Duration::from_secs(10))
                .unwrap()
                .unwrap();
        }
        let (tx, rx) = sync_channel(1);
        let output_id = output.id.clone();
        let system_path = root.join("system.wav");
        let system_stop = stop.clone();
        let system_worker =
            thread::spawn(move || capture(&output_id, &system_path, &system_stop, &tx));
        rx.recv_timeout(Duration::from_secs(10)).unwrap().unwrap();
        println!(
            "PLAY_BROWSER_AUDIO_NOW seconds={seconds} output={} mic={}",
            output.name,
            input.map(|device| device.name.as_str()).unwrap_or("none")
        );
        thread::sleep(Duration::from_secs(seconds));
        stop.store(true, Ordering::Release);
        let system_result = system_worker.join().unwrap();
        let system = fs::read(root.join("system.wav")).unwrap();
        let system_bytes = u32::from_le_bytes(system[40..44].try_into().unwrap());
        assert_eq!(&system[..4], b"RIFF");
        assert_eq!(system.len(), 44 + system_bytes as usize);
        let system_seconds = f64::from(system_bytes) / 96_000.0;
        let samples = system[44..].as_chunks::<2>().0;
        let nonzero = samples.iter().filter(|sample| **sample != [0, 0]).count();
        let (longest_silence, _) =
            samples
                .iter()
                .skip(24_000)
                .fold((0_usize, 0_usize), |(longest, current), sample| {
                    let current = if *sample == [0, 0] { current + 1 } else { 0 };
                    (longest.max(current), current)
                });
        println!(
            "system={} bytes, {:.3} s, nonzero_frames={}, longest_silence_after_start={:.1} ms, result={:?}, path={}",
            system_bytes,
            system_seconds,
            nonzero,
            longest_silence as f64 / 48.0,
            system_result,
            root.join("system.wav").display()
        );
        if let Some((worker, _)) = mic_worker {
            let mic_bytes = worker.join().unwrap().unwrap();
            let mic_seconds = f64::from(mic_bytes) / 96_000.0;
            println!(
                "microphone={} bytes, {:.3} s, delta={:.3} s, path={}",
                mic_bytes,
                mic_seconds,
                (system_seconds - mic_seconds).abs(),
                root.join("microphone.wav").display()
            );
            assert!(
                (system_seconds - mic_seconds).abs() < 0.5,
                "WAV durations differ by at least 0.5 s"
            );
        }
        assert!(
            nonzero > 0,
            "No browser audio reached the selected output device"
        );
        assert!(
            longest_silence < 4_800,
            "At least 100 ms of unexpected silence during browser tone"
        );
        assert!(system_result.is_ok(), "Loopback reported a discontinuity");
    }
}
