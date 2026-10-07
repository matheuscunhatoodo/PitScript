//! Explicit local captures only: opt in using --ignored. Never capture the desktop in ordinary tests.
use super::*;
use crate::audio::{loopback, microphone};
use std::fs;
use windows::{
    core::PCWSTR,
    Win32::{
        Foundation::FILETIME,
        Media::MediaFoundation::*,
        System::{
            ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS},
            Threading::{GetCurrentProcess, GetProcessTimes},
        },
    },
};

pub(crate) fn decode_video(path: &Path) -> Result<(u64, i64), String> {
    let _runtime = encoder::Runtime::initialize()?;
    unsafe {
        let url: Vec<u16> = path
            .to_string_lossy()
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let reader =
            MFCreateSourceReaderFromURL(PCWSTR(url.as_ptr()), None).map_err(|e| e.to_string())?;
        let stream = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
        let native = reader
            .GetNativeMediaType(stream, 0)
            .map_err(|e| e.to_string())?;
        assert_eq!(native.GetGUID(&MF_MT_SUBTYPE).unwrap(), MFVideoFormat_H264);
        let dimensions = native.GetUINT64(&MF_MT_FRAME_SIZE).unwrap();
        let (width, height) = ((dimensions >> 32) as u32, dimensions as u32);
        assert!([(854, 480), (1280, 720), (1920, 1080)].contains(&(width, height)));
        let rate = native.GetUINT64(&MF_MT_FRAME_RATE).unwrap();
        let (numerator, denominator) = ((rate >> 32) as u32, rate as u32);
        // MP4 reports the average rate, including the initial held frame and skipped frames.
        assert!(numerator > 0 && denominator > 0);
        let fps = f64::from(numerator) / f64::from(denominator);
        assert!(fps <= 30.1);
        println!("DECODE_PROFILE width={width} height={height} average_fps={fps:.3} rate={numerator}/{denominator}");
        let format = MFCreateMediaType().unwrap();
        format
            .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
            .unwrap();
        format.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12).unwrap();
        reader
            .SetCurrentMediaType(stream, None, &format)
            .map_err(|e| e.to_string())?;
        let mut frames = 0;
        let mut end = 0;
        let mut previous = -1;
        loop {
            let mut flags = 0;
            let mut sample = None;
            let mut pts = 0;
            reader
                .ReadSample(
                    stream,
                    0,
                    None,
                    Some(&mut flags),
                    Some(&mut pts),
                    Some(&mut sample),
                )
                .map_err(|e| e.to_string())?;
            if let Some(sample) = sample {
                assert!(pts >= previous, "timestamps must be monotonic");
                previous = pts;
                let buffer = sample
                    .ConvertToContiguousBuffer()
                    .map_err(|e| e.to_string())?;
                assert!(
                    buffer.GetCurrentLength().unwrap() > 0,
                    "decoded frame must have pixels"
                );
                frames += 1;
                end = pts
                    + sample
                        .GetSampleDuration()
                        .unwrap_or((10_000_000.0 / fps) as i64);
            }
            if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                break;
            }
        }
        assert!(frames > 0);
        Ok((frames, end / 10_000))
    }
}
fn process_sample() -> (f64, u64) {
    unsafe {
        let mut created = FILETIME::default();
        let mut exited = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        GetProcessTimes(
            GetCurrentProcess(),
            &mut created,
            &mut exited,
            &mut kernel,
            &mut user,
        )
        .unwrap();
        let ticks =
            |time: FILETIME| ((time.dwHighDateTime as u64) << 32) | time.dwLowDateTime as u64;
        let mut memory = PROCESS_MEMORY_COUNTERS::default();
        memory.cb = std::mem::size_of_val(&memory) as u32;
        GetProcessMemoryInfo(
            GetCurrentProcess(),
            &mut memory,
            std::mem::size_of_val(&memory) as u32,
        )
        .unwrap();
        (
            (ticks(kernel) + ticks(user)) as f64 / 10_000_000.0,
            memory.WorkingSetSize as u64,
        )
    }
}

