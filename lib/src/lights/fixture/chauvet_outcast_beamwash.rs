//! Chauvet Rogue Outcast 1 BeamWash
//!
//! 37CH personality: no pixel access, ring as one 16-bit RGB cell, center as
//! one 16-bit RGBW cell. Set with `rdm.py personality UID 7`.
//!
//! https://www.chauvetprofessional.com/products/rogue-outcast-1-beamwash/

use crate::prelude::*;

#[derive(Clone, Copy, Debug, Component)]
pub struct OutcastBeamwash {
    /// Tilt, `0..1` across 270°.
    pub pitch: f32,
    /// Pan, `0..1` across 540°.
    pub yaw: f32,
    /// Pan/tilt speed, `0..1` = fastest to slowest.
    pub speed: f32,

    /// Ring color. The ring has no white emitter: white is folded into RGB.
    pub ring: Rgbw,
    /// Ring dimmer.
    pub ring_alpha: f32,
    /// Raw ring strobe chart byte. 0-19 shutter closed, 20-24 open.
    pub ring_strobe: u8,

    /// Center color.
    pub center: Rgbw,
    /// Center dimmer.
    pub center_alpha: f32,
    /// Raw center strobe chart byte. 0-19 shutter closed, 20-24 open.
    pub center_strobe: u8,

    /// Zoom, `0..1` = in to out.
    pub zoom: f32,
}

impl OutcastBeamwash {
    /// Strobe chart: shutter closed.
    pub const SHUTTER_CLOSED: u8 = 0;
    /// Strobe chart: shutter open, no strobe.
    pub const SHUTTER_OPEN: u8 = 22;
}

impl DmxDevice for OutcastBeamwash {
    fn channels(&self) -> usize {
        37
    }

    fn encode(&self, dmx: &mut [u8]) {
        let dmx = &mut dmx[0..self.channels()];
        dmx.fill(0);

        dmx[0..2].copy_from_slice(&self.yaw.coarse_fine()); // 1/2   pan + fine
        dmx[2..4].copy_from_slice(&self.pitch.coarse_fine()); // 3/4   tilt + fine
        dmx[4] = self.speed.byte(); // 5     pan/tilt speed, 0 = fastest
        // 6 CTC, 7/8 preset colors, 9 pattern, 10-12 LED macro, and
        // 13-15 background stay 0 = no function
        dmx[15..17].copy_from_slice(&self.ring_alpha.coarse_fine()); // 16/17 ring dimmer + fine
        dmx[17..19].copy_from_slice(&self.center_alpha.coarse_fine()); // 18/19 center dimmer + fine
        dmx[19] = self.ring_strobe; //         20
        dmx[20] = self.center_strobe; //       21
        dmx[21] = self.zoom.byte(); //         22
        // 23 control stays 0 = no function: dimmer curve/speed are latching
        // menu settings with reset ranges nearby, set them from the menu

        let Rgbw(r, g, b, w) = self.ring;
        dmx[23..25].copy_from_slice(&r.max(w).coarse_fine()); // 24-29 ring rgb + fine
        dmx[25..27].copy_from_slice(&g.max(w).coarse_fine());
        dmx[27..29].copy_from_slice(&b.max(w).coarse_fine());

        let Rgbw(r, g, b, w) = self.center;
        dmx[29..31].copy_from_slice(&r.coarse_fine()); // 30-37 center rgbw + fine
        dmx[31..33].copy_from_slice(&g.coarse_fine());
        dmx[33..35].copy_from_slice(&b.coarse_fine());
        dmx[35..37].copy_from_slice(&w.coarse_fine());
    }
}

impl Default for OutcastBeamwash {
    fn default() -> Self {
        Self {
            pitch: 0.5,
            yaw: 0.0,
            speed: 0.0,
            ring: Rgbw::BLACK,
            ring_alpha: 1.0,
            ring_strobe: Self::SHUTTER_OPEN,
            center: Rgbw::BLACK,
            center_alpha: 1.0,
            center_strobe: Self::SHUTTER_OPEN,
            zoom: 0.5,
        }
    }
}

impl RdmDevice for OutcastBeamwash {
    const MANUFACTURER: u16 = 0x21A4;
    const MODEL: u16 = 0x0731;
    const PERSONALITY: u8 = 7;
}
