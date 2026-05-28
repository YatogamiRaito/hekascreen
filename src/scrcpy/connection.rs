use std::time::Duration;

use ffmpeg_next::frame;
use rust_i18n::t;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{
        TcpStream,
        tcp::{OwnedReadHalf, OwnedWriteHalf},
    },
    sync::{
        broadcast::{self, error::RecvError},
        mpsc::UnboundedSender,
        oneshot, watch,
    },
    time::timeout,
};
use tokio_util::sync::CancellationToken;

use crate::{
    mask::mask_command::MaskCommand,
    scrcpy::{
        control_msg::{ScrcpyControlMsg, ScrcpyDeviceMsg},
        media::{
            SC_CODEC_ID_AV1, SC_CODEC_ID_H264, SC_CODEC_ID_H265, VideoCodec, VideoDecoder,
            VideoMsg, read_media_packet,
        },
    },
    utils::{mask_win_move_helper, share::ControlledDevice},
};

pub fn parse_video_metadata_buf(
    buf: &[u8; 12],
    height_buf: Option<&[u8; 4]>,
) -> Result<(VideoCodec, u32, u32), String> {
    let raw_codec_id = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
    let val2 = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);
    let val3 = u32::from_be_bytes([buf[8], buf[9], buf[10], buf[11]]);

    let (width, height) = if (val2 & 0x80000000) != 0 {
        if let Some(h_buf) = height_buf {
            let width = val3;
            let height = u32::from_be_bytes(*h_buf);
            (width, height)
        } else {
            return Err("Missing height buffer for v3.0+ SessionMeta format".to_string());
        }
    } else {
        let width = val2;
        let height = val3;
        (width, height)
    };

    let codec = match raw_codec_id {
        SC_CODEC_ID_H264 => VideoCodec::H264,
        SC_CODEC_ID_H265 => VideoCodec::H265,
        SC_CODEC_ID_AV1 => VideoCodec::AV1,
        _ => return Err(format!("Unknown video codec ID: 0x{:x}", raw_codec_id)),
    };

    Ok((codec, width, height))
}

pub fn parse_device_name(buf: &[u8]) -> String {
    if let Ok(device_name_raw) = std::str::from_utf8(buf) {
        device_name_raw.trim_end_matches(char::from(0)).to_string()
    } else {
        "INVALID_NAME".to_string()
    }
}

pub struct ScrcpyConnection {
    pub socket: TcpStream,
}

impl ScrcpyConnection {
    pub fn new(socket: TcpStream) -> Self {
        ScrcpyConnection { socket }
    }

    async fn read_device_metadata(&mut self, scid: String) -> Result<(), String> {
        // read metadata (device name)
        let mut buf: [u8; 64] = [0; 64];
        match self.socket.read(&mut buf).await {
            Err(e) => Err(format!(
                "{}: {}",
                t!("scrcpy.failedToReadControlMetadata"),
                e
            )),
            Ok(0) => Err(format!(
                "{}: None",
                t!("scrcpy.failedToReadControlMetadata")
            )),
            Ok(n) => {
                let device_name = parse_device_name(&buf[..n]);
                if device_name == "INVALID_NAME" {
                    log::warn!("[Controller] {}", t!("scrcpy.invalidDeviceName"));
                }
                ControlledDevice::update_device_name(scid, device_name).await;
                Ok(())
            }
        }
    }

