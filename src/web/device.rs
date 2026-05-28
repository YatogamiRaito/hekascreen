use std::time::Duration;
use std::collections::HashMap;

use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
    routing::{get, post},
};
use rand::Rng;
use rust_i18n::t;
use serde::Deserialize;
use serde_json::json;
use tokio::{
    sync::{broadcast, mpsc::UnboundedSender, oneshot},
    time::sleep,
};

use crate::{
    config::LocalConfig,
    mask::mask_command::MaskCommand,
    scrcpy::{
        adb::{Adb, Device},
        constant::{KeyEventAction, Keycode, MetaState},
        control_msg::ScrcpyControlMsg,
        controller::ControllerCommand,
    },
    utils::{relate_to_root_path, share::ControlledDevice},
    web::{JsonResponse, WebServerError},
};

#[derive(Debug, Clone)]
pub struct AppStateDevice {
    cs_tx: broadcast::Sender<ScrcpyControlMsg>,
    d_tx: UnboundedSender<ControllerCommand>,
    m_tx: crossbeam_channel::Sender<(MaskCommand, oneshot::Sender<Result<String, String>>)>,
}

pub fn routers(
    cs_tx: broadcast::Sender<ScrcpyControlMsg>,
    d_tx: UnboundedSender<ControllerCommand>,
    m_tx: crossbeam_channel::Sender<(MaskCommand, oneshot::Sender<Result<String, String>>)>,
) -> Router {
    Router::new()
        .route("/device_list", get(device_list))
        .route("/control_device", post(control_device))
        .route("/decontrol_device", post(decontrol_device))
        .route("/reconnect_device", post(reconnect_device))
        .route("/adb_connect", post(adb_connect))
        .route("/adb_pair", post(adb_pair))
        .route("/adb_screenshot", post(adb_screenshot))
        .route("/control/set_display_power", post(set_display_power))
        .route("/control/send_key", post(send_key))
        .route("/control/eval_script", post(eval_script))
        .with_state(AppStateDevice { cs_tx, d_tx, m_tx })
}

async fn device_list() -> Result<JsonResponse, WebServerError> {
    let controlled_devices = ControlledDevice::get_device_list().await;
    let config = LocalConfig::get();
    let all_devices = Adb::new(config.adb_path)
        .devices()
        .map_err(|e| WebServerError::internal_error(e))?;

    Ok(JsonResponse::success(
        t!("web.device.deviceListObtained"),
        Some(json!({
            "controlled_devices": controlled_devices,
            "adb_devices": all_devices,
        })),
    ))
}

fn gen_scid() -> String {
    let mut rng = rand::rng();
    let suffix: String = (0..6)
        .map(|_| rng.random_range(1..=9).to_string())
        .collect();
    format!("10{}", suffix) // ensure 8 digits(HEX) and less than MAX_INT32
}

use once_cell::sync::Lazy;
use std::collections::HashSet;
use tokio::sync::Mutex;

static CONNECTING_DEVICES: Lazy<Mutex<HashSet<String>>> = Lazy::new(|| Mutex::new(HashSet::new()));
static ACTIVE_PROCESSES: Lazy<Mutex<HashMap<String, tokio::sync::oneshot::Sender<()>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

struct ConnectingGuard {
    device_id: String,
}

impl Drop for ConnectingGuard {
    fn drop(&mut self) {
        let device_id = self.device_id.clone();
        tokio::spawn(async move {
            let mut connecting = CONNECTING_DEVICES.lock().await;
            connecting.remove(&device_id);
        });
    }
}

#[derive(Deserialize)]
struct PostDataControlDevice {
    device_id: String,
    display_id: i32,
    video: bool,
}

