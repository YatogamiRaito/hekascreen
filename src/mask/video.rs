use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};

use crate::scrcpy::media::VideoMsg;
use crate::{mask::ui::basic::BORDER_THICKNESS, utils::{ChannelReceiverV, VideoBufferRecycler, LiveDiagnostics}};

#[derive(Resource, Default)]
pub struct VideoAttributes {
    width: u32,
    height: u32,
    image_handle: Option<Handle<Image>>,
}

impl VideoAttributes {}

#[derive(Component)]
pub struct VideoPlayer;

pub fn init_video(mut commands: Commands) {
    commands.spawn((
        Node {
            width: Val::Percent(100.),
            height: Val::Percent(100.),
            padding: UiRect::all(Val::Px(BORDER_THICKNESS)),
            box_sizing: BoxSizing::BorderBox,
            ..default()
        },
        ZIndex(-1),
        BackgroundColor(Color::NONE),
        ImageNode::default(),
        VideoPlayer,
    ));
}

pub fn handle_video_msg(
    mut commands: Commands,
    v_rx: Res<ChannelReceiverV>,
    recycler: Res<VideoBufferRecycler>,
    mut images: ResMut<Assets<Image>>,
    mut live_diagnostics: ResMut<LiveDiagnostics>,
    mut video_attr: Local<VideoAttributes>,
    video_node: Query<(Entity, &ImageNode), With<VideoPlayer>>,
) {
    let mut last_data_msg = None;
    for msg in v_rx.0.try_iter() {
        match msg {
            VideoMsg::Data { data, width, height, decode_time_ms, timestamp_us } => {
                if let Some(prev) = last_data_msg.take() {
                    if let VideoMsg::Data { data: old_data, .. } = prev {
                        let _ = recycler.0.send(old_data);
                    }
                }
                last_data_msg = Some(VideoMsg::Data { data, width, height, decode_time_ms, timestamp_us });
            }
            VideoMsg::StreamInfo { codec, hw_active, width, height } => {
                live_diagnostics.video_codec = Some(codec);
                live_diagnostics.hw_decode_active = hw_active;
                live_diagnostics.video_width = width;
                live_diagnostics.video_height = height;
            }
            VideoMsg::ScriptError { error } => {
                live_diagnostics.last_script_error = Some(error);
            }
            VideoMsg::ScriptClearError => {
                live_diagnostics.last_script_error = None;
            }
            VideoMsg::Close => {
                // Clear stream-level diagnostics when the connection drops
                live_diagnostics.video_codec = None;
                live_diagnostics.hw_decode_active = false;
                live_diagnostics.video_width = 0;
                live_diagnostics.video_height = 0;
                // Also reset per-frame stats so stale values don't linger
                live_diagnostics.video_fps = 0.0;
                live_diagnostics.video_frame_count = 0;
                live_diagnostics.video_fps_window_start = std::time::Instant::now();
                live_diagnostics.last_input_latency_ms = None;
                if let Some(image_handle) = video_attr.image_handle.take() {
                    if let Some(image) = images.get_mut(&image_handle) {
                        if let Some(old_data) = image.data.take() {
                            let _ = recycler.0.send(old_data);
                        }
                    }
                    images.remove(&image_handle);
                }
            }
        }
    }

    if let Some(VideoMsg::Data {
        data,
        width,
        height,
        decode_time_ms,
        timestamp_us,
    }) = last_data_msg {
        let now_us = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros() as u64;
        let queue_delay_ms = if now_us > timestamp_us {
            (now_us - timestamp_us) as f32 / 1000.0
        } else {
            0.0
        };
        
        let last_input_us = crate::utils::LAST_INPUT_TIME_MICROS.load(std::sync::atomic::Ordering::Relaxed);
        let input_latency_ms = if last_input_us > 0 && now_us > last_input_us {
            let diff = (now_us - last_input_us) as f32 / 1000.0;
            if diff < 10000.0 {
                Some(diff)
            } else {
                None
            }
        } else {
            None
        };
        
        live_diagnostics.decode_time_ms = decode_time_ms;
        live_diagnostics.queue_delay_ms = queue_delay_ms;
        if input_latency_ms.is_some() {
            live_diagnostics.last_input_latency_ms = input_latency_ms;
        }

        // Count real video FPS over a 1-second sliding window
        live_diagnostics.video_frame_count += 1;
        let elapsed = live_diagnostics.video_fps_window_start.elapsed().as_secs_f32();
        if elapsed >= 1.0 {
            live_diagnostics.video_fps = live_diagnostics.video_frame_count as f32 / elapsed;
            live_diagnostics.video_frame_count = 0;
            live_diagnostics.video_fps_window_start = std::time::Instant::now();
        }
        let size_changed = video_attr.width != width
            || video_attr.height != height
            || video_attr.image_handle.is_none();
        if size_changed {
            // Despawn old video player entity if it exists
            for (entity, _) in video_node.iter() {
                commands.entity(entity).despawn();
            }

            // Recycle old image data before creation
            if let Some(old_handle) = video_attr.image_handle.take() {
                if let Some(old_image) = images.get_mut(&old_handle) {
                    if let Some(old_data) = old_image.data.take() {
                        let _ = recycler.0.send(old_data);
                    }
                }
                images.remove(&old_handle);
            }

            // Create new Image asset with correct size and data directly
            let mut image = Image::new_fill(
                Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                TextureDimension::D2,
                &[0, 0, 0, 0],
                TextureFormat::Rgba8UnormSrgb,
                RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
            );
            image.texture_descriptor.usage =
                TextureUsages::COPY_DST | TextureUsages::TEXTURE_BINDING;
            image.data = Some(data);
            let image_handle = images.add(image);

            // Spawn new video player entity
            commands.spawn((
                Node {
                    width: Val::Percent(100.),
                    height: Val::Percent(100.),
                    padding: UiRect::all(Val::Px(BORDER_THICKNESS)),
                    box_sizing: BoxSizing::BorderBox,
                    ..default()
                },
                ZIndex(-1),
                BackgroundColor(Color::NONE),
                ImageNode::from(image_handle.clone()),
                VideoPlayer,
            ));

            video_attr.image_handle = Some(image_handle);
            video_attr.width = width;
            video_attr.height = height;
        } else {
            // Update existing image data
            if let Some(image_handle) = &video_attr.image_handle {
                if let Some(image) = images.get_mut(image_handle) {
                    if let Some(old_data) = image.data.take() {
                        let _ = recycler.0.send(old_data);
                    }
                    image.data = Some(data);
                }
            }
        }
    }
}

