#![allow(incomplete_features)]
#![allow(clippy::module_inception)]
#![allow(clippy::eq_op)]

mod audio;
mod color;
pub mod dmx;
mod e131;
mod gltf;
pub mod lights;
pub mod midi;
mod num;
mod osc;
mod plugin;
pub mod ui;

/// A set of common traits and types. Bring in scope with `use prelude::*`.
pub mod prelude {
    pub use anyhow::Context;
    pub use bevy::color::palettes::css::*;
    pub use bevy::prelude::*;
    pub use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};

    pub use crate::audio::*;
    pub use crate::color::*;
    pub use crate::dmx::{DmxDevice, DmxUniverse};
    pub use crate::e131::E131;
    pub use crate::gltf::*;
    pub use crate::midi::{Midi, MidiDevice};
    pub use crate::num::{Ease, *};
    pub use crate::osc::*;
    pub use crate::plugin::RavyPlugin;
    pub use crate::ui::{self, Ui};

    // A fake FloatExt trait to shadow bevy's which has a conflicting lerp() method
    pub trait FloatExt {}
}
