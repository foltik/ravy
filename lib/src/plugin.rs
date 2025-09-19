use bevy::asset::io::memory::{Dir, MemoryAssetReader};
use bevy::asset::io::{AssetSource, AssetSourceId};
use bevy::window::WindowMode;

use crate::prelude::*;

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

        // XXX: fix the data flow
        let models = Dir::default();
        let reader = MemoryAssetReader { root: models.clone() };
        app.register_asset_source(
            AssetSourceId::from_static("memory"),
            AssetSource::build().with_reader(move || Box::new(reader.clone())),
        );

        app.add_plugins(DefaultPlugins.set(bevy::log::LogPlugin {
            filter: format!("{deps_log_level},{}={app_log_level}", self.module),
            ..default()
        }))
        .add_plugins(super::gltf::GltfScenePlugin)
        .add_plugins(super::audio::AudioPlugin)
        .add_plugins(super::ui::UiPlugin)
        .add_plugins(super::sim::SimPlugin)
        .add_plugins(super::lights::LightsPlugin { models })
        .add_systems(PreUpdate, hotkeys);
    }
}

pub fn hotkeys(
    keys: Res<ButtonInput<KeyCode>>,
    mut ui: ResMut<Ui>,
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

    if keys.just_pressed(KeyCode::Backquote) {
        ui.visible = !ui.visible;
    }
}
