use super::{
    encoder::{Encoder, GraphicsDevice, Runtime},
    frame_index_for, VideoConfig, VideoSource, VideoState,
};

fn optional_frame<T>(result: windows::core::Result<T>) -> Result<Option<T>, String> {
    match result {
        Ok(frame) => Ok(Some(frame)),
        // A null WinRT interface projects to Error::empty (S_OK), meaning no frame yet.
        Err(error) if error.code().is_ok() => Ok(None),
        Err(error) => Err(format!("Falha ao receber frame do WGC: {error}")),
    }
}
struct FrameWait {
    since: Instant,
}
impl FrameWait {
    fn new() -> Self {
        Self {
            since: Instant::now(),
        }
    }
    fn restart(&mut self) {
        self.since = Instant::now();
    }
    fn expired(&self) -> bool {
        self.since.elapsed() > Duration::from_secs(5)
    }
}

use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::SyncSender,
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};
use windows::{
    core::{factory, IInspectable, Interface},
    Foundation::TypedEventHandler,
    Graphics::{
        Capture::{Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession},
        DirectX::DirectXPixelFormat,
    },
    Win32::{
        Foundation::HWND,
        Graphics::{
            Direct3D11::ID3D11Texture2D, Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM, Gdi::HMONITOR,
        },
        System::WinRT::{
            Direct3D11::IDirect3DDxgiInterfaceAccess,
            Graphics::Capture::IGraphicsCaptureItemInterop,
        },
    },
};

