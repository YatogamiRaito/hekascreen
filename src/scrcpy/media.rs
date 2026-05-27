use std::fmt;

use bevy::ecs::error::Result;
use ffmpeg_next::{Packet, codec, decoder, format::Pixel, frame, packet, software::scaling};
unsafe extern "C" fn get_hw_format(
    _ctx: *mut ffmpeg_next::ffi::AVCodecContext,
    pix_fmts: *const ffmpeg_next::ffi::AVPixelFormat,
) -> ffmpeg_next::ffi::AVPixelFormat {
    unsafe {
        let mut i = 0;
        while i < 1000 && *pix_fmts.offset(i) != ffmpeg_next::ffi::AVPixelFormat::AV_PIX_FMT_NONE {
            if *pix_fmts.offset(i) == ffmpeg_next::ffi::AVPixelFormat::AV_PIX_FMT_VAAPI {
                return ffmpeg_next::ffi::AVPixelFormat::AV_PIX_FMT_VAAPI;
            }
            i += 1;
        }
    }
    ffmpeg_next::ffi::AVPixelFormat::AV_PIX_FMT_NONE
}
use rust_i18n::t;
use serde::{Deserialize, Serialize};
use tokio::{io::AsyncReadExt, net::TcpStream};

const SC_PACKET_FLAG_CONFIG: u64 = 1u64 << 62;
const SC_PACKET_FLAG_KEY_FRAME: u64 = 1u64 << 61;
const SC_PACKET_PTS_MASK: u64 = SC_PACKET_FLAG_KEY_FRAME - 1;
pub async fn read_media_packet(socket: &mut TcpStream) -> Result<Packet, String> {
    // read header
    let mut header: [u8; 12] = [0; 12];
    socket
        .read_exact(&mut header)
        .await
        .map_err(|e| format!("{}: {}", t!("scrcpy.failedToReadFrameHeader"), e))?;

    let pts_flags = u64::from_be_bytes(header[0..8].try_into().unwrap());
    let len = u32::from_be_bytes(header[8..12].try_into().unwrap()) as usize;

    // Allocate packet with the required size inside FFmpeg directly.
    // len == 0 is valid: scrcpy server sends zero-size config packets as encoder
    // flush/reset signals (e.g. on device rotation). av_new_packet(pkt, 0) leaves
    // data = NULL, so data_mut() returns None — handle that case explicitly.
    let mut packet = Packet::empty();
    if len > 0 {
        packet.grow(len);
        if let Some(buf) = packet.data_mut() {
            socket
                .read_exact(&mut buf[..len])
                .await
                .map_err(|e| format!("{}: {}", t!("scrcpy.failedToReadFrameHeader"), e))?;
        } else {
            return Err(format!("Failed to access packet data buffer (len={})", len));
        }
    }

    if (pts_flags & SC_PACKET_FLAG_CONFIG) != 0 {
        packet.set_pts(None);
    } else {
        packet.set_pts(Some((pts_flags & SC_PACKET_PTS_MASK) as i64));
    }

    if (pts_flags & SC_PACKET_FLAG_KEY_FRAME) != 0 {
        packet.set_flags(packet.flags() | packet::Flags::KEY);
    }

    packet.set_dts(packet.pts());

    Ok(packet)
}

// Video Codec Constants
pub const SC_CODEC_ID_H264: u32 = 0x68_32_36_34;
pub const SC_CODEC_ID_H265: u32 = 0x68_32_36_35;
pub const SC_CODEC_ID_AV1: u32 = 0x00_61_76_31;

pub struct PacketMerger {
    config: Option<Vec<u8>>,
}

impl PacketMerger {
    pub fn new() -> Self {
        PacketMerger { config: None }
    }