async fn _control_device(
    device_id: &str,
    display_id: i32,
    video: bool,
    d_tx: &UnboundedSender<ControllerCommand>,
) -> Result<JsonResponse, WebServerError> {
    let device_id = device_id.to_string();

    // Step 1: Check CONTROLLED_DEVICES without holding any lock.
    // This avoids holding a tokio Mutex across an .await point and
    // serves as a fast early-exit for the common case.
    let device_list = ControlledDevice::get_device_list().await;
    if device_list
        .iter()
        .any(|device| device.device_id == device_id)
    {
        return Err(WebServerError(
            400,
            format!("{}: {}", t!("web.device.alreadyControlled"), device_id),
        ));
    }

    // Step 2: Atomically check-and-insert in CONNECTING_DEVICES.
    // No .await inside this block — the lock is never held across a yield point.
    // Any concurrent _control_device() call for the same device_id will be
    // rejected here, closing the TOCTOU window between step 1 and add_device().
    // add_device() (share.rs) also performs a dedup check as a final safety net.
    {
        let mut connecting = CONNECTING_DEVICES.lock().await;
        if connecting.contains(&device_id) {
            return Err(WebServerError(
                400,
                format!("{}: {}", t!("web.device.alreadyControlled"), device_id),
            ));
        }
        connecting.insert(device_id.clone());
    }

    let _guard = ConnectingGuard { device_id: device_id.clone() };
    let local_config = LocalConfig::get();


    // prepare for scrcpy app
    let scid = gen_scid();
    let version = std::fs::read_dir(relate_to_root_path(["assets"]))
        .ok()
        .and_then(|read_dir| {
            read_dir
                .filter_map(|entry| entry.ok())
                .filter_map(|entry| {
                    let file_name = entry.file_name().into_string().ok()?;
                    let v_str = file_name.strip_prefix("scrcpy-mask-server-v")?.to_string();
                    let normalized = if v_str.split('.').count() == 2 {
                        format!("{}.0", v_str)
                    } else {
                        v_str.clone()
                    };
                    let parsed = semver::Version::parse(&normalized).ok()?;
                    Some((parsed, v_str))
                })
                .max_by(|a, b| a.0.cmp(&b.0))
                .map(|(_, original)| original)
        })
        .unwrap_or_else(|| "4.0".to_string());
    let scrcpy_path = relate_to_root_path(["assets", &format!("scrcpy-mask-server-v{}", version)]);
    Device::push(
        &device_id,
        scrcpy_path.to_str().unwrap(),
        "/data/local/tmp/scrcpy-server.jar",
    )
    .map_err(|e| WebServerError(500, e))?;
    log::info!("[WebServe] {}", t!("web.device.pushScrcpyServerSuccess"));

    let remote = format!("localabstract:scrcpy_{}", scid);
    let local = format!("tcp:{}", local_config.controller_port);
    Device::reverse(&device_id, &remote, &local).map_err(|e| WebServerError(500, e))?;
    log::info!(
        "[WebServe] {}",
        t!("web.device.reverseSuccess", remote => remote, local => local)
    );

    let mut args = [
        "CLASSPATH=/data/local/tmp/scrcpy-server.jar",
        "app_process",
        "/",
        "com.genymobile.scrcpy.Server",
    ]
    .iter_mut()
    .map(|arg| arg.to_string())
    .collect::<Vec<String>>();

    args.push(version.to_string());
    args.push(format!("scid={}", scid));
    args.push(format!("video={}", video));
    args.push(format!("display_id={}", display_id));
    args.push("audio=false".to_string());

    // create device
    let main = ControlledDevice::get_device_list().await.len() == 0;
    let mut socket_id: Vec<String> = Vec::new();
    let mut commands: Vec<ControllerCommand> = Vec::new();
    if main {
        let mut meta_flag = true;
        if video {
            socket_id.push("main_video".to_string());
            commands.push(ControllerCommand::ConnectMainVideo(scid.clone(), meta_flag));
            if meta_flag {
                meta_flag = false;
            }

            // video shell args
            args.push(format!("video_codec={}", local_config.video_codec));
            args.push(format!("video_bit_rate={}", local_config.video_bit_rate));
            if local_config.video_max_size > 0 {
                args.push(format!("max_size={}", local_config.video_max_size));
            }
            if local_config.video_max_fps > 0 {
                args.push(format!("max_fps={}", local_config.video_max_fps));
            }
            if local_config.video_i_frame_interval > 0 {
                args.push(format!("video_i_frame_interval={}", local_config.video_i_frame_interval));
            }
            let mut codec_opts = Vec::new();
            if local_config.video_low_latency {
                codec_opts.push("latency=0".to_string());
            }
            if local_config.video_realtime_priority {
                codec_opts.push("priority=0".to_string());
            }
            if local_config.video_qcom_low_latency {
                codec_opts.push("vendor.qti-ext-enc-low-latency.enable=1".to_string());
            }
            if local_config.video_intra_refresh {
                codec_opts.push("intra-refresh-period=60".to_string());
            }
            if !local_config.video_codec_options.is_empty() {
                codec_opts.push(local_config.video_codec_options.clone());
            }
            if !codec_opts.is_empty() {
                args.push(format!("video_codec_options={}", codec_opts.join(",")));
            }
        }
        socket_id.push("main_control".to_string());
        commands.push(ControllerCommand::ConnectMainControl(
            scid.clone(),
            meta_flag,
        ));
    } else {
        socket_id.push(format!("sub_control_{}", scid));
        commands.push(ControllerCommand::ConnectSubControl(scid.clone()));
    }

    ControlledDevice::add_device(device_id.clone(), scid.clone(), main, socket_id).await;
    // send command to controller server
    for cmd in commands {
        if let Err(e) = d_tx.send(cmd) {
            log::error!("[WebServe] Failed to send connect command to controller: {}", e);
        }
    }

    // run scrcpy app
    sleep(Duration::from_millis(500)).await;
    log::info!("[WebServe] {}", t!("web.device.startingScrcpyApp"));

    let mut child = match tokio::process::Command::new(&local_config.adb_path)
        .arg("-s")
        .arg(&device_id)
        .arg("shell")
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            log::error!("[WebServe] Failed to start adb shell: {}", e);
            return Err(WebServerError(500, format!("Failed to start adb shell: {}", e)));
        }
    };

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let (kill_tx, kill_rx) = tokio::sync::oneshot::channel::<()>();
    {
        let mut processes = ACTIVE_PROCESSES.lock().await;
        processes.insert(device_id.clone(), kill_tx);
    }

    use tokio::io::{AsyncBufReadExt, BufReader};
    let device_id_log = device_id.clone();
    tokio::spawn(async move {
        let mut reader = BufReader::new(stdout).lines();
        while let Ok(Some(l)) = reader.next_line().await {
            log::info!("[Adb stdout {}] {}", device_id_log, l);
        }
    });

    let device_id_err = device_id.clone();
    tokio::spawn(async move {
        let mut reader = BufReader::new(stderr).lines();
        while let Ok(Some(l)) = reader.next_line().await {
            log::error!("[Adb stderr {}] {}", device_id_err, l);
        }
    });

    let scid_copy = scid.clone();
    let device_id_wait = device_id.clone();
    tokio::spawn(async move {
        tokio::select! {
            status = child.wait() => {
                log::info!("[WebServe] Child exited on its own: {:?}", status);
            }
            _ = kill_rx => {
                log::info!("[WebServe] Kill signal received, killing child process for device: {}", device_id_wait);
                let _ = child.kill().await;
                // Wait with a timeout of 2 seconds for it to exit
                if let Err(_) = tokio::time::timeout(std::time::Duration::from_secs(2), child.wait()).await {
                    log::warn!("[WebServe] Child process did not exit after kill signal, ignoring");
                }
            }
        }

        log::info!("[WebServe] {}", t!("web.device.removingDeviceAfterExit"));
        ControlledDevice::remove_device(&scid_copy).await;

        let mut processes = ACTIVE_PROCESSES.lock().await;
        processes.remove(&device_id_wait);
    });

    Ok(JsonResponse::success(
        t!("web.device.tryStartingScrcpy"),
        Some(json!({"scid": scid, "device_id": device_id})),
    ))
}

