use bevy::prelude::*;
use bevy::window::WindowMode;

pub struct RavyPlugin {
    pub module: &'static str,
    pub debug: bool,
    pub trace: bool,
}

impl Plugin for RavyPlugin {
    fn build(&self, app: &mut App) {
        #[rustfmt::skip]
        let (app_log_level, deps_log_level) = match (self.debug, self.trace) {
            (_, true) => ("trace", "debug"), // -V, --trace
            (true, _) => ("debug", "info"),  // -v, --debug
            (_, _)    => ("info",  "warn"),  // default
        };

        app.add_plugins(DefaultPlugins.set(bevy::log::LogPlugin {
            filter: format!("{deps_log_level},{}={app_log_level}", self.module),
            ..default()
        }))
        .add_plugins(crate::gltf::GltfScenePlugin)
        .add_systems(PreUpdate, hotkeys);
    }
}

pub fn hotkeys(
    keys: Res<ButtonInput<KeyCode>>,
    mut window: Single<&mut Window>,
    mut exit: EventWriter<AppExit>,
) {
    if keys.just_pressed(KeyCode::KeyQ) {
        exit.write(AppExit::Success);
    }

    if keys.pressed(KeyCode::AltLeft) && keys.just_pressed(KeyCode::Enter) {
        window.mode = match window.mode {
            WindowMode::Windowed => WindowMode::BorderlessFullscreen(MonitorSelection::Current),
            _ => WindowMode::Windowed,
        };
    }
}
