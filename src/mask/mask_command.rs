use bevy::{prelude::*, window::{WindowLevel, PrimaryWindow, PresentMode, CompositeAlphaMode}};
use rust_i18n::t;

use crate::{
    config::LocalConfig,
    mask::mapping::{
        MappingState,
        config::{
            ActiveMappingConfig, MappingConfig, load_mapping_config, validate_mapping_config,
        },
        cursor::{CursorPosition, CursorState},
        input_actions::RebuildInputBindings,
        script_helper::ScriptAST,
    },
    utils::{ChannelReceiverM, ChannelSenderCS, ChannelSenderV},
    scrcpy::media::VideoMsg,
};
use bevy_tokio_tasks::TokioTasksRuntime;

#[derive(Debug)]
pub enum MaskCommand {
    WinMove {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    },
    WinSwitchLevel {
        top: bool,
    },
    DeviceConnectionChange {
        connect: bool,
    },
    GetActiveMapping,
    ValidateMappingConfig {
        config: MappingConfig,
    },
    LoadAndActivateMappingConfig {
        file_name: String,
    },
    EvalScript {
        script: String,
    },
}

#[derive(Resource)]
pub struct MaskSize(pub Vec2);

pub enum ResizeState {
    Resizing,
    Showing,
}

pub struct PendingResize {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
    pub oneshot_tx: Option<tokio::sync::oneshot::Sender<Result<String, String>>>,
    pub state: ResizeState,
}

