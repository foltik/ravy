//! 35W RGB strobe light
//!
//! https://www.amazon.com/gp/product/B01MZYQJSA

use crate::color::Rgb;
use crate::dmx::Device;
use crate::num::Interp;

#[derive(Clone, Copy, Debug)]
pub struct Strobe {
    // pub mode: StrobeMode,
    pub color: Rgb,
    pub alpha: f64,
}

impl Device for Strobe {
    fn channels(&self) -> usize {
        6
    }

    fn encode(&self, buf: &mut [u8]) {
        let Rgb(r, g, b) = self.color;

        buf[0] = self.alpha.byte();
        // buf[1]: mode
        buf[2] = r.byte();
        buf[3] = g.byte();
        buf[4] = b.byte();
        // buf[5]: sound control
    }
}

// pub enum StrobeMode {
//     Manual,
//     ColorCycle,
//     Auto,
// }

// impl StrobeMode {
//     pub fn byte(&self) -> u8 {
//         match self {
//             StrobeMode::Manual => 0,
//             StrobeMode::ColorCycle => 159,
//             StrobeMode::Auto => 60,
//         }
//     }
// }

impl Default for Strobe {
    fn default() -> Self {
        Self {
            // mode: StrobeMode::Manual,
            color: Rgb::BLACK,
            alpha: 1.0,
        }
    }
}
