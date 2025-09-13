//! 60W RGBW moving head
//!
//! https://www.amazon.com/gp/product/B089QGPJ2L
//! https://www.aliexpress.com/w/wholesale-Beam-60W-LED-Moving-Head-RGBW-4-IN-1-Stage-Lightin.html

use crate::color::Rgbw;
use crate::dmx::Device;
use crate::num::Interp;

#[derive(Clone, Copy, Debug)]
pub struct BigBeam {
    pub pitch: f64,
    pub yaw: f64,
    pub speed: f64,
    pub color: Rgbw,
    pub alpha: f64,
    pub strobe: f64,
}

impl Device for BigBeam {
    fn channels(&self) -> usize {
        13
    }

    fn encode(&self, buf: &mut [u8]) {
        let Rgbw(r, g, b, w) = self.color;

        buf[0] = self.yaw.byte();
        // buf[0] = (self.yaw * (2.0 / 3.0)).byte();
        // buf[0] = self.yaw.lerp((1.0 / 3.0)..1.0).byte();
        buf[1] = self.pitch.inv().byte();
        buf[2] = (1.0 - self.speed).byte();
        buf[3] = self.alpha.byte();
        buf[4] = self.strobe.byte();
        buf[5] = r.byte();
        buf[6] = g.byte();
        buf[7] = b.byte();
        buf[8] = w.byte();
    }
}

impl Default for BigBeam {
    fn default() -> Self {
        Self {
            pitch: 0.0,
            yaw: 0.33,
            speed: 1.0,
            strobe: 0.0,

            color: Rgbw::BLACK,
            alpha: 1.0,
        }
    }
}
