//! 18W RGB light bar
//!
//! https://www.amazon.com/gp/product/B0045EP4WG

use crate::color::Rgbw;
use crate::dmx::Device;
use crate::num::Interp;

#[derive(Default, Clone, Copy, Debug)]
pub struct Bar {
    pub color: Rgbw,
    pub alpha: f64,
}

impl Device for Bar {
    fn channels(&self) -> usize {
        7
    }

    fn encode(&self, buf: &mut [u8]) {
        let Rgbw(r, g, b, w) = self.color;

        if r == g && g == b && w == 0.0 {
            // ignore Rgb::WHITE since we have to white channel
            buf[0] = 0;
            buf[1] = 0;
            buf[2] = 0;
        } else {
            buf[0] = r.byte();
            buf[1] = g.byte();
            buf[2] = b.byte();
        }

        buf[0] = r.byte();
        buf[1] = g.byte();
        buf[2] = b.byte();
        // buf[3]: preset colors
        // buf[4]: strobe
        // buf[5]: mode
        buf[6] = self.alpha.byte();
    }
}

// #[derive(Default, Clone, Copy, Debug)]
// pub enum BarMode {
//     #[default]
//     Manual,
//     ColorCycle,
//     Auto,
// }

// impl BarMode {
//     pub fn byte(&self) -> u8 {
//         match self {
//             BarMode::Manual => 0,
//             BarMode::ColorCycle => 159,
//             BarMode::Auto => 60,
//         }
//     }
// }
