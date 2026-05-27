use std::{
    fs::{File, create_dir_all},
    io::Write,
    sync::RwLock,
};

use crate::{scrcpy::media::VideoCodec, utils::relate_to_data_path};
use once_cell::sync::Lazy;
use paste::paste;
use rust_i18n::t;
use serde::{Deserialize, Serialize};
use serde_json::to_string_pretty;

static CONFIG: Lazy<RwLock<LocalConfig>> = Lazy::new(|| RwLock::default());

// TODO 单独写外部脚本来捕获特定窗口，发送Post消息来设置蒙版相关配置（宽度>=高度则设置横屏相关配置，否则设置竖屏）

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LocalConfig {
    // port
    pub web_port: u16,
    pub controller_port: u16,
    // adb
    pub adb_path: String,
    // mask
    pub always_on_top: bool,
    pub vertical_mask_height: u32,
    pub horizontal_mask_width: u32,
    pub vertical_position: (i32, i32),
    pub horizontal_position: (i32, i32),
    // mapping
    pub active_mapping_file: String,
    pub mapping_label_opacity: f32,
    // language
    pub language: String,
    // clipboard sync
    pub clipboard_sync: bool,
    // video config
    pub video_codec: VideoCodec,
    pub video_bit_rate: u32,
    pub video_max_size: u32,
    pub video_max_fps: u32,
    pub present_mode: String,
    pub video_codec_options: String,
    pub video_low_latency: bool,
    pub video_realtime_priority: bool,
    pub video_qcom_low_latency: bool,
    pub video_intra_refresh: bool,
    pub show_diagnostics: bool,
    pub config_version: u32,
    pub hw_decode: bool,
}

fn get_system_language() -> String {
    if let Ok(lang) = std::env::var("LANG") {
        if lang.starts_with("zh") {
            "zh-CN".to_string()
        } else if lang.starts_with("tr") {
            "tr-TR".to_string()
        } else {
            "en-US".to_string()
        }
    } else {
        "en-US".to_string()
    }
}

impl Default for LocalConfig {
    fn default() -> Self {
        Self {
            adb_path: "adb".to_string(),
            web_port: 27799,
            controller_port: 27798,
            always_on_top: true,
            vertical_mask_height: 720,
            horizontal_mask_width: 1280,
            vertical_position: (100, 100),
            horizontal_position: (100, 100),
            active_mapping_file: "default.json".to_string(),
            mapping_label_opacity: 0.3,
            language: get_system_language(),
            clipboard_sync: true,
            video_codec: VideoCodec::H265,
            video_bit_rate: 16_000000, // 16M
            video_max_size: 1920,      // default 1920
            video_max_fps: 60,         // default 60
            present_mode: "AutoVsync".to_string(),
            video_codec_options: "".to_string(),
            video_low_latency: true,
            video_realtime_priority: true,
            video_qcom_low_latency: false, // causes stack corruption on Android 16 (OMX.qcom.video.encoder.hevc)
            video_intra_refresh: false,
            show_diagnostics: true,
            config_version: 1,
            hw_decode: false,
        }
    }
}

macro_rules! define_setter {
    ($(($field:ident, $typ:ty)),* $(,)?) => {
        paste! {
            $(
                pub fn [<set_ $field>] (value: $typ) {
                    CONFIG.write().unwrap().$field = value;
                    Self::save().unwrap();
                }
            )*
        }
    };
}

impl LocalConfig {
    pub fn save() -> Result<(), String> {
        let config_json = to_string_pretty(&Self::get())
            .map_err(|e| format!("{}: {}", t!("localConfig.serializeConfigError"), e))?;

        let path = relate_to_data_path(["config.json"]);
        if let Some(parent) = path.parent() {
            create_dir_all(parent)
                .map_err(|e| format!("{}: {}", t!("localConfig.createConfigDirError"), e))?;
        }
        let mut file = File::create(path)
            .map_err(|e| format!("{}: {}", t!("localConfig.createConfigError"), e))?;
        file.write_all(config_json.as_bytes())
            .map_err(|e| format!("{}: {}", t!("localConfig.writeConfigError"), e))?;
        Ok(())
    }

    pub fn load() -> Result<(), String> {
        let path = relate_to_data_path(["config.json"]);
        let config_string = std::fs::read_to_string(&path).map_err(|e| {
            format!(
                "{} {}: {}",
                t!("localConfig.readConfigError"),
                path.to_str().unwrap(),
                e
            )
        })?;
        let mut config: LocalConfig = serde_json::from_str(&config_string)
            .map_err(|e| format!("{}: {}", t!("localConfig.serializeConfigError"), e))?;

        let mut migrated = false;
        if config.video_codec == VideoCodec::H264 {
            config.video_codec = VideoCodec::H265;
            eprintln!("[HekaScreen] Config migration: video_codec H264 → H265 (Extended Profile causes decode errors)");
            migrated = true;
        }

        if config.video_max_size == 0 {
            config.video_max_size = 1920;
            eprintln!("[HekaScreen] Config migration: video_max_size 0 → 1920 (Optimal for S20 FE 5G / low-latency)");
            migrated = true;
        }

        if config.video_max_fps == 0 {
            config.video_max_fps = 60;
            eprintln!("[HekaScreen] Config migration: video_max_fps 0 → 60 (Standard gaming FPS limit)");
            migrated = true;
        }

        if config.video_bit_rate == 8_000000 {
            config.video_bit_rate = 16_000000;
            eprintln!("[HekaScreen] Config migration: video_bit_rate 8M → 16M (Optimal H.265 gaming bitrate)");
            migrated = true;
        }

        if config.config_version == 0 {
            config.config_version = 1;
            migrated = true;
        }

        // v1 → v2: disable vendor.qti-ext-enc-low-latency.enable
        // This option triggers -fstack-protector stack corruption in
        // OMX.qcom.video.encoder.hevc/.avc on Android 16 (SM-S916B).
        // Force it off regardless of what the stored config says.
        if config.config_version < 2 {
            if config.video_qcom_low_latency {
                config.video_qcom_low_latency = false;
                eprintln!("[HekaScreen] Config migration v1→v2: video_qcom_low_latency forced to false (causes stack corruption on Android 16 Qualcomm encoder)");
            }
            config.config_version = 2;
            migrated = true;
        }

        *CONFIG.write().unwrap() = config;

        if migrated {
            if let Err(e) = Self::save() {
                eprintln!("[HekaScreen] Failed to save migrated config: {}", e);
            }
        }

        Ok(())
    }

    pub fn get() -> LocalConfig {
        CONFIG.read().unwrap().clone()
    }

    pub fn get_clipboard_sync() -> bool {
        CONFIG.read().unwrap().clipboard_sync
    }

    define_setter!(
        (web_port, u16),
        (controller_port, u16),
        (adb_path, String),
        (always_on_top, bool),
        (vertical_mask_height, u32),
        (horizontal_mask_width, u32),
        (vertical_position, (i32, i32)),
        (horizontal_position, (i32, i32)),
        (active_mapping_file, String),
        (mapping_label_opacity, f32),
        (language, String),
        (clipboard_sync, bool),
        (video_codec, VideoCodec),
        (video_bit_rate, u32),
        (video_max_size, u32),
        (video_max_fps, u32),
        (present_mode, String),
        (video_codec_options, String),
        (video_low_latency, bool),
        (video_realtime_priority, bool),
        (video_qcom_low_latency, bool),
        (video_intra_refresh, bool),
        (show_diagnostics, bool),
        (config_version, u32),
        (hw_decode, bool),
    );
}
