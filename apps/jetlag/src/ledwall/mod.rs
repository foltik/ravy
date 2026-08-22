//! The LED wall: 3x2 modules of 0.5m, 128px square each, so a 1.5m x 1.0m
//! surface carrying 384x256 pixels.
//!
//! One offscreen camera renders the selected pattern into a 384x256 feed,
//! the fx pass runs the milstrike filter chain over it, and the result is
//! the wall's one source of truth. The sim shows it through the LED panel
//! material on the wall mesh; the Jetlag pane's Screen button pops out a
//! window with it upscaled nearest-neighbour for the LED processor: drag
//! that window onto the extended HDMI output and tap F.

mod ball;
mod cuberoom;
mod flat;
mod fx;
mod output;
mod screen;
mod wall;

pub use ball::*;
pub use cuberoom::*;
pub use flat::*;
pub use fx::*;
pub use output::*;
pub use screen::*;
pub use wall::*;