#[derive(Component)]
pub struct DiagnosticsHudText;



pub fn update_diagnostics_hud(
    mut commands: Commands,
    live_diagnostics: Res<LiveDiagnostics>,
    mut text_query: Query<(&mut Text, &mut TextColor), With<DiagnosticsHudText>>,
    hud_query: Query<Entity, With<DiagnosticsHudText>>,
) {
    let show = crate::config::LocalConfig::get().show_diagnostics || live_diagnostics.last_script_error.is_some();
    
    if !show {
        for entity in hud_query.iter() {
            commands.entity(entity).despawn();
        }
        return;
    }
    
    
    if hud_query.iter().next().is_none() {
        commands.spawn((
            Text::new(""),
            TextFont {
                font_size: 14.,
                ..default()
            },
            TextColor(if live_diagnostics.last_script_error.is_some() {
                Color::srgba_u8(255, 50, 50, 230)
            } else {
                Color::srgba_u8(0, 255, 0, 230)
            }),
            DiagnosticsHudText,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(10.),
                top: Val::Px(10.),
                padding: UiRect::all(Val::Px(5.)),
                border_radius: BorderRadius::all(Val::Px(4.)),
                ..default()
            },
            BackgroundColor(Color::srgba_u8(0, 0, 0, 150)),
            ZIndex(100),
        ));
        return;
    }
    
    for (mut text, mut color) in text_query.iter_mut() {
        if live_diagnostics.last_script_error.is_some() {
            color.0 = Color::srgba_u8(255, 50, 50, 230);
        } else {
            color.0 = Color::srgba_u8(0, 255, 0, 230);
        }

        let input_latency_str = match live_diagnostics.last_input_latency_ms {
            Some(lat) => format!("{:.1} ms", lat),
            None => "N/A".to_string(),
        };

        // ── Perf line ────────────────────────────────────────────────
        let video_fps_str = if live_diagnostics.video_fps > 0.0 {
            format!("{:.1}", live_diagnostics.video_fps)
        } else {
            "--".to_string()
        };
        let mut lines = vec![
            format!(
                "FPS: {}  Decode: {:.1} ms  Queue: {:.1} ms",
                video_fps_str,
                live_diagnostics.decode_time_ms,
                live_diagnostics.queue_delay_ms,
            ),
            format!("Input Lag: {}", input_latency_str),
        ];

        // ── Stream info (only when actually connected) ────────────────
        if let Some(ref codec) = live_diagnostics.video_codec {
            lines.push("---------------------".to_string());

            // Codec | HW/SW | WxH
            let decode_mode = if live_diagnostics.hw_decode_active {
                "VAAPI"
            } else {
                "SW"
            };
            lines.push(format!(
                "{}  {}  {}×{}",
                codec, decode_mode,
                live_diagnostics.video_width,
                live_diagnostics.video_height,
            ));

            // Active codec options — read from config, but only shown here
            // because the connection is alive (options were accepted by encoder).
            let cfg = crate::config::LocalConfig::get();
            let mut opts: Vec<&str> = Vec::new();
            if cfg.video_low_latency       { opts.push("latency=0"); }
            if cfg.video_realtime_priority { opts.push("priority=0"); }
            if cfg.video_qcom_low_latency  { opts.push("qcom-ll"); }
            if cfg.video_intra_refresh     { opts.push("intra-refresh"); }
            if !cfg.video_codec_options.is_empty() { opts.push("custom"); }

            if !opts.is_empty() {
                lines.push(format!("LL: {}", opts.join("  ")));
            }
        }

        if let Some(ref err) = live_diagnostics.last_script_error {
            lines.push("---------------------".to_string());
            lines.push(format!("SCRIPT ERROR:\n{}", err));
        }

        text.0 = lines.join("\n");
    }
}
