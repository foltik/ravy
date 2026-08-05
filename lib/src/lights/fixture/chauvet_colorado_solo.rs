//! Chauvet COLORado 1 Solo
//!
//! STDY personality (17ch): 16-bit dimmer + RGBW, 8-bit zoom.
//!
//! https://www.chauvetprofessional.com/products/colorado-1-solo/

use crate::prelude::*;

#[derive(Clone, Copy, Debug, Component)]
pub struct ColoradoSolo {
    pub color: Rgbw,
    pub alpha: f32,
    /// Zoom, `0..1`.
    pub zoom: f32,
}

impl DmxDevice for ColoradoSolo {
    fn channels(&self) -> usize {
        17
    }

    fn encode(&self, dmx: &mut [u8]) {
        let dmx = &mut dmx[0..self.channels()];
        dmx.fill(0);

        dmx[0..2].copy_from_slice(&self.alpha.coarse_fine()); // 1/2 dimmer + fine

        let Rgbw(r, g, b, w) = self.color;
        dmx[2..4].copy_from_slice(&r.coarse_fine()); // 3-10 rgbw + fine
        dmx[4..6].copy_from_slice(&g.coarse_fine());
        dmx[6..8].copy_from_slice(&b.coarse_fine());
        dmx[8..10].copy_from_slice(&w.coarse_fine());

        // 11 color macro, 12 strobe, 13/14 auto programs stay 0 = no function
        dmx[14] = self.zoom.byte(); // 15
        // 16 zoom control and 17 dimmer speed stay 0: nonzero ch17 kills
        // output on this firmware despite what the chart says
    }
}

impl RdmDevice for ColoradoSolo {
    const MANUFACTURER: u16 = 0x21A4;
    const MODEL: u16 = 0x0464;
    /// "STDY". The others are HSIC 9, SSP 9, TOUR 12, TR16 17.
    const PERSONALITY: u8 = 5;
}

impl Default for ColoradoSolo {
    fn default() -> Self {
        Self { color: Rgbw::BLACK, alpha: 1.0, zoom: 0.5 }
    }
}
