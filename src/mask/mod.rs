pub mod mapping;
pub mod mask_command;
pub mod ui;
pub mod video;

use bevy::prelude::*;

use crate::mask::{
    mask_command::{MaskSize, handle_mask_command},
    video::{VideoAttributes, handle_video_msg, init_video, update_diagnostics_hud},
};

pub struct MaskPlugins;

impl Plugin for MaskPlugins {
    fn build(&self, app: &mut App) {
        app.add_plugins((ui::UiPlugins, mapping::MappingPlugins))
            .init_non_send_resource::<VideoAttributes>()
            .add_systems(Startup, (init_mask_size, init_video))
            .add_systems(
                Update,
                (
                    handle_mask_command,
                    handle_video_msg.after(handle_mask_command),
                    update_diagnostics_hud.after(handle_video_msg),
                ),
            );
    }
}

fn init_mask_size(mut commands: Commands) {
    // On Linux the primary window is not created at startup (it is spawned on device
    // connect), so we cannot read its size here. Use a safe default; the real size is
    // set by MaskCommand::WinMove once a device connects.
    commands.insert_resource(MaskSize(Vec2::new(800., 600.)));
}
