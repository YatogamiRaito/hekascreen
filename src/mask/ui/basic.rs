
/// Bevy basic plugin that creates a window with a transparent background and a border.
use bevy::{
    prelude::*,
    winit::{UpdateMode, WinitSettings},
};

pub const BORDER_THICKNESS: f32 = 1.0; // logical size

pub struct BasicPlugin;

impl Plugin for BasicPlugin {
    fn build(&self, app: &mut App) {
        app
            // ClearColor resource: The color used to clear the screen at the beginning of each frame
            .insert_resource(ClearColor(Color::NONE))
            .insert_resource(WinitSettings {
                focused_mode: UpdateMode::Continuous,
                unfocused_mode: UpdateMode::Continuous,
            })
            .add_systems(Startup, (setup, border.after(setup)));
    }
}

fn setup(mut commands: Commands) {
    // NOTE: Do NOT touch the Window component here. On X11, even a benign property
    // write (e.g. resolution) on a hidden (unmapped) window is enough to trigger a
    // ConfigureRequest, which some window managers respond to by mapping the window.
    // The real size is set by MaskCommand::WinMove once a device connects.
    commands.spawn(Camera2d::default());
}

fn border(mut commands: Commands) {
    let border_color = Color::srgba_u8(183, 42, 32, 255);
    commands.spawn((
        Node {
            width: Val::Percent(100.),
            height: Val::Percent(100.),
            border: UiRect::all(Val::Px(BORDER_THICKNESS)),
            box_sizing: BoxSizing::BorderBox,
            ..default()
        },
        BackgroundColor(Color::NONE),
        BorderColor::all(border_color),
    ));
}
