use super::{fit_rect_for, VideoConfig};
use std::{mem::ManuallyDrop, path::Path};
use windows::{
    core::{Interface, PCWSTR},
    Graphics::DirectX::Direct3D11::IDirect3DDevice,
    Win32::{
        Foundation::{HMODULE, RECT},
        Graphics::{
            Direct3D::D3D_DRIVER_TYPE_HARDWARE,
            Direct3D11::*,
            Dxgi::{Common::*, IDXGIDevice},
        },
        Media::MediaFoundation::*,
        System::WinRT::{
            Direct3D11::CreateDirect3D11DeviceFromDXGIDevice, RoInitialize, RoUninitialize,
            RO_INIT_MULTITHREADED,
        },
    },
};

pub(super) struct Runtime;
impl Runtime {
    pub(super) fn initialize() -> Result<Self, String> {
        unsafe {
            RoInitialize(RO_INIT_MULTITHREADED).map_err(|e| e.to_string())?;
            if let Err(error) = MFStartup(MF_VERSION, MFSTARTUP_FULL) {
                RoUninitialize();
                return Err(error.to_string());
            }
        }
        Ok(Self)
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        unsafe {
            let _ = MFShutdown();
            RoUninitialize();
        }
    }
}

pub(super) struct GraphicsDevice {
    pub device: ID3D11Device,
    pub context: ID3D11DeviceContext,
    pub capture_device: IDirect3DDevice,
}
impl GraphicsDevice {
    pub(super) fn new() -> windows::core::Result<Self> {
        unsafe {
            let mut device = None;
            let mut context = None;
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_VIDEO_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )?;
            let device = device.unwrap();
            let context = context.unwrap();
            let multithread: ID3D11Multithread = context.cast()?;
            let _ = multithread.SetMultithreadProtected(true);
            let dxgi: IDXGIDevice = device.cast()?;
            let capture_device = CreateDirect3D11DeviceFromDXGIDevice(&dxgi)?.cast()?;
            Ok(Self {
                device,
                context,
                capture_device,
            })
        }
    }
    pub(super) fn texture(
        &self,
        width: u32,
        height: u32,
        format: DXGI_FORMAT,
    ) -> windows::core::Result<ID3D11Texture2D> {
        let desc = D3D11_TEXTURE2D_DESC {
            Width: width,
            Height: height,
            MipLevels: 1,
            ArraySize: 1,
            Format: format,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: D3D11_BIND_RENDER_TARGET.0 as u32,
            ..Default::default()
        };
        let mut texture = None;
        unsafe {
            self.device
                .CreateTexture2D(&desc, None, Some(&mut texture))?;
        }
        Ok(texture.unwrap())
    }
}

