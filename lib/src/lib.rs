#![allow(incomplete_features)]
#![allow(clippy::module_inception)]
#![allow(clippy::eq_op)]

pub mod bevy;
pub mod color;
pub mod dmx;
pub mod e131;
pub mod lights;
pub mod midi;
pub mod num;
pub mod osc;

/// A set of common traits and types. Bring in scope with `use prelude::*`.
pub mod prelude {
    pub use anyhow::Context;

    pub use crate::bevy::{FloatExt, *};
    pub use crate::color::*;
    pub use crate::dmx::{DmxDevice, DmxUniverse};
    pub use crate::e131::E131;
    pub use crate::midi::{Midi, MidiDevice};
    pub use crate::num::{Ease, *};
    pub use crate::osc::*;
}