async fn control_device(
    State(state): State<AppStateDevice>,
    Json(payload): Json<PostDataControlDevice>,
) -> Result<JsonResponse, WebServerError> {
    let device_id = payload.device_id;
    let video = payload.video;
    let display_id = payload.display_id;

    _control_device(&device_id, display_id, video, &state.d_tx).await
}

#[derive(Deserialize)]
struct PostDataReconnectDevice {
    device_id: String,
    display_id: i32,
    video: bool,
}

async fn reconnect_device(
    State(state): State<AppStateDevice>,
    Json(payload): Json<PostDataReconnectDevice>,
) -> Result<JsonResponse, WebServerError> {
    let device_id = payload.device_id;
    let device_list = ControlledDevice::get_device_list().await;
    for device in device_list {
        if device.device_id == device_id {
            _decontrol_device(&device_id, &state.d_tx).await?;
            _control_device(&device_id, payload.display_id, payload.video, &state.d_tx).await?;
            return Ok(JsonResponse::success(
                format!("{}: {}", t!("web.device.reconnectDevice"), device_id),
                None,
            ));
        }
    }
    Err(WebServerError::bad_request(format!(
        "{}: {}",
        t!("web.device.deviceNotFound"),
        device_id
    )))
}

#[derive(Deserialize)]
struct PostDataDeControlDevice {
    device_id: String,
}

