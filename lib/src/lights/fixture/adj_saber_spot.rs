//! Eliminator Stealth Beam
//!
//! https://www.adj.com/products/saber-spot-rgbw

use crate::lights::SpotDevice;
use crate::prelude::*;

#[derive(Clone, Copy, Debug, Component)]
pub struct SaberSpot {
    pub color: Rgbw,
    pub alpha: f32,
}

impl SpotDevice for SaberSpot {
    fn name(&self) -> &'static str {
        "ADJ Saber Spot"
    }
    fn intensity(&self) -> f32 {
        1_000_000.0
    }
    fn range(&self) -> f32 {
        10.0
    }
    fn beam_angle(&self) -> f32 {
        2.0
    }
    fn model(&self) -> &'static [u8] {
        include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/fixtures/ADJ_Saber_Spot.glb"))
    }
    fn model_path(&self) -> &'static str {
        "fixtures/ADJ_Saber_Spot.glb"
    }

    fn color(&self) -> Rgbw {
        self.color
    }
}

impl DmxDevice for SaberSpot {
    fn channels(&self) -> usize {
        8
    }

    fn encode(&self, dmx: &mut [u8]) {
        dmx.fill(0);

        let Rgbw(r, g, b, w) = self.color;
        dmx[0] = r.byte();
        dmx[1] = g.byte();
        dmx[2] = b.byte();
        dmx[3] = w.byte();
        dmx[4] = 255; // strobe: led on
        dmx[5] = self.alpha.byte();
    }
}

impl Default for SaberSpot {
    fn default() -> Self {
        Self { color: Rgbw::BLACK, alpha: 1.0 }
    }
}
