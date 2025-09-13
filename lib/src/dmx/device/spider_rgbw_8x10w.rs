//! 8x10W RGBW spider light
//!
//! https://www.amazon.com/gp/product/B081H833BG

use crate::color::Rgbw;
use crate::dmx::Device;
use crate::num::Interp;

#[derive(Clone, Copy, Debug)]
pub struct Spider {
    // pub mode: SpiderMode,
    // pub speed: f64,
    pub alpha: f64,

    pub color0: Rgbw,
    pub pos0: f64,

    pub color1: Rgbw,
    pub pos1: f64,
}

impl Device for Spider {
    fn channels(&self) -> usize {
        15
    }

    fn encode(&self, buf: &mut [u8]) {
        let Rgbw(r0, g0, b0, w0) = self.color0;
        let Rgbw(r1, g1, b1, w1) = self.color1;

        buf[0] = self.pos0.byte();
        buf[1] = self.pos1.byte();
        buf[2] = self.alpha.byte();
        // buf[3]: strobe

        buf[4] = r0.byte();
        buf[5] = g0.byte();
        buf[6] = b0.byte();
        buf[7] = w0.byte();

        buf[8] = r1.byte();
        buf[9] = g1.byte();
        buf[10] = b1.byte();
        buf[11] = w1.byte();
        // buf[12]: effect preset
        // buf[13]: effect speed
        // buf[14]: reset
    }
}

// pub enum SpiderMode {
//     Manual,
//     ColorCycle,
//     Auto,
// }

// impl SpiderMode {
//     pub fn byte(&self) -> u8 {
//         match self {
//             SpiderMode::Manual => 0,
//             SpiderMode::ColorCycle => 159,
//             SpiderMode::Auto => 60,
//         }
//     }
// }

impl Default for Spider {
    fn default() -> Self {
        Self {
            // mode: SpiderMode::Manual,
            // speed: 1.0,
            alpha: 1.0,

            color0: Rgbw::BLACK,
            pos0: 0.0,

            color1: Rgbw::BLACK,
            pos1: 0.0,
        }
    }
}
