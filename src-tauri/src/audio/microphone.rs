use std::{
    ffi::c_void,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::SyncSender,
    },
    time::Instant,
};

use serde::Serialize;
use windows::{
    core::{PCWSTR, PWSTR},
    Win32::{
        Devices::FunctionDiscovery::PKEY_Device_FriendlyName,
        Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT},
        Media::Audio::{
            eCapture, eConsole, IAudioCaptureClient, IAudioClient, IMMDevice, IMMDeviceEnumerator,
            MMDeviceEnumerator, AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED,
            AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM, AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
            DEVICE_STATE_ACTIVE, WAVEFORMATEX, WAVE_FORMAT_PCM,
        },
        System::{
            Com::{
                CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize,
                StructuredStorage::{PropVariantClear, PropVariantToStringAlloc},
                CLSCTX_ALL, COINIT_MULTITHREADED, STGM_READ,
            },
            Threading::{CreateEventW, WaitForSingleObject},
        },
    },
};

use super::wav::{WavWriter, BITS_PER_SAMPLE, CHANNELS, SAMPLE_RATE};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InputDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

pub fn list_input_devices() -> Result<Vec<InputDevice>, String> {
    let _com = ComApartment::initialize()?;
    let enumerator = device_enumerator()?;
    let collection = unsafe { enumerator.EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE) }
        .map_err(|error| error.to_string())?;
    let default_id = unsafe { enumerator.GetDefaultAudioEndpoint(eCapture, eConsole) }
        .ok()
        .and_then(|device| device_id(&device).ok());
    let mut devices = Vec::new();
    let count = unsafe { collection.GetCount() }.map_err(|error| error.to_string())?;
    for index in 0..count {
        let device = unsafe { collection.Item(index) }.map_err(|error| error.to_string())?;
        let id = device_id(&device)?;
        let name = device_name(&device).unwrap_or_else(|_| id.clone());
        devices.push(InputDevice {
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
            .unwrap_or_else(|| "Capture did not start".to_owned())));
    }
    result
}

pub(crate) struct ComApartment;

impl ComApartment {
    pub(crate) fn initialize() -> Result<Self, String> {
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
            .ok()
            .map_err(|error| error.to_string())?;
        Ok(Self)
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

struct EventHandle(HANDLE);

impl Drop for EventHandle {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.0) };
    }
}

pub(crate) struct StartedClient(pub(crate) IAudioClient);

impl Drop for StartedClient {
    fn drop(&mut self) {
        let _ = unsafe { self.0.Stop() };
    }
}

pub(crate) fn device_enumerator() -> Result<IMMDeviceEnumerator, String> {
    unsafe {
        CoCreateInstance(
            &MMDeviceEnumerator,
            None::<&windows::core::IUnknown>,
            CLSCTX_ALL,
        )
    }
    .map_err(|error| error.to_string())
}

pub(crate) fn device_id(device: &IMMDevice) -> Result<String, String> {
    let id = unsafe { device.GetId() }.map_err(|error| error.to_string())?;
    let result = unsafe { id.to_string() }.map_err(|error| error.to_string());
    unsafe { CoTaskMemFree(Some(id.0.cast::<c_void>())) };
    result
}