    pub fn merge(&mut self, packet: &mut Packet) {
        let is_config = packet.pts().is_none();

        if is_config {
            if let Some(data) = packet.data() {
                log::info!("[PacketMerger] Received config packet of size {}", data.len());
                self.config = Some(data.to_vec());
            } else {
                self.config = Some(Vec::new());
            }
        } else if let Some(config_data) = &self.config {
            let config_size = config_data.len();
            if config_size > 0 {
                let media_size = packet.size();
                let new_size = config_size + media_size;
                log::info!("[PacketMerger] Merging config of size {} with media of size {}. Target size: {}", config_size, media_size, new_size);

                let old_size = packet.size();
                packet.grow(config_size);
                let grown_size = packet.size();
                log::info!("[PacketMerger] Grown packet from {} to {}.", old_size, grown_size);

                if let Some(buf) = packet.data_mut() {
                    // Shift original data in-place to make room for config
                    buf.copy_within(0..media_size, config_size);
                    // Copy config data to the beginning
                    buf[0..config_size].copy_from_slice(config_data);
                }
            }
            self.config = None;
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum VideoCodec {
    H264,
    H265,
    AV1,
}

impl From<VideoCodec> for codec::Id {
    fn from(codec: VideoCodec) -> Self {
        match codec {
            VideoCodec::H264 => Self::H264,
            VideoCodec::H265 => Self::HEVC,
            VideoCodec::AV1 => Self::AV1,
        }
    }
}

impl fmt::Display for VideoCodec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            VideoCodec::H264 => "h264",
            VideoCodec::H265 => "h265",
            VideoCodec::AV1 => "av1",
        };
        write!(f, "{}", s)
    }
}

pub struct VideoDecoder {
    pub decoder: decoder::Video,
    pub scaler: Option<scaling::Context>,
    pub width: u32,
    pub height: u32,
    pub frame_size: usize,
    pub must_merge_config: bool,
    pub packet_merger: PacketMerger,
    pub hw_device_ctx: *mut ffmpeg_next::ffi::AVBufferRef,
    pub cpu_frame: frame::Video,
    pub rgba_frame: frame::Video,
}

// Safety: VideoDecoder contains raw pointers (*mut AVBufferRef) which are not automatically Send.
// However, the raw pointer points to a thread-safe FFmpeg hardware device context that is only
// ever dereferenced or modified inside the single-threaded video decoder run loop (connection.rs).
// No concurrent access to these raw pointers occurs, making VideoDecoder safe to transfer (Send) across threads.
unsafe impl Send for VideoDecoder {}

impl Drop for VideoDecoder {
    fn drop(&mut self) {
        if !self.hw_device_ctx.is_null() {
            unsafe {
                ffmpeg_next::ffi::av_buffer_unref(&mut self.hw_device_ctx);
            }
        }
    }
}

impl VideoDecoder {
    pub fn new(codec_id: VideoCodec, width: u32, height: u32) -> Result<Self, String> {
        let sw_codec = decoder::find(codec_id.into())
            .ok_or_else(|| format!("FFmpeg codec '{:?}' not available", codec_id))?;
        let mut codec_context = codec::Context::new_with_codec(sw_codec);
        let flags = unsafe {
            let raw_flags = (*codec_context.as_mut_ptr()).flags;
            let flags = codec::Flags::from_bits(raw_flags as std::ffi::c_uint)
                .unwrap_or(codec::Flags::empty());
            flags | codec::Flags::LOW_DELAY
        };
        let mut threading = codec_context.threading();
        threading.count = 1;
        codec_context.set_threading(threading);
        codec_context.set_flags(flags);

        let mut hw_device_ctx: *mut ffmpeg_next::ffi::AVBufferRef = std::ptr::null_mut();
        let mut using_hw = false;

        if crate::config::LocalConfig::get().hw_decode {
            unsafe {
                let ret = ffmpeg_next::ffi::av_hwdevice_ctx_create(
                    &mut hw_device_ctx,
                    ffmpeg_next::ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_VAAPI,
                    std::ptr::null(), // auto-select
                    std::ptr::null_mut(),
                    0,
                );
                if ret >= 0 {
                    // Find hw_pix_fmt (AV_PIX_FMT_VAAPI) by looping with avcodec_get_hw_config()
                    let mut hw_pix_fmt = ffmpeg_next::ffi::AVPixelFormat::AV_PIX_FMT_NONE;
                    let raw_codec = sw_codec.as_ptr();
                    let mut i = 0;
                    loop {
                        let config = ffmpeg_next::ffi::avcodec_get_hw_config(raw_codec, i);
                        if config.is_null() {
                            break;
                        }
                        let config = &*config;
                        if // AV_CODEC_HW_CONFIG_METHOD_HW_DEVICE_CTX = 0x01
                        (config.methods & 0x01) != 0 && config.device_type == ffmpeg_next::ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_VAAPI {
                            hw_pix_fmt = config.pix_fmt;
                            break;
                        }
                        i += 1;
                    }

                    if hw_pix_fmt != ffmpeg_next::ffi::AVPixelFormat::AV_PIX_FMT_NONE {
                        // Attach the hardware device context reference to the codec context
                        (*codec_context.as_mut_ptr()).hw_device_ctx = ffmpeg_next::ffi::av_buffer_ref(hw_device_ctx);
                        // Set the get_format callback
                        (*codec_context.as_mut_ptr()).get_format = Some(get_hw_format);
                        using_hw = true;
                        log::info!("[HekaScreen] VAAPI hardware context configured with pixel format {:?}", hw_pix_fmt);
                    } else {
                        log::warn!("[HekaScreen] VAAPI hardware config not found for this codec, falling back to SW");
                        ffmpeg_next::ffi::av_buffer_unref(&mut hw_device_ctx);
                    }
                } else {
                    eprintln!("[HekaScreen] VAAPI device init failed (ret={}), falling back to SW", ret);
                }
            }
        }

        let video_decoder = codec_context.decoder().video()
            .map_err(|e| format!("Failed to initialize video decoder: {}", e))?;

        if using_hw {
            log::info!("[HekaScreen] HW decoding (VAAPI) initialized successfully");
        }

        Ok(Self {
            decoder: video_decoder,
            scaler: None,
            width,
            height,
            must_merge_config: matches!(codec_id, VideoCodec::H264 | VideoCodec::H265),
            packet_merger: PacketMerger::new(),
            frame_size: (width * height * 4) as usize,
            hw_device_ctx,
            cpu_frame: frame::Video::empty(),
            rgba_frame: frame::Video::empty(),
        })
    }