pub(super) struct Encoder {
    config: VideoConfig,
    writer: IMFSinkWriter,
    stream: u32,
    video_device: ID3D11VideoDevice,
    video_context: ID3D11VideoContext,
    processor: Option<(
        u32,
        u32,
        ID3D11VideoProcessorEnumerator,
        ID3D11VideoProcessor,
    )>,
    finalized: bool,
    pub name: String,
    pub hardware: bool,
}
impl Encoder {
    pub(super) fn new(
        path: &Path,
        gpu: &GraphicsDevice,
        config: VideoConfig,
    ) -> Result<Self, String> {
        let create = |hardware| unsafe { Self::create_writer(path, gpu, hardware, config) };
        // Initialization fallback only: never restart/overwrite an encoder that already wrote frames.
        let (writer, stream) = create(true)
            .or_else(|_| create(false))
            .map_err(|e| e.to_string())?;
        let (name, hardware) = encoder_identity(&writer, stream);
        Ok(Self {
            config,
            writer,
            stream,
            video_device: gpu.device.cast().map_err(|e| e.to_string())?,
            video_context: gpu.context.cast().map_err(|e| e.to_string())?,
            processor: None,
            finalized: false,
            name,
            hardware,
        })
    }
    unsafe fn create_writer(
        path: &Path,
        gpu: &GraphicsDevice,
        hardware: bool,
        config: VideoConfig,
    ) -> windows::core::Result<(IMFSinkWriter, u32)> {
        unsafe {
            let mut attributes = None;
            MFCreateAttributes(&mut attributes, 5)?;
            let attributes = attributes.unwrap();
            attributes.SetUINT32(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, hardware as u32)?;
            // Keep default backpressure: a slow encoder must not queue unbounded GPU textures.
            attributes.SetUINT32(&MF_LOW_LATENCY, 1)?;
            let mut reset_token = 0;
            let mut manager = None;
            MFCreateDXGIDeviceManager(&mut reset_token, &mut manager)?;
            let manager = manager.unwrap();
            manager.ResetDevice(&gpu.device, reset_token)?;
            attributes.SetUnknown(&MF_SINK_WRITER_D3D_MANAGER, &manager)?;
            let url: Vec<u16> = path
                .as_os_str()
                .to_string_lossy()
                .encode_utf16()
                .chain(Some(0))
                .collect();
            let writer = MFCreateSinkWriterFromURL(PCWSTR(url.as_ptr()), None, &attributes)?;
            let output = media_type(&MFVideoFormat_H264, config)?;
            output.SetUINT32(&MF_MT_AVG_BITRATE, 2_000_000)?;
            output.SetUINT32(&MF_MT_MPEG2_PROFILE, 66)?;
            let stream = writer.AddStream(&output)?;
            let input = media_type(&MFVideoFormat_NV12, config)?;
            writer.SetInputMediaType(stream, &input, None)?;
            writer.BeginWriting()?;
            Ok((writer, stream))
        }
    }
    pub(super) fn write(
        &mut self,
        gpu: &GraphicsDevice,
        texture: &ID3D11Texture2D,
        width: u32,
        height: u32,
        pts: i64,
        duration: i64,
    ) -> Result<(), String> {
        self.write_native(gpu, texture, width, height, pts, duration)
            .map_err(|e| e.to_string())
    }
    fn write_native(
        &mut self,
        gpu: &GraphicsDevice,
        texture: &ID3D11Texture2D,
        width: u32,
        height: u32,
        pts: i64,
        duration: i64,
    ) -> windows::core::Result<()> {
        unsafe {
            if self
                .processor
                .as_ref()
                .is_none_or(|(w, h, _, _)| (*w, *h) != (width, height))
            {
                let rate = DXGI_RATIONAL {
                    Numerator: self.config.fps as u32,
                    Denominator: 1,
                };
                let desc = D3D11_VIDEO_PROCESSOR_CONTENT_DESC {
                    InputFrameFormat: D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
                    InputFrameRate: rate,
                    InputWidth: width,
                    InputHeight: height,
                    OutputFrameRate: rate,
                    OutputWidth: self.config.width,
                    OutputHeight: self.config.height,
                    Usage: D3D11_VIDEO_USAGE_PLAYBACK_NORMAL,
                };
                let enumerator = self.video_device.CreateVideoProcessorEnumerator(&desc)?;
                let processor = self.video_device.CreateVideoProcessor(&enumerator, 0)?;
                self.processor = Some((width, height, enumerator, processor));
            }
            let (_, _, enumerator, processor) = self.processor.as_ref().unwrap();
            let output = gpu.texture(self.config.width, self.config.height, DXGI_FORMAT_NV12)?;
            let mut input_view = None;
            let mut output_view = None;
            self.video_device.CreateVideoProcessorInputView(
                texture,
                enumerator,
                &D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC {
                    ViewDimension: D3D11_VPIV_DIMENSION_TEXTURE2D,
                    ..Default::default()
                },
                Some(&mut input_view),
            )?;
            self.video_device.CreateVideoProcessorOutputView(
                &output,
                enumerator,
                &D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC {
                    ViewDimension: D3D11_VPOV_DIMENSION_TEXTURE2D,
                    ..Default::default()
                },
                Some(&mut output_view),
            )?;
            let (left, top, right, bottom) =
                fit_rect_for(width, height, self.config).map_err(|error| {
                    windows::core::Error::new(windows::Win32::Foundation::E_INVALIDARG, error)
                })?;
            let source = RECT {
                left: 0,
                top: 0,
                right: width as i32,
                bottom: height as i32,
            };
            let destination = RECT {
                left,
                top,
                right,
                bottom,
            };
            self.video_context.VideoProcessorSetStreamFrameFormat(
                processor,
                0,
                D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
            );
            self.video_context
                .VideoProcessorSetStreamSourceRect(processor, 0, true, Some(&source));
            self.video_context.VideoProcessorSetStreamDestRect(
                processor,
                0,
                true,
                Some(&destination),
            );
            let black = D3D11_VIDEO_COLOR {
                Anonymous: D3D11_VIDEO_COLOR_0 {
                    YCbCr: D3D11_VIDEO_COLOR_YCbCrA {
                        Y: 0.0,
                        Cb: 0.5,
                        Cr: 0.5,
                        A: 1.0,
                    },
                },
            };
            self.video_context
                .VideoProcessorSetOutputBackgroundColor(processor, true, &black);
            let mut stream = D3D11_VIDEO_PROCESSOR_STREAM {
                Enable: true.into(),
                pInputSurface: ManuallyDrop::new(input_view),
                ..Default::default()
            };
            let converted = self.video_context.VideoProcessorBlt(
                processor,
                &output_view.unwrap(),
                0,
                std::slice::from_ref(&stream),
            );
            // Generated structs wrap COM interfaces in ManuallyDrop; release our reference explicitly.
            ManuallyDrop::drop(&mut stream.pInputSurface);
            converted?;
            gpu.context.Flush();
            let buffer = MFCreateDXGISurfaceBuffer(&ID3D11Texture2D::IID, &output, 0, false)?;
            let contiguous: IMF2DBuffer = buffer.cast()?;
            buffer.SetCurrentLength(contiguous.GetContiguousLength()?)?;
            let sample = MFCreateSample()?;
            sample.AddBuffer(&buffer)?;
            sample.SetSampleTime(pts)?;
            sample.SetSampleDuration(duration)?;
            self.writer.WriteSample(self.stream, &sample)
        }
    }
    pub(super) fn finish(&mut self) -> Result<(), String> {
        let result = unsafe { self.writer.Finalize() }.map_err(|e| e.to_string());
        self.finalized = result.is_ok();
        result
    }
}
impl Drop for Encoder {
    fn drop(&mut self) {
        if !self.finalized {
            let _ = self.finish();
        }
    }
}
unsafe fn media_type(
    subtype: &windows::core::GUID,
    config: VideoConfig,
) -> windows::core::Result<IMFMediaType> {
    unsafe {
        let media = MFCreateMediaType()?;
        media.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
        media.SetGUID(&MF_MT_SUBTYPE, subtype)?;
        media.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
        media.SetUINT64(
            &MF_MT_FRAME_SIZE,
            ((config.width as u64) << 32) | config.height as u64,
        )?;
        media.SetUINT64(&MF_MT_FRAME_RATE, (config.fps << 32) | 1)?;
        media.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, (1_u64 << 32) | 1)?;
        Ok(media)
    }
}
fn encoder_identity(writer: &IMFSinkWriter, stream: u32) -> (String, bool) {
    let query = || unsafe {
        let extended: IMFSinkWriterEx = writer.cast()?;
        for index in 0..8 {
            let mut transform = None;
            let mut category = windows::core::GUID::zeroed();
            if extended
                .GetTransformForStream(stream, index, Some(&mut category), &mut transform)
                .is_err()
            {
                break;
            }
            if category == MFT_CATEGORY_VIDEO_ENCODER {
                let attributes = transform.unwrap().GetAttributes()?;
                let mut name = [0_u16; 512];
                let name = if attributes
                    .GetString(&MFT_FRIENDLY_NAME_Attribute, &mut name, None)
                    .is_ok()
                {
                    String::from_utf16_lossy(
                        &name[..name.iter().position(|v| *v == 0).unwrap_or(name.len())],
                    )
                } else {
                    "Media Foundation H.264".into()
                };
                let mut hardware_url = [0_u16; 512];
                let hardware = attributes
                    .GetString(&MFT_ENUM_HARDWARE_URL_Attribute, &mut hardware_url, None)
                    .is_ok();
                return Ok::<_, windows::core::Error>((name, hardware));
            }
        }
        Ok((
            "Media Foundation H.264 (identificação indisponível)".into(),
            false,
        ))
    };
    query().unwrap_or_else(|_| {
        (
            "Media Foundation H.264 (identificação indisponível)".into(),
            false,
        )
    })
}