    async fn control_writer(
        mut write_half: OwnedWriteHalf,
        token: CancellationToken,
        mut cs_rx: broadcast::Receiver<ScrcpyControlMsg>,
        mut watch_rx: watch::Receiver<(u32, u32)>,
        scid: String,
    ) {
        tokio::select! {
            _ = token.cancelled()=>{
                log::info!("[Controller] {}", t!("scrcpy.controlConnectionCancelled"));
            }
            _ = async {
                loop {
                    match cs_rx.recv().await {
                        Ok(mut msg) => {
                                // scale position
                                match &mut msg {
                                    ScrcpyControlMsg::InjectTouchEvent {
                                        x,
                                        y,
                                        w,
                                        h,
                                        action: _,
                                        pointer_id: _,
                                        pressure: _,
                                        action_button: _,
                                        buttons: _,
                                    } => {
                                        let mut device_w = 0;
                                        let mut device_h = 0;
                                        if let Some((dw, dh)) = ControlledDevice::get_device_size(&scid).await {
                                            device_w = dw;
                                            device_h = dh;
                                        }
                                        if device_w == 0 || device_h == 0 {
                                            let (dw, dh) = watch_rx.borrow_and_update().clone();
                                            device_w = dw;
                                            device_h = dh;
                                        }
                                        let (old_x, old_y) = (*x, *y);
                                        let (old_w, old_h) = (*w, *h);
                                        if old_w > 0 && old_h > 0 {
                                            *x = old_x * device_w as i32 / old_w as i32;
                                            *y = old_y * device_h as i32 / old_h as i32;
                                        }
                                        *w = device_w as u16;
                                        *h = device_h as u16;
                                    }
                                    ScrcpyControlMsg::InjectScrollEvent {
                                        x,
                                        y,
                                        w,
                                        h,
                                        hscroll: _,
                                        vscroll: _,
                                        buttons: _,
                                    } => {
                                        let mut device_w = 0;
                                        let mut device_h = 0;
                                        if let Some((dw, dh)) = ControlledDevice::get_device_size(&scid).await {
                                            device_w = dw;
                                            device_h = dh;
                                        }
                                        if device_w == 0 || device_h == 0 {
                                            let (dw, dh) = watch_rx.borrow_and_update().clone();
                                            device_w = dw;
                                            device_h = dh;
                                        }
                                        let (old_x, old_y) = (*x, *y);
                                        let (old_w, old_h) = (*w, *h);
                                        if old_w > 0 && old_h > 0 {
                                            *x = old_x * device_w as i32 / old_w as i32;
                                            *y = old_y * device_h as i32 / old_h as i32;
                                        }
                                        *w = device_w as u16;
                                        *h = device_h as u16;
                                    }
                                    _ => {}
                                };
                                 let data:Vec<u8> = msg.into();
                                 if let Err(e) = write_half.write_all(&data).await {
                                     log::error!("[Controller] {}: {}", t!("scrcpy.controlConnWriteFailed"),e);
                                 } else {
                                     let now = std::time::SystemTime::now()
                                         .duration_since(std::time::UNIX_EPOCH)
                                         .unwrap_or_default()
                                         .as_micros() as u64;
                                     crate::utils::LAST_INPUT_TIME_MICROS.store(now, std::sync::atomic::Ordering::Relaxed);
                                 }
                        }
                        Err(RecvError::Lagged(skipped)) => {
                             log::warn!("[Controller] {}",t!("controller.csReceiverLagged", skipped => skipped));
                        }
                        Err(e) => {
                            log::info!("[Controller] {}: {}", t!("scrcpy.controlChannelClosed"),e);
                            break;
                        }
                    }
                }
            }=>{
                log::error!("[Controller] {}", t!("scrcpy.controlCnnShutdownUnexpectedly"));
            }
        }
        timeout(Duration::from_millis(500), write_half.shutdown())
            .await
            .ok();
    }

    async fn control_reader_handler(
        mut read_half: OwnedReadHalf,
        cr_tx: UnboundedSender<ScrcpyDeviceMsg>,
        watch_tx: watch::Sender<(u32, u32)>,
        scid: &str,
        main: bool,
    ) {
        loop {
            match ScrcpyDeviceMsg::read_msg(&mut read_half, scid.to_string()).await {
                Ok(msg) => {
                    if let ScrcpyDeviceMsg::Rotation {
                        rotation: _,
                        width,
                        height,
                        scid,
                    } = msg.clone()
                    {
                        ControlledDevice::update_device_size(scid, (width, height)).await;
                        let _ = watch_tx.send((width, height));
                    }
                    // only forward other message from main device
                    if main {
                        if let Err(e) = cr_tx.send(msg) {
                            log::warn!("[Controller] Device message receiver dropped: {}", e);
                            break;
                        }
                    }
                }
                Err(e) => {
                    log::error!("[Controller] {}", e);
                    break;
                }
            };
        }
    }

    async fn control_reader(
        read_half: OwnedReadHalf,
        token: CancellationToken,
        cr_tx: UnboundedSender<ScrcpyDeviceMsg>,
        watch_tx: watch::Sender<(u32, u32)>,
        scid: &str,
        main: bool,
    ) {
        tokio::select! {
            _ = token.cancelled()=>{
                log::info!("[Controller] {}", t!("scrcpy.controlConnectionReaderCancelled"));
            }
            _ = Self::control_reader_handler(read_half, cr_tx, watch_tx, scid, main)=>{
                log::error!("[Controller] {}", t!("scrcpy.controlReadShutdownUnexpectedly"));
            }
        }
        // no need to shutdown the read_half
    }