pub(super) fn record(
    path: &Path,
    source: &VideoSource,
    stop: &AtomicBool,
    origin: Instant,
    ready: &SyncSender<Result<(), String>>,
    state: &Arc<Mutex<VideoState>>,
    config: VideoConfig,
) -> Result<(), String> {
    let _runtime = Runtime::initialize()?;
    if !GraphicsCaptureSession::IsSupported().map_err(|e| e.to_string())? {
        return Err("Windows Graphics Capture não está disponível neste sistema.".into());
    }
    let gpu = GraphicsDevice::new().map_err(|e| e.to_string())?;
    let interop: IGraphicsCaptureItemInterop =
        factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>().map_err(|e| e.to_string())?;
    let handle = source
        .id
        .split(':')
        .nth(1)
        .and_then(|id| id.parse::<usize>().ok())
        .ok_or("Fonte de vídeo inválida")?;
    let item: GraphicsCaptureItem = unsafe {
        if source.kind == "window" {
            interop.CreateForWindow(HWND(handle as *mut _))
        } else {
            interop.CreateForMonitor(HMONITOR(handle as *mut _))
        }
    }
    .map_err(|e| e.to_string())?;
    let mut size = item.Size().map_err(|e| e.to_string())?;
    if size.Width <= 0 || size.Height <= 0 {
        return Err("A fonte está minimizada ou sem área capturável.".into());
    }
    let closed = Arc::new(AtomicBool::new(false));
    let callback_closed = closed.clone();
    let token = item
        .Closed(
            &TypedEventHandler::<GraphicsCaptureItem, IInspectable>::new(move |_, _| {
                callback_closed.store(true, Ordering::Release);
                Ok(())
            }),
        )
        .map_err(|e| e.to_string())?;
    let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
        &gpu.capture_device,
        DirectXPixelFormat::B8G8R8A8UIntNormalized,
        2,
        size,
    )
    .map_err(|e| e.to_string())?;
    let session = pool
        .CreateCaptureSession(&item)
        .map_err(|e| e.to_string())?;
    // Supported on newer Windows; failure preserves the 1903+ baseline and our own FPS limiter.
    let _ = session.SetMinUpdateInterval(windows::Foundation::TimeSpan {
        Duration: 10_000_000 / config.fps as i64,
    });
    let mut encoder = Encoder::new(path, &gpu, config)?;
    {
        let mut state = state.lock().unwrap_or_else(|e| e.into_inner());
        state.encoder = encoder.name.clone();
        state.hardware_accelerated = encoder.hardware;
    }
    let capture_result = (|| {
        session.StartCapture().map_err(|e| e.to_string())?;
        let mut cached: Option<ID3D11Texture2D> = None;
        let mut previous = None;
        let mut next_tick = Instant::now();
        let mut frame_wait = FrameWait::new();
        while !stop.load(Ordering::Acquire) {
            if closed.load(Ordering::Acquire) {
                return Err("A janela ou monitor da captura foi fechado/removido; o áudio continua disponível.".into());
            }
            if let Some(frame) = optional_frame(pool.TryGetNextFrame())? {
                let content = frame.ContentSize().map_err(|e| e.to_string())?;
                if content.Width > 0 && content.Height > 0 && content != size {
                    frame.Close().map_err(|e| e.to_string())?;
                    size = content;
                    cached = None;
                    pool.Recreate(
                        &gpu.capture_device,
                        DirectXPixelFormat::B8G8R8A8UIntNormalized,
                        2,
                        size,
                    )
                    .map_err(|e| e.to_string())?;
                    frame_wait.restart();
                } else {
                    let access: IDirect3DDxgiInterfaceAccess = frame
                        .Surface()
                        .map_err(|e| e.to_string())?
                        .cast()
                        .map_err(|e| e.to_string())?;
                    let texture: ID3D11Texture2D =
                        unsafe { access.GetInterface() }.map_err(|e| e.to_string())?;
                    if cached.is_none() {
                        cached = Some(
                            gpu.texture(
                                size.Width as u32,
                                size.Height as u32,
                                DXGI_FORMAT_B8G8R8A8_UNORM,
                            )
                            .map_err(|e| e.to_string())?,
                        );
                    }
                    unsafe {
                        gpu.context.CopyResource(cached.as_ref().unwrap(), &texture);
                    }
                    frame.Close().map_err(|e| e.to_string())?;
                }
            }
            if let Some(texture) = &cached {
                let index = frame_index_for(previous, origin.elapsed(), config.fps);
                // Seed timestamp zero with the first available image so setup delay does not shift the whole video.
                if previous.is_none() && index > 0 {
                    encoder.write(
                        &gpu,
                        texture,
                        size.Width as u32,
                        size.Height as u32,
                        0,
                        (index * 10_000_000 / config.fps) as i64,
                    )?;
                }
                encoder.write(
                    &gpu,
                    texture,
                    size.Width as u32,
                    size.Height as u32,
                    (index * 10_000_000 / config.fps) as i64,
                    10_000_000 / config.fps as i64,
                )?;
                {
                    let mut state = state.lock().unwrap_or_else(|e| e.into_inner());
                    state.status = "recording".into();
                    state.frames_written += if previous.is_none() && index > 0 {
                        2
                    } else {
                        1
                    };
                    state.skipped_frames +=
                        previous.map_or(0, |last| index.saturating_sub(last + 1));
                    state.duration_ms = (index + 1) * 1000 / config.fps;
                }
                if previous.is_none() {
                    let _ = ready.try_send(Ok(()));
                }
                previous = Some(index);
            } else if frame_wait.expired() {
                return Err("A fonte de vídeo não entregou frames em cinco segundos.".into());
            }
            next_tick += Duration::from_nanos(1_000_000_000 / config.fps);
            let now = Instant::now();
            if next_tick > now {
                thread::sleep(next_tick - now);
            } else {
                next_tick = now;
            }
        }
        Ok(())
    })();
    // Always close WGC before flushing the encoder, including device/target failures.
    let _ = session.Close();
    let _ = pool.Close();
    let _ = item.RemoveClosed(token);
    if let Err(error) = encoder.finish() {
        state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .finalization_error = Some(error.clone());
        return Err(format!("Não foi possível finalizar o MP4: {error}"));
    }
    capture_result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_pool_is_normal_but_device_failure_is_reported() {
        assert!(optional_frame::<u8>(Err(windows::core::Error::empty()))
            .unwrap()
            .is_none());
        assert!(optional_frame::<u8>(Err(windows::core::Error::from_hresult(
            windows::Win32::Foundation::E_ACCESSDENIED
        )))
        .is_err());
        assert_eq!(optional_frame(Ok(42)).unwrap(), Some(42));
    }
    #[test]
    fn resize_restarts_the_frame_timeout_after_long_running_capture() {
        let mut wait = FrameWait {
            since: Instant::now() - Duration::from_secs(6),
        };
        assert!(wait.expired());
        wait.restart();
        assert!(!wait.expired());
    }
}
