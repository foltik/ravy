//! 12x3W RGBW par light
//!
//! https://www.aliexpress.com/w/wholesale-12x3w-rgbw-dmx-led-par-light.html

use crate::color::Rgbw;
use crate::dmx::Device;
use crate::num::Interp;

#[derive(Clone, Copy, Debug)]
pub struct Par {
    pub color: Rgbw,
}

impl Device for Par {
    fn channels(&self) -> usize {
        8
    }

    fn encode(&self, buf: &mut [u8]) {
        let Rgbw(r, g, b, w) = self.color;

        // buf[0]: e
        // buf[1]: ?
        // buf[2]: ?
        buf[3] = 1.0.byte();
        buf[4] = r.byte();
        buf[5] = g.byte();
        buf[6] = b.byte();
        buf[7] = w.byte();
    }
}

impl Default for Par {
    fn default() -> Self {
        Self { color: Rgbw::BLACK }
    }
}