    pub async fn handle_control(
        mut self,
        cs_rx: broadcast::Receiver<ScrcpyControlMsg>,
        cr_tx: UnboundedSender<ScrcpyDeviceMsg>,
        m_tx: crossbeam_channel::Sender<(MaskCommand, oneshot::Sender<Result<String, String>>)>,
        scid: String,
        main: bool,
        token: CancellationToken,
        meta_flag: bool,
    ) {
        log::info!("[Controller] {}", t!("scrcpy.handleControlConnection"));
        if meta_flag {
            if let Err(e) = self.read_device_metadata(scid.to_string()).await {
                log::error!("[Controller] {}", e);
                token.cancel();
                return;
            }
        }

        let (read_half, write_half) = self.socket.into_split();
        let finnal_token = token.clone();
        let token_copy = token.clone();
        let (watch_tx, watch_rx) = watch::channel::<(u32, u32)>((0, 0)); // share device size with writer
        if main {
            let (oneshot_tx, oneshot_rx) = oneshot::channel::<Result<String, String>>();
            if let Err(e) = m_tx.send((
                MaskCommand::DeviceConnectionChange { connect: true },
                oneshot_tx,
            )) {
                log::error!("[Controller] Failed to send DeviceConnectionChange connect: {}", e);
            } else {
                crate::utils::wakeup_bevy();
                let _ = oneshot_rx.await;
            }
        }

        tokio::select! {
            _ = Self::control_writer(write_half, token, cs_rx, watch_rx, scid.clone()) => {finnal_token.cancel();}
            _ = Self::control_reader(read_half, token_copy, cr_tx, watch_tx, &scid, main) => {finnal_token.cancel();}
        }

        log::info!("[Controller] {}", t!("scrcpy.controlConnectionClosed"));
        if main {
            let (oneshot_tx, oneshot_rx) = oneshot::channel::<Result<String, String>>();
            if let Err(e) = m_tx.send((
                MaskCommand::DeviceConnectionChange { connect: false },
                oneshot_tx,
            )) {
                log::error!("[Controller] Failed to send DeviceConnectionChange disconnect: {}", e);
            } else {
                crate::utils::wakeup_bevy();
                let _ = oneshot_rx.await;
            }
        }
    }

    async fn video_handler(
        &mut self,
        v_tx: crossbeam_channel::Sender<VideoMsg>,
        m_tx: crossbeam_channel::Sender<(MaskCommand, oneshot::Sender<Result<String, String>>)>,
        scid: &str,
        recycle_rx: crossbeam_channel::Receiver<Vec<u8>>,
        recycle_tx: crossbeam_channel::Sender<Vec<u8>>,
    ) {
        // read metadata
        let mut buf: [u8; 12] = [0; 12];
        let mut video_decoder = match self.socket.read_exact(&mut buf).await {
            Err(_) => {
                log::error!("[Controller] {}", t!("scrcpy.failedToReadVideoMetadata"));
                return;
            }
            Ok(_) => {
                let val2 = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);
                let height_buf = if (val2 & 0x80000000) != 0 {
                    let mut h_buf: [u8; 4] = [0; 4];
                    if let Err(e) = self.socket.read_exact(&mut h_buf).await {
                        log::error!("[Controller] Failed to read video height: {}", e);
                        return;
                    }
                    Some(h_buf)
                } else {
                    None
                };

                let (codec_id, width, height) = match parse_video_metadata_buf(&buf, height_buf.as_ref()) {
                    Ok(parsed) => parsed,
                    Err(e) => {
                        log::error!(
                            "[Controller] {}: {}",
                            t!("scrcpy.invalidVideoCodec"),
                            e
                        );
                        return;
                    }
                };

                log::info!("[Controller] Video dimensions: {}x{}", width, height);
                log::info!("[Controller] {}: {:?}", t!("scrcpy.videoCodec"), codec_id);

                let video_decoder = match VideoDecoder::new(codec_id, width, height) {
                    Ok(d) => d,
                    Err(e) => {
                        log::error!("[Controller] Failed to initialize video decoder: {}", e);
                        return;
                    }
                };

                // Send stream-info once so the HUD can show ground-truth status
                // (hw_active reflects whether VAAPI actually initialized, not just
                //  whether hw_decode is enabled in settings).
                let _ = v_tx.send(VideoMsg::StreamInfo {
                    codec: format!("{}", codec_id),
                    hw_active: video_decoder.hw_active(),
                    width,
                    height,
                });

                let scid_str = scid.to_string();
                let m_tx_copy = m_tx.clone();
                tokio::spawn(async move {
                    ControlledDevice::update_device_size(scid_str, (width, height)).await;
                    mask_win_move_helper(width, height, &m_tx_copy).await;
                });
                video_decoder
            }
        };

