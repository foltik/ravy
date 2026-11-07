//! Chauvet Hurricane Haze 4D hazer, 2 channel mode.
//!
//! https://www.chauvetdj.com/wp-content/uploads/2016/09/Hurricane_Haze_4D_UM_Rev4.pdf

use crate::dmx::DmxDevice;
use crate::math::Interp;

#[derive(Clone, Copy, Debug, Default)]
pub struct Hazer {
    /// Fan speed, `0..1`.
    pub fan: f32,
    /// Haze output, `0..1`.
    pub haze: f32,
}

impl DmxDevice for Hazer {
    fn channels(&self) -> usize {
        2
    }

    fn encode(&self, buf: &mut [u8]) {
        buf[0] = self.fan.byte();
        buf[1] = self.haze.byte();
    }
}
