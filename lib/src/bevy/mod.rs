pub use bevy::color::palettes::css::*;
pub use bevy::prelude::*;
pub use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};

mod audio;
mod debug;
mod gltf;
mod plugin;

pub use audio::*;
pub use debug::*;
pub use gltf::*;
pub use plugin::RavyPlugin;

// A fake FloatExt trait to shadow bevy's which has a conflicting lerp() method
pub trait FloatExt {}