        // read video packets
        let mut frame_count: u64 = 0;
        loop {
            match read_media_packet(&mut self.socket).await {
                Ok(mut packet) => {
                    frame_count += 1;
                    let is_config = packet.pts().is_none();

                    if video_decoder.must_merge_config {
                        // merge config packet if needed
                        video_decoder.packet_merger.merge(&mut packet);
                    }

                    // no send config packet
                    if packet.pts().is_some() {
                    let packet_arrival_us = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_micros() as u64;
                        let decode_start = std::time::Instant::now();
                        if let Err(e) = video_decoder.decoder.send_packet(&mut packet) {
                            log::warn!("[Controller] Failed to send packet to decoder: {} (frame_count={})", e, frame_count);
                            continue;
                        }
                        let mut decoded = frame::Video::empty();
                        match video_decoder.decoder.receive_frame(&mut decoded) {
                            Ok(_) => {}
                            Err(e) => {
                                log::warn!("[Controller] No frame yet from decoder: {} (frame_count={}, decoder_size={}x{})", e, frame_count, video_decoder.width, video_decoder.height);
                                continue;
                            }
                        }
                        // update size after decoding video packet
                        let decoded_w = decoded.width();
                        let decoded_h = decoded.height();
                        // save size before update() so we can detect a real change
                        // (update() also returns true on first frame because scaler is None,
                        //  which would duplicate the WinMove already sent from metadata)
                        let pre_w = video_decoder.width;
                        let pre_h = video_decoder.height;
                        if video_decoder.update(&decoded) {
                            let new_w = video_decoder.width;
                            let new_h = video_decoder.height;
                            if new_w != pre_w || new_h != pre_h {
                                log::info!("[Controller] Video size changed: {}x{} -> {}x{} (frame_count={})",
                                    pre_w, pre_h, new_w, new_h, frame_count);
                                let scid_str = scid.to_string();
                                let m_tx_copy = m_tx.clone();
                                tokio::spawn(async move {
                                    ControlledDevice::update_device_size(scid_str, (new_w, new_h)).await;
                                    mask_win_move_helper(new_w, new_h, &m_tx_copy).await;
                                });
                            } else {
                                log::info!("[Controller] Scaler initialized {}x{} (frame_count={})",
                                    new_w, new_h, frame_count);
                                _ = decoded_w; // suppress unused warning
                                _ = decoded_h;
                            }
                        }

                        let frame_size = video_decoder.frame_size;
                        let decoder_width = video_decoder.width;
                        let decoder_height = video_decoder.height;
                        let rgb_frame = match video_decoder.convert_to_rgba(&decoded) {
                            Ok(f) => f,
                            Err(e) => {
                                log::error!("[Controller] Convert to RGBA failed: {}", e);
                                continue;
                            }
                        };
                        let decode_elapsed_ms = decode_start.elapsed().as_secs_f32() * 1000.0;
                        let mut buf = match recycle_rx.try_recv() {
                            Ok(mut b) => {
                                b.resize(frame_size, 0);
                                b
                            }
                            Err(_) => {
                                let mut b = Vec::with_capacity(frame_size);
                                b.resize(frame_size, 0);
                                b
                            }
                        };

                        // Handle stride padding: FFmpeg may pad each row for
                        // alignment (stride > width*4), especially with VAAPI.
                        // Copy row-by-row to strip the padding when needed.
                        let stride = rgb_frame.stride(0) as usize;
                        let row_bytes = decoder_width as usize * 4;
                        let src_data = rgb_frame.data(0);
                        if stride == row_bytes {
                            buf.copy_from_slice(src_data);
                        } else {
                            for y in 0..decoder_height as usize {
                                let src_offset = y * stride;
                                let dst_offset = y * row_bytes;
                                buf[dst_offset..dst_offset + row_bytes]
                                    .copy_from_slice(&src_data[src_offset..src_offset + row_bytes]);
                            }
                        }

                        let data = buf;
                        match v_tx.try_send(VideoMsg::Data {
                            data,
                            width: decoder_width,
                            height: decoder_height,
                            decode_time_ms: decode_elapsed_ms,
                            timestamp_us: packet_arrival_us,
                        }) {
                            Ok(_) => {}
                            Err(crossbeam_channel::TrySendError::Full(VideoMsg::Data { data, .. })) => {
                                // Channel is full, recycle buffer and drop frame to prevent latency and OOM
                                let _ = recycle_tx.send(data);
                            }
                            Err(e) => {
                                log::warn!("[Controller] Video receiver dropped, stopping video loop: {}", e);
                                break;
                            }
                        }
                    } else {
                        log::info!("[Controller] Config packet received (frame_count={}, is_config={})", frame_count, is_config);
                    }
                }
                Err(e) => {
                    log::error!("[Controller] Video read error: {} (frame_count={})", e, frame_count);
                    break;
                }
            }
        }
    }

    pub async fn handle_video(
        mut self,
        token: CancellationToken,
        v_tx: crossbeam_channel::Sender<VideoMsg>,
        m_tx: crossbeam_channel::Sender<(MaskCommand, oneshot::Sender<Result<String, String>>)>,
        meta_flag: bool,
        scid: &str,
        recycle_rx: crossbeam_channel::Receiver<Vec<u8>>,
        recycle_tx: crossbeam_channel::Sender<Vec<u8>>,
    ) {
        log::info!("[Controller] {}", t!("scrcpy.handleVideoConnection"));
        if meta_flag {
            if let Err(e) = self.read_device_metadata(scid.to_string()).await {
                log::error!("[Controller] {}", e);
                token.cancel();
                return;
            }
        }

        let finnal_token = token.clone();

        tokio::select! {
            _ = token.cancelled()=>{
                log::info!("[Controller] {}", t!("scrcpy.videoConnectionReaderCancelled"));
            }
            _ = self.video_handler(v_tx.clone(), m_tx, scid, recycle_rx, recycle_tx)=>{
                log::error!("[Controller] {}", t!("scrcpy.videoReadShutdownUnexpectedly"));
                finnal_token.cancel();
            }
        }
        let _ = v_tx.send(VideoMsg::Close);
        log::info!("[Controller] {}", t!("scrcpy.videoConnectionClosed"));
        self.socket.shutdown().await.unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_device_name() {
        // Normal string
        assert_eq!(parse_device_name(b"MyDevice"), "MyDevice");

        // String with null bytes
        assert_eq!(parse_device_name(b"MyDevice\0\0\0"), "MyDevice");

        // String with null bytes in middle (will parse everything up to trailing nulls, char::from(0))
        assert_eq!(parse_device_name(b"My\0Device\0"), "My\0Device");

        // Invalid UTF-8
        assert_eq!(parse_device_name(&[0, 159, 146, 150]), "INVALID_NAME");
    }

    #[test]
    fn test_parse_video_metadata_buf() {
        // 1. Test Old format (12 bytes)
        // Codec H264 (0x68323634), width 1920 (0x00000780), height 1080 (0x00000438)
        let mut buf_h264_old = [0u8; 12];
        buf_h264_old[0..4].copy_from_slice(&SC_CODEC_ID_H264.to_be_bytes());
        buf_h264_old[4..8].copy_from_slice(&1920u32.to_be_bytes());
        buf_h264_old[8..12].copy_from_slice(&1080u32.to_be_bytes());

        let res = parse_video_metadata_buf(&buf_h264_old, None).unwrap();
        assert_eq!(res.0, VideoCodec::H264);
        assert_eq!(res.1, 1920);
        assert_eq!(res.2, 1080);

        // Codec H265 (0x68323635), width 2560 (0x00000a00), height 1440 (0x000005a0)
        let mut buf_h265_old = [0u8; 12];
        buf_h265_old[0..4].copy_from_slice(&SC_CODEC_ID_H265.to_be_bytes());
        buf_h265_old[4..8].copy_from_slice(&2560u32.to_be_bytes());
        buf_h265_old[8..12].copy_from_slice(&1440u32.to_be_bytes());

        let res = parse_video_metadata_buf(&buf_h265_old, None).unwrap();
        assert_eq!(res.0, VideoCodec::H265);
        assert_eq!(res.1, 2560);
        assert_eq!(res.2, 1440);

        // 2. Test new v3.0+ SessionMeta format (16 bytes)
        // val2 has flags: e.g. 0x80000000
        // val3 has width: 1920
        // height_buf has height: 1080
        let mut buf_h265_new = [0u8; 12];
        buf_h265_new[0..4].copy_from_slice(&SC_CODEC_ID_H265.to_be_bytes());
        buf_h265_new[4..8].copy_from_slice(&0x80000000u32.to_be_bytes());
        buf_h265_new[8..12].copy_from_slice(&1920u32.to_be_bytes());
        let height_buf = 1080u32.to_be_bytes();

        let res = parse_video_metadata_buf(&buf_h265_new, Some(&height_buf)).unwrap();
        assert_eq!(res.0, VideoCodec::H265);
        assert_eq!(res.1, 1920);
        assert_eq!(res.2, 1080);

        // Height buffer missing error case
        let res_err = parse_video_metadata_buf(&buf_h265_new, None);
        assert!(res_err.is_err());
    }
}