    /// Returns true if a VAAPI hardware device context was successfully created.
    /// This is the ground-truth check — the setting `hw_decode` in config only
    /// requests hardware decoding; `hw_active()` tells you whether it actually worked.
    pub fn hw_active(&self) -> bool {
        !self.hw_device_ctx.is_null()
    }

    pub fn update(&mut self, frame: &frame::Video) -> bool {
        let width = frame.width();
        let height = frame.height();

        if self.scaler.is_none() || width != self.width || height != self.height {
            self.cpu_frame = frame::Video::empty();
            self.rgba_frame = frame::Video::empty();
            let frame_format = unsafe {
                let raw_frame = frame.as_ptr();
                if (*raw_frame).format == ffmpeg_next::ffi::AVPixelFormat::AV_PIX_FMT_VAAPI as std::ffi::c_int {
                    let ret = ffmpeg_next::ffi::av_hwframe_transfer_data(
                        self.cpu_frame.as_mut_ptr(),
                        raw_frame,
                        0,
                    );
                    if ret >= 0 {
                        self.cpu_frame.format()
                    } else {
                        frame.format()
                    }
                } else {
                    frame.format()
                }
            };

            self.width = width;
            self.height = height;
            self.scaler = Some(
                scaling::Context::get(
                    frame_format,
                    width,
                    height,
                    Pixel::RGBA,
                    width,
                    height,
                    scaling::Flags::BILINEAR,
                )
                .unwrap(),
            );
            self.frame_size = (width * height * 4) as usize;

            true
        } else {
            false
        }
    }

    pub fn convert_to_rgba(&mut self, decoded: &frame::Video) -> Result<&frame::Video, String> {
        let frame_to_scale = unsafe {
            let raw_frame = decoded.as_ptr();
            if (*raw_frame).format == ffmpeg_next::ffi::AVPixelFormat::AV_PIX_FMT_VAAPI as std::ffi::c_int {
                let ret = ffmpeg_next::ffi::av_hwframe_transfer_data(
                    self.cpu_frame.as_mut_ptr(),
                    raw_frame,
                    0,
                );
                if ret < 0 {
                    return Err(format!("Failed to transfer VAAPI hardware frame to CPU: {}", ret));
                } else {
                    self.cpu_frame.set_pts(decoded.pts());
                    &self.cpu_frame
                }
            } else {
                decoded
            }
        };

        let scaler = self.scaler.as_mut().ok_or("Scaler not initialized. Call update() before convert_to_rgba().")?;
        scaler.run(frame_to_scale, &mut self.rgba_frame)
            .map_err(|e| format!("Scaler run failed: {}", e))?;
        Ok(&self.rgba_frame)
    }
}

pub enum VideoMsg {
    Data {
        data: Vec<u8>,
        width: u32,
        height: u32,
        decode_time_ms: f32,
        timestamp_us: u64,
    },
    /// Sent once when the stream is established. Carries ground-truth info
    /// about what is *actually* active (codec from server, hw from VAAPI init).
    StreamInfo {
        codec: String,
        hw_active: bool,
        width: u32,
        height: u32,
    },
    ScriptError {
        error: String,
    },
    ScriptClearError,
    Close,
}
