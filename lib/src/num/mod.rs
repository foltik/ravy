mod axis;
mod byte;
mod db;
mod ease;
mod ema;
mod interp;
mod pd;
mod range;

pub use axis::Axis;
pub use byte::Byte;
pub use db::*;
pub use ease::Ease;
pub use ema::Ema;
pub use interp::Interp;
pub use pd::Pd;
pub use range::Range;

/// τ/2, a half circle.
pub const PI: f32 = std::f32::consts::PI;

/// τ, a full circle.
pub const TAU: f32 = std::f32::consts::TAU;
/// τ/2, a half-circle.
pub const TAU_2: f32 = TAU / 2.0;
/// τ/4, a quarter-circle.
pub const TAU_4: f32 = TAU / 4.0;
