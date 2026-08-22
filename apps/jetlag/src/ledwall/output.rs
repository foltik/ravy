//! The popped-out output window for the LED processor.

use bevy::camera::visibility::RenderLayers;
use bevy::camera::{Projection, RenderTarget, ScalingMode};
use bevy::input::keyboard::KeyboardInput;
use bevy::window::{WindowRef, WindowResolution};
use lib::prelude::*;

use super::{LAYER_OUTPUT, PX, Screen};

/// Open the output window: drag it onto the HDMI display, tap F to fill it.
pub fn popout(cmds: &mut Commands, screen: &mut Screen) {
    if screen.output.is_some() {
        return;
    }
    let window = cmds
        .spawn(Window {
            title: "jetlag screen".to_string(),
            resolution: WindowResolution::new(PX.x * 2, PX.y * 2),
            ..default()
        })
        .id();
    let camera = cmds
        .spawn((
            Camera3d::default(),
            Camera { clear_color: ClearColorConfig::Custom(Color::BLACK), ..default() },
            RenderTarget::Window(WindowRef::Entity(window)),
            // AutoMin letterboxes: the frame fills the window without
            // stretching.
            Projection::from(OrthographicProjection {
                scaling_mode: ScalingMode::AutoMin {
                    min_width: PX.x as f32,
                    min_height: PX.y as f32,
                },
                ..OrthographicProjection::default_3d()
            }),
            Transform::from_xyz(0.0, 0.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
            RenderLayers::layer(LAYER_OUTPUT),
        ))
        .id();
    screen.output = Some((window, camera));
}

/// F pressed in the output window fullscreens it, wherever it was dragged.
pub(super) fn fullscreen(
    mut keys: MessageReader<KeyboardInput>,
    screen: Res<Screen>,
    mut windows: Query<&mut Window>,
) {
    use bevy::window::WindowMode;
    let Some((entity, _)) = screen.output else { return };
    for key in keys.read() {
        if key.window != entity
            || key.key_code != KeyCode::KeyF
            || !key.state.is_pressed()
            || key.repeat
        {
            continue;
        }
        let Ok(mut window) = windows.get_mut(entity) else { continue };
        window.mode = match window.mode {
            WindowMode::Windowed => WindowMode::BorderlessFullscreen(MonitorSelection::Current),
            _ => WindowMode::Windowed,
        };
    }
}

/// Drop the output camera once its window is closed.
pub(super) fn closed(
    mut cmds: Commands,
    mut removed: RemovedComponents<Window>,
    mut screen: ResMut<Screen>,
) {
    for entity in removed.read() {
        if let Some((window, camera)) = screen.output
            && window == entity
        {
            cmds.entity(camera).try_despawn();
            screen.output = None;
        }
    }
}