async fn _decontrol_device(
    device_id: &str,
    d_tx: &UnboundedSender<ControllerCommand>,
) -> Result<JsonResponse, WebServerError> {
    {
        let mut processes = ACTIVE_PROCESSES.lock().await;
        if let Some(kill_tx) = processes.remove(device_id) {
            log::info!("[WebServe] Sending kill signal to adb shell process for device: {}", device_id);
            let _ = kill_tx.send(());
        }
    }

    let device_list = ControlledDevice::get_device_list().await;
    for device in device_list {
        if device.device_id == device_id {
            let scid = device.scid.clone();
            if device.main {
                if let Err(e) = d_tx.send(ControllerCommand::ShutdownMain(scid)) {
                    log::error!("[WebServe] Failed to send shutdown command to controller: {}", e);
                }
            } else {
                if let Err(e) = d_tx.send(ControllerCommand::ShutdownSub(scid)) {
                    log::error!("[WebServe] Failed to send shutdown command to controller: {}", e);
                }
            }
            ControlledDevice::remove_device(&device.scid).await;
            return Ok(JsonResponse::success(
                format!("{}: {}", t!("web.device.decontrolDevice"), device_id),
                None,
            ));
        }
    }
    Err(WebServerError::bad_request(format!(
        "{}: {}",
        t!("web.device.deviceNotFound"),
        device_id
    )))
}

async fn decontrol_device(
    State(state): State<AppStateDevice>,
    Json(payload): Json<PostDataDeControlDevice>,
) -> Result<JsonResponse, WebServerError> {
    let device_id = payload.device_id;
    _decontrol_device(&device_id, &state.d_tx).await
}

#[derive(Deserialize)]
struct PostDataAddress {
    address: String,
}

async fn adb_connect(Json(payload): Json<PostDataAddress>) -> Result<JsonResponse, WebServerError> {
    let config = LocalConfig::get();
    match Adb::new(config.adb_path).connect_device(&payload.address) {
        Ok(_) => Ok(JsonResponse::success(
            format!(
                "{}",
                t!("web.device.adbConnect", address => payload.address)
            ),
            None,
        )),
        Err(e) => Err(WebServerError::bad_request(format!(
            "{}: {}",
            t!("web.device.adbConnectFailed", address => payload.address),
            e
        ))),
    }
}

#[derive(Deserialize)]
struct PostDataAdbPair {
    address: String,
    code: String,
}

async fn adb_pair(Json(payload): Json<PostDataAdbPair>) -> Result<JsonResponse, WebServerError> {
    let config = LocalConfig::get();
    match Adb::new(config.adb_path).pair_device(&payload.address, &payload.code) {
        Ok(_) => Ok(JsonResponse::success(
            format!(
                "{}",
                t!("web.device.adbPairSuccess", address => payload.address, code => payload.code)
            ),
            None,
        )),
        Err(e) => Err(WebServerError::bad_request(format!(
            "{}: {}",
            t!("web.device.adbPairFailed", address => payload.address, code => payload.code),
            e
        ))),
    }
}

#[derive(Deserialize)]
struct PostDataId {
    id: String,
}

