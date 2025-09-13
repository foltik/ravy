#![allow(incomplete_features)]
#![allow(clippy::module_inception)]
#![allow(clippy::eq_op)]

pub mod bevy;
pub mod color;
pub mod dmx;
pub mod e131;
pub mod midi;
pub mod num;
pub mod osc;

/// A set of common traits and types. Bring in scope with `use prelude::*`.
pub mod prelude {
    pub use crate::bevy::*;
    pub use crate::color::{Rgb, Rgbw};
    pub use crate::midi::Midi;
    pub use crate::num::{Byte, Ease, Ema, Interp, Range, TAU, TAU_2, TAU_4};
}
