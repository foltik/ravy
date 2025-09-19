//! Eliminator Stealth Beam
//!
//! https://www.adj.com/cdn/shop/files/Stealth_Beam__1st_Ed.pdf

use crate::lights::MovingHeadDevice;
use crate::prelude::*;
use crate::sim::motor::MotorDynamics;

#[derive(Clone, Copy, Debug, Component)]
pub struct EliminatorStealthBeam {
    pub pitch: f32,
    pub yaw: f32,
    pub color: Rgbw,
    pub alpha: f32,
    pub strobe: f32,
}

impl MovingHeadDevice for EliminatorStealthBeam {
    fn name(&self) -> &'static str {
        "Eliminator Stealth Beam"
    }
    fn intensity(&self) -> f32 {
        10_000_000.0
    }
    fn range(&self) -> f32 {
        10.0
    }
    fn beam_angle(&self) -> f32 {
        5.5
    }
    fn model(&self) -> &'static [u8] {
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/fixtures/EliminatorStealthBeam.glb"
        ))
    }
    fn model_path(&self) -> &'static str {
        "fixtures/EliminatorStealthBeam.glb"
    }
    fn pitch_dynamics(&self) -> MotorDynamics {
        MotorDynamics {
            θ_min: 0.0,
            θ_max: 180.0,
            v_max: 450.0,
            a_max: 3_000.9,
            j_max: 100_000.0,
            linear_threshold: 15.0,
            linear_gain: 5.0,
        }
    }
    fn yaw_dynamics(&self) -> MotorDynamics {
        MotorDynamics {
            θ_min: 0.0,
            θ_max: -540.0,
            v_max: 480.0,
            a_max: 1_200.0,
            j_max: 100_000.0,
            linear_threshold: 80.0,
            linear_gain: 1.5,
        }
    }

    fn pitch(&self) -> f32 {
        self.pitch
    }
    fn yaw(&self) -> f32 {
        self.yaw
    }
    fn color(&self) -> Rgbw {
        self.color
    }
}

impl DmxDevice for EliminatorStealthBeam {
    fn channels(&self) -> usize {
        16
    }

    fn encode(&self, dmx: &mut [u8]) {
        dmx.fill(0);

        dmx[0] = self.yaw.byte();
        dmx[2] = self.pitch.byte();

        let Rgbw(r, g, b, w) = self.color;
        dmx[4] = r.byte();
        dmx[5] = g.byte();
        dmx[6] = b.byte();
        dmx[7] = w.byte();

        dmx[9] = self.alpha.byte();
        dmx[10] = 255; // strobe: shutter open
    }
}

impl Default for EliminatorStealthBeam {
    fn default() -> Self {
        Self { pitch: 0.5, yaw: 0.0, strobe: 0.0, color: Rgbw::BLACK, alpha: 1.0 }
    }
}
