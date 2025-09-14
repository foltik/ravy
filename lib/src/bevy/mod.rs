pub use bevy::color::palettes::css::*;
pub use bevy::prelude::*;
pub use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};

mod audio;
mod gltf;
mod plugin;
mod ui;

pub use audio::*;
pub use gltf::*;
pub use plugin::RavyPlugin;
pub use ui::{Ui, widgets};

// A fake FloatExt trait to shadow bevy's which has a conflicting lerp() method
pub trait FloatExt {}