pub(crate) fn device_name(device: &IMMDevice) -> Result<String, String> {
    let store =
        unsafe { device.OpenPropertyStore(STGM_READ) }.map_err(|error| error.to_string())?;
    let mut value =
        unsafe { store.GetValue(&PKEY_Device_FriendlyName) }.map_err(|error| error.to_string())?;
    let name = unsafe { PropVariantToStringAlloc(&value) }.map_err(|error| error.to_string());
    let result = name.and_then(|name: PWSTR| {
        let text = unsafe { name.to_string() }.map_err(|error| error.to_string());
        unsafe { CoTaskMemFree(Some(name.0.cast::<c_void>())) };
        text
    });
    let _ = unsafe { PropVariantClear(&mut value) };
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
        return Err("The selected microphone is not active".to_owned());
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
            AUDCLNT_STREAMFLAGS_EVENTCALLBACK | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
            2_000_000,
            0,
            &format,
            None,
        )
    }
    .map_err(|error| error.to_string())?;
    let event = EventHandle(
        unsafe { CreateEventW(None, false, false, None) }.map_err(|error| error.to_string())?,
    );
    unsafe { client.SetEventHandle(event.0) }.map_err(|error| error.to_string())?;
    let capture_client: IAudioCaptureClient =
        unsafe { client.GetService() }.map_err(|error| error.to_string())?;
    let mut wav = WavWriter::create(path).map_err(|error| error.to_string())?;
    unsafe { client.Start() }.map_err(|error| error.to_string())?;
    let active = StartedClient(client);
    if let Some(origin) = origin {
        // Align sample zero with the meeting/video clock without discarding captured audio.
        let frames = (origin.elapsed().as_nanos() * u128::from(SAMPLE_RATE) / 1_000_000_000)
            .min(u128::from(u32::MAX / 2)) as usize;
        write_silence(&mut wav, frames * 2).map_err(|error| error.to_string())?;
    }
    *started_sent = true;
    let _ = started.send(Ok(()));

    while !stop.load(Ordering::Acquire) {
        let wait = unsafe { WaitForSingleObject(event.0, 250) };
        if wait != WAIT_OBJECT_0 && wait != WAIT_TIMEOUT {
            return Err(std::io::Error::last_os_error().to_string());
        }
        loop {
            let packet_size =
                unsafe { capture_client.GetNextPacketSize() }.map_err(|error| error.to_string())?;
            if packet_size == 0 {
                break;
            }
            let mut data = std::ptr::null_mut();
            let mut frames = 0;
            let mut flags = 0;
            unsafe { capture_client.GetBuffer(&mut data, &mut frames, &mut flags, None, None) }
                .map_err(|error| error.to_string())?;
            let byte_count = usize::try_from(frames)
                .ok()
                .and_then(|frames| frames.checked_mul(2))
                .ok_or_else(|| "Capture buffer is too large".to_owned());
            let byte_count = match byte_count {
                Ok(count) => count,
                Err(error) => {
                    let _ = unsafe { capture_client.ReleaseBuffer(frames) };
                    return Err(error);
                }
            };
            let written = if flags & (AUDCLNT_BUFFERFLAGS_SILENT.0 as u32) != 0 {
                write_silence(&mut wav, byte_count)
            } else if data.is_null() {
                Err(std::io::Error::other("Capture returned a null buffer"))
            } else {
                let bytes = unsafe { std::slice::from_raw_parts(data, byte_count) };
                wav.write_samples(bytes)
            };
            let release = unsafe { capture_client.ReleaseBuffer(frames) };
            written.map_err(|error| error.to_string())?;
            release.map_err(|error| error.to_string())?;
        }
    }
    drop(active);
    wav.finish().map_err(|error| error.to_string())
}

pub(crate) fn write_silence(wav: &mut WavWriter, mut bytes: usize) -> std::io::Result<()> {
    const ZEROES: [u8; 4096] = [0; 4096];
    while bytes > 0 {
        let length = bytes.min(ZEROES.len());
        wav.write_samples(&ZEROES[..length])?;
        bytes -= length;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        sync::{atomic::AtomicBool, mpsc::sync_channel},
        thread,
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    use super::{capture, list_input_devices};

    #[test]
    #[ignore = "requires a Windows microphone and explicit permission to record"]
    fn manual_record_five_seconds() {
        let devices = list_input_devices().expect("enumerate microphones");
        assert!(!devices.is_empty(), "No active microphone is available");
        for device in &devices {
            println!(
                "Input: {} | id={} | default={}",
                device.name, device.id, device.is_default
            );
        }
        let selected_id = std::env::var("MEETING_RECORDER_MIC_DEVICE_ID").ok();
        let selected = selected_id
            .as_deref()
            .map(|id| {
                devices
                    .iter()
                    .find(|device| device.id == id)
                    .expect("selected microphone not found")
            })
            .unwrap_or_else(|| {
                devices
                    .iter()
                    .find(|device| device.is_default)
                    .unwrap_or(&devices[0])
            });
        let seconds = std::env::var("MEETING_RECORDER_MIC_TEST_SECONDS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(5);
        assert!(
            (1..=1800).contains(&seconds),
            "duration must be 1..=1800 seconds"
        );
        let root = std::env::temp_dir().join(format!(
            "meeting-recorder-manual-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        let path = root.join("microphone.wav");
        let stop = std::sync::Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let device_id = selected.id.clone();
        let (tx, rx) = sync_channel(1);
        let worker = thread::spawn(move || capture(&device_id, &path, &worker_stop, &tx));
        rx.recv_timeout(Duration::from_secs(10)).unwrap().unwrap();
        thread::sleep(Duration::from_secs(seconds));
        stop.store(true, std::sync::atomic::Ordering::Release);
        let bytes = worker.join().unwrap().unwrap();
        assert!(bytes > 0, "Microphone returned no samples");
        let data = fs::read(root.join("microphone.wav")).unwrap();
        assert_eq!(&data[0..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(data[40..44].try_into().unwrap()), bytes);
        println!(
            "Device: {} | WAV: {} | PCM bytes: {bytes}",
            selected.name,
            root.join("microphone.wav").display()
        );
    }
}
