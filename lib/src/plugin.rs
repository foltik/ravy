use std::time::Duration;

use bevy::app::ScheduleRunnerPlugin;
use bevy::asset::io::AssetSourceBuilder;
use bevy::asset::io::memory::{Dir, MemoryAssetReader};
use bevy::window::WindowMode;

use crate::prelude::*;

pub struct RavyPlugin {
    pub module: &'static str,
    pub debug: bool,
    pub trace: bool,
}

/// The log filter: lib is ours too, so it speaks at the app's level, not a
/// dep's.
fn log_filter(module: &str, debug: bool, trace: bool) -> String {
    #[rustfmt::skip]
    let (app_log_level, deps_log_level) = match (debug, trace) {
        (_, true) => ("trace", "debug"), // -V, --trace
        (true, _) => ("debug", "info"),  // -v, --debug
        (_, _)    => ("info",  "warn"),  // default
    };

    let quiet = if debug || trace {
        ""
    } else {
        // Benign startup noise: no XSETTINGS daemon, swapchain rebuilds while
        // the window settles, no @invariant in bevy's mesh shader under the
        // depth prepass.
        ",winit::platform_impl::linux::x11::xdisplay=error,bevy_render::view::window=error,wgpu_core::validation=error"
    };
    format!("{deps_log_level}{quiet},lib={app_log_level},{module}={app_log_level}")
}

/// The show without a window: no render, no sim, no panels. For a box with
/// nothing but the control surfaces and the wire plugged into it. The apps
/// own the rest: anything of theirs that reads the sim or draws a panel is
/// theirs to leave out.
pub struct RavyHeadlessPlugin {
    pub module: &'static str,
    pub debug: bool,
    pub trace: bool,
}

/// How often the headless loop ticks: what a windowed app gets from vsync.
const HEADLESS_TICK: Duration = Duration::from_micros(1_000_000 / 60);

impl Plugin for RavyHeadlessPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(HEADLESS_TICK)))
            .add_plugins(bevy::log::LogPlugin {
                filter: log_filter(self.module, self.debug, self.trace),
                ..default()
            });
    }
}

impl Plugin for RavyPlugin {
    fn build(&self, app: &mut App) {
        // XXX: fix the data flow
        let models = Dir::default();
        let reader = MemoryAssetReader { root: models.clone() };
        app.register_asset_source("memory", AssetSourceBuilder::new(move || Box::new(reader.clone())));

        // Part meshes unpacked from .gdtf archives at runtime.
        let gdtf_models = Dir::default();
        let reader = MemoryAssetReader { root: gdtf_models.clone() };
        app.register_asset_source("gdtf", AssetSourceBuilder::new(move || Box::new(reader.clone())));

        app.add_plugins(DefaultPlugins.set(bevy::log::LogPlugin {
            filter: log_filter(self.module, self.debug, self.trace),
            ..default()
        }))
        .add_plugins(super::gltf::GltfScenePlugin)
        .add_plugins(super::audio::AudioPlugin)
        .add_plugins(super::ui::UiPlugin)
        .add_plugins(super::sim::SimPlugin)
        .add_plugins(super::lights::LightsPlugin { models })
        .add_plugins(super::gdtf::GdtfPlugin { models: gdtf_models })
        .add_systems(PreUpdate, hotkeys);
    }
}

pub fn hotkeys(
    keys: Res<ButtonInput<KeyCode>>,
    mut ui: ResMut<Ui>,
    mut window: Single<&mut Window, With<bevy::window::PrimaryWindow>>,
    mut exit: MessageWriter<AppExit>,
) {
    if keys.pressed(KeyCode::ShiftLeft) && keys.just_pressed(KeyCode::KeyQ) {
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
