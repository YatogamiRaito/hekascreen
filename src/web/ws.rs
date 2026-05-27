use axum::{
    Router,
    extract::{
        State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    response::IntoResponse,
    routing::any,
};
use rust_i18n::t;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast::{self, error::RecvError};

use crate::{
    scrcpy::{ScrcpyDevice, constant, control_msg::ScrcpyControlMsg, controller::ControllerCommand},
    utils::share::ControlledDevice,
};
use tokio::sync::mpsc::UnboundedSender;
use futures_util::{
    SinkExt,
    stream::{SplitSink, SplitStream, StreamExt},
};

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum WebSocketNotification {
    ScrcpyDeviceRotation {
        rotation: u16,
        width: u32,
        height: u32,
        scid: String,
    },
    ScrcpyDeviceConnection {
        scid: String,
        main: bool,
        connected: bool,
    },
    ScrcpyDeviceList {
        devices: Vec<ScrcpyDevice>,
    },
}

impl From<WebSocketNotification> for Message {
    fn from(msg: WebSocketNotification) -> Self {
        let json = serde_json::to_string(&msg).unwrap();
        Message::Text(json.into())
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")] // use type field to determine which variant to use
enum WebSocketMsg {
    InjectKeycode {
        action: constant::KeyEventAction, // Enum name, e.g., Down, Up
        keycode: constant::Keycode,       // Enum name, e.g., Home, Back
        metastate: constant::MetaState,   // Enum name, e.g., NONE, CTRL_ON|SHIFT_ON
    },
    InjectText {
        text: String,
    },
    InjectTouchEvent {
        action: constant::MotionEventAction, // Enum name, e.g., Down, Up
        pointer_id: u64, // ID representing a single finger (for multi-touch tracking)
        x: i32,
        y: i32,
        w: u16, // Expected screen width
        h: u16, // Expected screen height
                // The final (x, y) coordinates will be scaled in scrcpy-server to match the actual device resolution
                // based on the ratio between expected dimensions (w, h) and the actual device screen size.
                // For example, using x=100, y=100, w=200, h=200 will tap the center of the screen no matter what the actual screen size is.
    },
    InjectScrollEvent {
        x: i32,
        y: i32,
        w: u16,
        h: u16,
        hscroll: u16,
        vscroll: u16,
    },
    SetClipboard {
        sequence: u64,
        paste: bool,
        text: String,
    },
}

impl From<WebSocketMsg> for ScrcpyControlMsg {
    fn from(msg: WebSocketMsg) -> Self {
        match msg {
            WebSocketMsg::InjectKeycode {
                action,
                keycode,
                metastate,
            } => ScrcpyControlMsg::InjectKeycode {
                action,
                keycode,
                repeat: 0,
                metastate,
            },
            WebSocketMsg::InjectText { text } => ScrcpyControlMsg::InjectText { text },
            WebSocketMsg::InjectTouchEvent {
                action,
                pointer_id,
                x,
                y,
                w,
                h,
            } => ScrcpyControlMsg::InjectTouchEvent {
                action,
                pointer_id,
                x,
                y,
                w,
                h,
                pressure: half::f16::from_f32_const(1.0),
                action_button: constant::MotionEventButtons::PRIMARY,
                buttons: constant::MotionEventButtons::PRIMARY,
            },
            WebSocketMsg::InjectScrollEvent {
                x,
                y,
                w,
                h,
                hscroll,
                vscroll,
            } => ScrcpyControlMsg::InjectScrollEvent {
                x,
                y,
                w,
                h,
                hscroll,
                vscroll,
                buttons: 0,
            },
            WebSocketMsg::SetClipboard {
                sequence,
                paste,
                text,
            } => ScrcpyControlMsg::SetClipboard {
                sequence,
                paste,
                text,
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct AppStateWS {
    cs_tx: broadcast::Sender<ScrcpyControlMsg>,
    ws_tx: broadcast::Sender<WebSocketNotification>,
    d_tx: UnboundedSender<ControllerCommand>,
}

pub fn routers(
    cs_tx: broadcast::Sender<ScrcpyControlMsg>,
    ws_tx: broadcast::Sender<WebSocketNotification>,
    d_tx: UnboundedSender<ControllerCommand>,
) -> Router {
    Router::new()
        .route("/connect", any(ws_handler))
        .with_state(AppStateWS { cs_tx, ws_tx, d_tx })
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppStateWS>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state.cs_tx, state.ws_tx.subscribe(), state.d_tx))
}

async fn handle_socket(
    socket: WebSocket,
    cs_tx: broadcast::Sender<ScrcpyControlMsg>,
    ws_rx: broadcast::Receiver<WebSocketNotification>,
    d_tx: UnboundedSender<ControllerCommand>,
) {
    log::info!("[WebSocket] {}", t!("web.ws.connected"));
    let (sender, receiver) = socket.split();

    let mut send_handler = tokio::spawn(async move {
        handle_send(sender, ws_rx).await;
    });

    let mut recv_handler = tokio::spawn(async move {
        handle_recv(receiver, cs_tx).await;
    });

    tokio::select! {
        _ = (&mut send_handler) => {
            recv_handler.abort();
        },
        _ = (&mut recv_handler) => {
            send_handler.abort();
        }
    }
    log::info!("[WebSocket] {}", t!("web.ws.disconnected"));

    // Cleanup: shut down all controlled streaming sessions since WebSocket disconnected
    let device_list = ControlledDevice::get_device_list().await;
    for device in device_list {
        let scid = device.scid.clone();
        let cmd = if device.main {
            ControllerCommand::ShutdownMain(scid)
        } else {
            ControllerCommand::ShutdownSub(scid)
        };
        if let Err(e) = d_tx.send(cmd) {
            log::error!("[WebSocket] Failed to send shutdown command to controller: {}", e);
        }
        ControlledDevice::remove_device(&device.scid).await;
    }
}

async fn handle_send(
    mut sender: SplitSink<WebSocket, Message>,
    mut ws_rx: broadcast::Receiver<WebSocketNotification>,
) {
    if sender
        .send(
            (WebSocketNotification::ScrcpyDeviceList {
                devices: ControlledDevice::get_device_list().await,
            })
            .into(),
        )
        .await
        .is_err()
    {
        return;
    }

    loop {
        match ws_rx.recv().await {
            Ok(msg) => {
                if sender.send(msg.into()).await.is_err() {
                    break;
                }
            }
            Err(RecvError::Lagged(skipped)) => {
                log::warn!(
                    "[WebSocket] {}",
                    t!("web.ws.receiverLagged", skipped => skipped)
                );
            }
            Err(e) => {
                log::info!("[WebSocket] {}: {}", t!("web.ws.wsChannelClosed"), e);
                break;
            }
        }
    }
}

async fn handle_recv(
    mut receiver: SplitStream<WebSocket>,
    cs_tx: broadcast::Sender<ScrcpyControlMsg>,
) {
    while let Some(Ok(msg)) = receiver.next().await {
        match msg {
            Message::Text(t) => {
                if t.len() > 1_048_576 {
                    log::warn!("[WebSocket] Received text message exceeding 1MB limit: {} bytes", t.len());
                    continue;
                }
                let msg: WebSocketMsg = match serde_json::from_str(&t) {
                    Ok(m) => m,
                    Err(e) => {
                        log::error!("[WebSocket] {}: {}", t!("web.ws.failedToParseMessage"), e);
                        continue;
                    }
                };
                if let Err(e) = cs_tx.send(msg.into()) {
                    log::warn!("[WebSocket] Failed to broadcast control message: {}", e);
                }
            }
            Message::Close(_) => {
                break;
            }
            _ => {}
        }
    }
}