async fn adb_screenshot(
    Json(payload): Json<PostDataId>,
) -> Result<impl IntoResponse, WebServerError> {
    let src = "/data/local/tmp/_screenshot_scrcpy_mask.png";

    let mut display_id_info = Vec::new();
    Device::shell(
        &payload.id,
        ["dumpsys", "SurfaceFlinger", "--display-id"],
        &mut display_id_info,
    )
    .map_err(|e| WebServerError::bad_request(format!("failed get display id: {}", e)))?;
    let text = String::from_utf8_lossy(&display_id_info);
    let first_line = text
        .lines()
        .next()
        .ok_or_else(|| WebServerError::bad_request("no display found"))?;
    let display_id = first_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| WebServerError::bad_request("invalid display line"))?;

    Device::shell(
        &payload.id,
        ["screencap", "-p", "-d", display_id, src],
        &mut std::io::stdout(),
    )
    .map_err(|e| {
        WebServerError::bad_request(format!(
            "{} {}: {}",
            t!("web.device.screenshotError"),
            payload.id,
            e
        ))
    })?;

    let mut image_bytes = Vec::<u8>::new();
    Device::pull(&payload.id, src.to_string(), &mut image_bytes).map_err(|e| {
        WebServerError::bad_request(format!(
            "{}: {}",
            t!("web.device.failedGetScreenshotFile"),
            e
        ))
    })?;

    Device::shell(&payload.id, ["rm", src], &mut std::io::stdout()).map_err(|e| {
        WebServerError::bad_request(format!(
            "{} {}: {}",
            t!("web.device.failedRemoveScreenshot"),
            payload.id,
            e
        ))
    })?;

    let mut headers = HeaderMap::new();
    headers.insert("Content-Type", HeaderValue::from_static("image/png"));
    headers.insert("Cache-Control", HeaderValue::from_static("no-cache"));

    Ok((StatusCode::OK, headers, image_bytes))
}

#[derive(Deserialize)]
struct PostDataSetDisplayPower {
    mode: bool,
}
async fn set_display_power(
    State(state): State<AppStateDevice>,
    Json(payload): Json<PostDataSetDisplayPower>,
) -> Result<JsonResponse, WebServerError> {
    if !ControlledDevice::is_any_device_controlled().await {
        return Err(WebServerError::bad_request(t!(
            "web.device.noDeviceControlled"
        )));
    }

    if let Err(e) = state
        .cs_tx
        .send(ScrcpyControlMsg::SetDisplayPower { mode: payload.mode })
    {
        log::warn!("[WebServe] Failed to send SetDisplayPower: {}", e);
    }
    Ok(JsonResponse::success(
        t!("web.device.setDisplayPowerSuccess"),
        None,
    ))
}

#[derive(Deserialize)]
struct PostDataSendKey {
    keycode: Keycode,
}

async fn send_key(
    State(state): State<AppStateDevice>,
    Json(payload): Json<PostDataSendKey>,
) -> Result<JsonResponse, WebServerError> {
    if !ControlledDevice::is_any_device_controlled().await {
        return Err(WebServerError::bad_request(t!(
            "web.device.noDeviceControlled"
        )));
    }

    if let Err(e) = state
        .cs_tx
        .send(ScrcpyControlMsg::InjectKeycode {
            action: KeyEventAction::Down,
            keycode: payload.keycode.clone(),
            repeat: 0,
            metastate: MetaState::NONE,
        })
    {
        log::warn!("[WebServe] Failed to send KeyDown: {}", e);
    }
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    if let Err(e) = state
        .cs_tx
        .send(ScrcpyControlMsg::InjectKeycode {
            action: KeyEventAction::Up,
            keycode: payload.keycode,
            repeat: 0,
            metastate: MetaState::NONE,
        })
    {
        log::warn!("[WebServe] Failed to send KeyUp: {}", e);
    }
    Ok(JsonResponse::success(t!("web.device.sendKeySuccess"), None))
}

#[derive(Deserialize)]
struct PostDataEvalScript {
    script: String,
}

async fn eval_script(
    State(state): State<AppStateDevice>,
    Json(payload): Json<PostDataEvalScript>,
) -> Result<JsonResponse, WebServerError> {
    if !ControlledDevice::is_any_device_controlled().await {
        return Err(WebServerError::bad_request(t!(
            "web.device.noDeviceControlled"
        )));
    }

    let (oneshot_tx, oneshot_rx) = oneshot::channel::<Result<String, String>>();
    if let Err(e) = state
        .m_tx
        .send((
            MaskCommand::EvalScript {
                script: payload.script,
            },
            oneshot_tx,
        ))
    {
        log::error!("[WebServe] Failed to send EvalScript command: {}", e);
        return Err(WebServerError::internal_error(e.to_string()));
    }
    match oneshot_rx.await {
        Ok(res) => match res {
            Ok(_) => Ok(JsonResponse::success(
                t!("web.device.evalScriptSuccess"),
                None,
            )),
            Err(e) => Err(WebServerError::bad_request(format!(
                "{}:\n{}",
                t!("web.device.evalScriptError"),
                e
            ))),
        },
        Err(e) => Err(WebServerError::internal_error(format!(
            "Bevy main thread dropped the response channel: {}",
            e
        ))),
    }
}
