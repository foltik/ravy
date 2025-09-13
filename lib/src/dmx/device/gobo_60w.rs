//! 60W RGBW moving head
//!
//! <TODO: amazon link>

use crate::dmx::Device;
use crate::num::Interp;

#[derive(Clone, Copy, Debug, Default)]
pub struct Gobo {
    pub pan: f32,
    pub tilt: f32,
    pub color: f32,
    pub pattern: f32,
    pub strobe: f32,
    pub alpha: f32,
    pub speed: f32,
    pub auto: f32,
}

impl Device for Gobo {
    fn channels(&self) -> usize {
        9
    }

    fn encode(&self, buf: &mut [u8]) {
        buf[0] = self.pan.byte();
        buf[1] = self.tilt.byte();
        buf[2] = self.color.byte();
        buf[3] = self.pattern.byte();
        buf[4] = self.strobe.byte();
        buf[5] = self.alpha.byte();
        buf[6] = self.speed.byte();
        buf[7] = self.auto.byte();
        buf[8] = 0; // reset
    }
}