#[test]
#[ignore = "records selected browser test window/monitor and real microphone+loopback; explicit opt-in required"]
fn native_capture_and_decode_with_simultaneous_audio() {
    let kind =
        std::env::var("MEETING_RECORDER_VIDEO_TEST_SOURCE").unwrap_or_else(|_| "monitor".into());
    let seconds = std::env::var("MEETING_RECORDER_VIDEO_TEST_SECONDS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(10);
    let resolution =
        std::env::var("MEETING_RECORDER_VIDEO_TEST_RESOLUTION").unwrap_or_else(|_| "720p".into());
    let fps = std::env::var("MEETING_RECORDER_VIDEO_TEST_FPS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(15);
    let config = VideoConfig::from_settings(&resolution, fps).unwrap();
    let source = list_sources()
        .unwrap()
        .into_iter()
        .find(|source| {
            if kind == "chrome" || kind == "brave" || kind == "window" {
                source.kind == "window" && source.title.contains("MeetingRecorder video test")
            } else {
                source.kind == "monitor"
            }
        })
        .expect("open the synthetic browser test page or connect a monitor");
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let directory =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../.tooling/video/{kind}-{nonce}"));
    fs::create_dir_all(&directory).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let origin = Instant::now();
    let input = microphone::list_input_devices()
        .unwrap()
        .into_iter()
        .find(|device| device.is_default)
        .expect("default microphone");
    let output = loopback::list_output_devices()
        .unwrap()
        .into_iter()
        .find(|device| device.is_default)
        .expect("default output");
    let (mic_ready, mic_rx) = sync_channel(1);
    let (sys_ready, sys_rx) = sync_channel(1);
    let mic_stop = stop.clone();
    let mic_path = directory.join("microphone.wav");
    let mic = thread::spawn(move || {
        microphone::capture_on_timeline(&input.id, &mic_path, &mic_stop, &mic_ready, Some(origin))
    });
    let sys_stop = stop.clone();
    let sys_path = directory.join("system.wav");
    let system = thread::spawn(move || {
        loopback::capture_on_timeline(&output.id, &sys_path, &sys_stop, &sys_ready, Some(origin))
    });
    mic_rx
        .recv_timeout(Duration::from_secs(10))
        .unwrap()
        .unwrap();
    sys_rx
        .recv_timeout(Duration::from_secs(10))
        .unwrap()
        .unwrap();
    let started = VideoRecorder::start_configured(
        directory.join("video.mp4"),
        "video-test".into(),
        (source, origin, config),
        None,
        directory.join("test.log"),
        Arc::new(AtomicBool::new(false)),
    );
    if let Err(error) = &started {
        stop.store(true, Ordering::Release);
        let _ = mic.join();
        let _ = system.join();
        panic!("video startup: {error}");
    }
    let mut video = started.unwrap_or_else(|_| unreachable!());
    let (cpu_start, _) = process_sample();
    let measuring = Instant::now();
    let mut ram_peak = 0;
    for _ in 0..seconds {
        thread::sleep(Duration::from_secs(1));
        ram_peak = ram_peak.max(process_sample().1);
    }
    let (cpu_end, _) = process_sample();
    let elapsed = measuring.elapsed().as_secs_f64();
    stop.store(true, Ordering::Release);
    video.stop.store(true, Ordering::Release);
    let mic_bytes = mic.join().unwrap().unwrap();
    let system_bytes = system.join().unwrap().unwrap();
    let result = video.finish();
    println!("VIDEO_RESULT kind={kind} state={result:?} directory={} CPU_one_core_pct={:.2} CPU_machine_pct={:.2} RAM_peak_MiB={:.2}",directory.display(),(cpu_end-cpu_start)/elapsed*100.0,(cpu_end-cpu_start)/elapsed*100.0/std::thread::available_parallelism().unwrap().get() as f64,ram_peak as f64/1048576.0);
    assert!(result.finalized, "{result:?}");
    assert!(result.error.is_none(), "{result:?}");
    let (frames, duration) = decode_video(&directory.join("video.mp4")).unwrap();
    let _runtime = encoder::Runtime::initialize().unwrap();
    unsafe {
        let path: Vec<u16> = directory
            .join("video.mp4")
            .to_string_lossy()
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let reader = MFCreateSourceReaderFromURL(PCWSTR(path.as_ptr()), None).unwrap();
        let native = reader
            .GetNativeMediaType(MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32, 0)
            .unwrap();
        assert_eq!(
            native.GetUINT64(&MF_MT_FRAME_SIZE).unwrap(),
            (u64::from(config.width) << 32) | u64::from(config.height)
        );
        let rate = native.GetUINT64(&MF_MT_FRAME_RATE).unwrap();
        let average = (rate >> 32) as f64 / (rate as u32) as f64;
        assert!(average > 0.0 && average <= config.fps as f64 + 0.1);
    }
    let mic_ms = u64::from(mic_bytes) * 1000 / 96_000;
    let system_ms = u64::from(system_bytes) * 1000 / 96_000;
    println!("DECODE frames={frames} video_ms={duration} mic_ms={mic_ms} system_ms={system_ms} delta_mic_ms={} delta_system_ms={} bytes={}",(duration as u64).abs_diff(mic_ms),(duration as u64).abs_diff(system_ms),fs::metadata(directory.join("video.mp4")).unwrap().len());
    assert!((duration as u64).abs_diff(mic_ms) < 500);
    assert!((duration as u64).abs_diff(system_ms) < 500);
    let path = directory.join("video.mp4");
    let size = fs::metadata(&path).unwrap().len();
    assert!(VideoRecorder::start(
        path.clone(),
        "duplicate".into(),
        select_source(&std::env::var("MEETING_RECORDER_VIDEO_TEST_SOURCE_ID").unwrap_or_default())
            .unwrap_or_else(|_| list_sources()
                .unwrap()
                .into_iter()
                .find(|s| s.kind == "monitor")
                .unwrap()),
        Instant::now(),
        None,
        directory.join("test.log"),
        Arc::new(AtomicBool::new(false))
    )
    .is_err());
    assert_eq!(fs::metadata(path).unwrap().len(), size);
}

#[test]
#[ignore = "decode an explicitly provided local MP4 produced by the desktop app"]
fn decode_existing_app_mp4() {
    let path =
        PathBuf::from(std::env::var("MEETING_RECORDER_VIDEO_DECODE_PATH").expect("MP4 path"));
    let (frames, duration) = decode_video(&path).unwrap();
    println!(
        "APP_MP4 frames={frames} duration_ms={duration} bytes={}",
        fs::metadata(path).unwrap().len()
    );
}