pub fn handle_mask_command(
    m_rx: Res<ChannelReceiverM>,
    cs_tx_res: Res<ChannelSenderCS>,
    v_tx_res: Res<ChannelSenderV>,
    cursor_pos: Res<CursorPosition>,
    mut window_query: Query<(Entity, &mut Window), With<PrimaryWindow>>,
    mut next_mapping_state: ResMut<NextState<MappingState>>,
    mut next_cursor_state: ResMut<NextState<CursorState>>,
    mut commands: Commands,
    mut active_mapping: ResMut<ActiveMappingConfig>,
    mut mask_size: ResMut<MaskSize>,
    mut pending_resize: Local<Option<PendingResize>>,
    runtime: ResMut<TokioTasksRuntime>,
    // Linux: track whether a device is currently connected so that stale WinMove
    // commands arriving after disconnect do not re-spawn the window.
    mut device_connected: Local<bool>,
    // Linux: entity of the spawned mask window (None when no device is connected).
    mut mask_window_entity: Local<Option<Entity>>,
) {
    if let Some(ref mut pending) = *pending_resize {
        if let Ok((_, mut window)) = window_query.single_mut() {
            match pending.state {
                ResizeState::Resizing => {
                    let width = (pending.right - pending.left) as f32;
                    let height = (pending.bottom - pending.top) as f32;

                    window.resolution.set(width, height);
                    window.position.set((pending.left, pending.top).into());

                    pending.state = ResizeState::Showing;
                }
                ResizeState::Showing => {
                    window.visible = true;
                    mask_size.0 = window.resolution.size();

                    let msg = t!(
                        "mask.windowMovedAndResized",
                        left => pending.left,
                        top => pending.top,
                        width => mask_size.0.x,
                        height => mask_size.0.y
                    )
                    .to_string();

                    log::info!("[Mask] {}", msg);
                    if let Some(oneshot_tx) = pending.oneshot_tx.take() {
                        let _ = oneshot_tx.send(Ok(msg));
                    }

                    *pending_resize = None;
                }
            }
        }
        return;
    }

    for (msg, oneshot_tx) in m_rx.0.try_iter() {
        match msg {
            MaskCommand::WinMove {
                left,
                top,
                right,
                bottom,
            } => {
                let width = (right - left) as f32;
                let height = (bottom - top) as f32;

                #[cfg(target_os = "linux")]
                {
                    // Discard stale WinMove messages that arrive after device disconnect.
                    if !*device_connected {
                        let _ = oneshot_tx.send(Ok(String::new()));
                    } else if let Ok((_, mut window)) = window_query.single_mut() {
                        // Window already exists (spawned in DeviceConnectionChange) — update it.
                        window.resolution.set(width, height);
                        window.position.set((left, top).into());
                        // On X11 this actually shows the window (we start with visible:false).
                        // On Wayland set_visible is a no-op but the window is already visible.
                        window.visible = true;

                        mask_size.0 = window.resolution.size();

                        let msg = t!(
                            "mask.windowMovedAndResized",
                            left => left,
                            top => top,
                            width => mask_size.0.x,
                            height => mask_size.0.y
                        )
                        .to_string();

                        log::info!("[Mask] {}", msg);
                        let _ = oneshot_tx.send(Ok(msg));
                    } else {
                        // Window entity was spawned this frame but commands haven't been flushed
                        // yet (deferred). This should be rare; just report size.
                        mask_size.0 = Vec2::new(width, height);
                        let msg = t!(
                            "mask.windowMovedAndResized",
                            left => left,
                            top => top,
                            width => width,
                            height => height
                        )
                        .to_string();
                        log::info!("[Mask] {}", msg);
                        let _ = oneshot_tx.send(Ok(msg));
                    }
                }

                #[cfg(not(target_os = "linux"))]
                {
                    if let Ok((_, window)) = window_query.single() {
                        if window.visible {
                            *pending_resize = Some(PendingResize {
                                left,
                                top,
                                right,
                                bottom,
                                oneshot_tx: Some(oneshot_tx),
                                state: ResizeState::Resizing,
                            });
                            if let Ok((_, mut window_mut)) = window_query.single_mut() {
                                window_mut.visible = false;
                            }
                            return;
                        } else {
                            if let Ok((_, mut window_mut)) = window_query.single_mut() {
                                window_mut.resolution.set(width, height);
                                window_mut.position.set((left, top).into());
                            }

                            mask_size.0 = Vec2::new(width, height);

                            let msg = t!(
                                "mask.windowMovedAndResized",
                                left => left,
                                top => top,
                                width => mask_size.0.x,
                                height => mask_size.0.y
                            )
                            .to_string();

                            log::info!("[Mask] {}", msg);
                            let _ = oneshot_tx.send(Ok(msg));
                        }
                    }
                }
            }
            MaskCommand::WinSwitchLevel { top } => {
                if let Ok((_, mut window)) = window_query.single_mut() {
                    if top {
                        window.window_level = WindowLevel::AlwaysOnTop;
                    } else {
                        window.window_level = WindowLevel::Normal;
                    }
                }
                let msg = format!("[Mask] {}: {}", t!("mask.windowLevelChanged"), top);
                log::info!("{}", msg);
                oneshot_tx.send(Ok(msg)).unwrap();
            }
            MaskCommand::DeviceConnectionChange { connect } => {
                let msg = if connect {
                    next_mapping_state.set(MappingState::Normal);
                    log::info!("[Mapping] {}", t!("mask.enterNormalMappingMode"));

                    #[cfg(target_os = "linux")]
                    {
                        *device_connected = true;
                        let config = LocalConfig::get();
                        let present_mode = match config.present_mode.as_str() {
                            "AutoNoVsync" => PresentMode::AutoNoVsync,
                            "Immediate" => PresentMode::Immediate,
                            "Mailbox" => PresentMode::Mailbox,
                            _ => PresentMode::AutoVsync,
                        };
                        let window_level = if config.always_on_top {
                            WindowLevel::AlwaysOnTop
                        } else {
                            WindowLevel::Normal
                        };
                        let entity = commands.spawn((
                            Window {
                                name: Some("scrcpy-mask".to_string()),
                                has_shadow: false,
                                transparent: true,
                                decorations: false,
                                present_mode,
                                resizable: true,
                                // X11: starts hidden, made visible on first WinMove.
                                // Wayland: set_visible is a no-op; the window appears immediately.
                                visible: false,
                                focused: false,
                                window_level,
                                composite_alpha_mode: CompositeAlphaMode::PreMultiplied,
                                ..default()
                            },
                            PrimaryWindow,
                        )).id();
                        *mask_window_entity = Some(entity);
                    }

                    #[cfg(not(target_os = "linux"))]
                    if let Ok((_, mut window)) = window_query.single_mut() {
                        window.visible = true;
                    }

                    t!("mask.mainDeviceConnected").to_string()
                } else {
                    next_cursor_state.set(CursorState::Normal);
                    next_mapping_state.set(MappingState::Stop);
                    log::info!("[Mapping] {}", t!("mask.exitStopMappingMode"));

                    #[cfg(target_os = "linux")]
                    {
                        *device_connected = false;
                        if let Some(entity) = mask_window_entity.take() {
                            commands.entity(entity).despawn();
                        }
                    }

                    #[cfg(not(target_os = "linux"))]
                    if let Ok((_, mut window)) = window_query.single_mut() {
                        window.visible = false;
                    }

                    t!("mask.mainDeviceDisconnected").to_string()
                };
                log::info!("[Mask] {}", msg);
                oneshot_tx.send(Ok(msg)).unwrap();
            }
            MaskCommand::GetActiveMapping => {
                oneshot_tx.send(Ok(active_mapping.1.clone())).unwrap();
            }
            MaskCommand::ValidateMappingConfig { config } => {
                match validate_mapping_config(&config) {
                    Ok(_) => {
                        oneshot_tx.send(Ok(String::new())).unwrap();
                    }
                    Err(err) => {
                        oneshot_tx.send(Err(err)).unwrap();
                    }
                }
            }
            MaskCommand::LoadAndActivateMappingConfig { file_name } => {
                log::info!(
                    "[Mapping] {}: {}",
                    t!("mask.loadActivateMappingConfig"),
                    file_name
                );
                match load_mapping_config(&file_name) {
                    Ok(mapping_config) => {
                        active_mapping.0 = Some(mapping_config);
                        active_mapping.1 = file_name;
                        commands.trigger(RebuildInputBindings);
                        oneshot_tx.send(Ok(String::new())).unwrap();
                    }
                    Err(e) => {
                        oneshot_tx.send(Err(e)).unwrap();
                    }
                }
            }
            MaskCommand::EvalScript { script } => {
                let ast = match ScriptAST::new(&script) {
                    Err(e) => {
                        let _ = oneshot_tx.send(Err(e));
                        continue;
                    }
                    Ok(ast) => ast,
                };

                if let Some(mapping_config) = &active_mapping.0 {
                    let cs_tx = cs_tx_res.0.clone();
                    let v_tx = v_tx_res.0.clone();
                    let original_size = mapping_config.original_size.into();
                    let cursor_pos = cursor_pos.0;
                    let mask_size = mask_size.0;

                    let _ = v_tx.send(VideoMsg::ScriptClearError);
                    runtime.spawn_background_task(move |_ctx| async move {
                        let res = tokio::task::spawn_blocking(move || {
                            ast.eval_script(&cs_tx, original_size, cursor_pos, mask_size)
                        }).await;
                        match res {
                            Ok(Err(e)) => {
                                let _ = v_tx.send(VideoMsg::ScriptError { error: format!("Eval Script: {}", e) });
                                let _ = oneshot_tx.send(Err(e.to_string()));
                            }
                            Ok(Ok(_)) => {
                                let _ = oneshot_tx.send(Ok(String::new()));
                            }
                            Err(e) => {
                                log::error!("Tokio task join error: {}", e);
                                let _ = oneshot_tx.send(Err(e.to_string()));
                            }
                        }
                    });
                } else {
                    let _ = oneshot_tx.send(Err(t!("mask.evalScriptnoMappingError").to_string()));
                }
            }
        }
    }
}
