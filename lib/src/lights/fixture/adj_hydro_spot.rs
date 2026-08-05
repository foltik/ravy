//! ADJ Hydro Spot 1
//!
//! Extended 22CH mode: 16-bit pan/tilt/dimmer/zoom, two color wheels, gobo
//! wheel, two prisms, motorized focus, and two frosts.
//!
//! https://www.adj.com/products/hydro-spot-1

use crate::prelude::*;

#[derive(Clone, Copy, Debug, Component)]
pub struct HydroSpot {
    /// Tilt, `0..1` across 270°.
    pub pitch: f32,
    /// Pan, `0..1` across 540° or 630° (fixture menu setting).
    pub yaw: f32,
    /// Pan/tilt speed, `0..1` = fastest to slowest.
    pub speed: f32,

    /// Raw color wheel 1 byte: 0-9 open, slots to 127, then rotation.
    pub color: u8,
    /// Raw color wheel 2 byte: 0-9 open, slots to 127, then rotation.
    pub color2: u8,
    /// Raw gobo wheel byte: 0-4 open, slots, shakes, then rotation.
    pub gobo: u8,
    /// Raw gobo rotation byte: 0-127 indexing, then cw/ccw spin.
    pub gobo_rot: u8,

    /// Raw shutter byte. See [`Self::SHUTTER_OPEN`], [`Self::strobe`],
    /// [`Self::pulse`], [`Self::random_strobe`].
    pub shutter: u8,
    /// Dimmer.
    pub alpha: f32,

    /// Raw prism byte: 0-5 open, 6-66 5-facet, 67-127 6-facet, then macros.
    pub prism: u8,
    /// Raw prism rotation byte: 0-5 off, 6-128 indexing, then cw/ccw spin.
    pub prism_rot: u8,
    /// Focus, `0..1`.
    pub focus: f32,
    /// Zoom, `0..1` = narrow to wide.
    pub zoom: f32,
    /// Heavy frost, `0..1`.
    pub frost: f32,
    /// Medium frost, `0..1`.
    pub frost2: f32,

    /// Raw dimmer mode byte: 0-20 unit default, then Standard/Stage/TV/
    /// Architectural/Theater/Stage 2, 141-160 dim speed fast to slow.
    pub dimmer_mode: u8,
    /// Raw dim curve byte: 0-20 square, 21-40 linear, 41-60 inverse square,
    /// 61-80 S curve.
    pub dim_curve: u8,
}

impl HydroSpot {
    /// Shutter chart: closed.
    pub const SHUTTER_CLOSED: u8 = 0;
    /// Shutter chart: open, no strobe.
    pub const SHUTTER_OPEN: u8 = 255;

    /// Shutter chart byte for a hardware strobe, `0..1` slow to fast.
    pub fn strobe(fr: f32) -> u8 {
        64 + (fr.clamp(0.0, 1.0) * 31.0) as u8
    }
    /// Shutter chart byte for the pulse effect, `0..1` slow to fast.
    pub fn pulse(fr: f32) -> u8 {
        128 + (fr.clamp(0.0, 1.0) * 31.0) as u8
    }
    /// Shutter chart byte for a random strobe, `0..1` slow to fast.
    pub fn random_strobe(fr: f32) -> u8 {
        192 + (fr.clamp(0.0, 1.0) * 31.0) as u8
    }
}

impl DmxDevice for HydroSpot {
    fn channels(&self) -> usize {
        22
    }

    fn encode(&self, dmx: &mut [u8]) {
        let dmx = &mut dmx[0..self.channels()];
        dmx.fill(0);

        dmx[0..2].copy_from_slice(&self.yaw.coarse_fine()); // 1/2   pan + fine
        dmx[2..4].copy_from_slice(&self.pitch.coarse_fine()); // 3/4   tilt + fine
        dmx[4] = self.color; //          5
        dmx[5] = self.color2; //         6
        dmx[6] = self.gobo; //           7
        dmx[7] = self.gobo_rot; //       8
        dmx[8] = self.shutter; //        9
        dmx[9..11].copy_from_slice(&self.alpha.coarse_fine()); // 10/11 dimmer + fine
        dmx[11] = self.prism; //         12
        dmx[12] = self.prism_rot; //     13
        dmx[13] = self.focus.byte(); //  14
        dmx[14..16].copy_from_slice(&self.zoom.coarse_fine()); // 15/16 zoom + fine
        dmx[16] = self.frost.byte(); //  17
        dmx[17] = self.frost2.byte(); // 18
        dmx[18] = self.dimmer_mode; //   19
        dmx[19] = self.dim_curve; //     20
        // 21 pan/tilt speed: values >= 226 are blackout modes, stay in 0-225
        dmx[20] = (self.speed.clamp(0.0, 1.0) * 225.0) as u8;
        // 22 special functions stays 0 = no function: fan control, resets,
        // and refresh rate overrides live here, deliberately not exposed
    }
}

impl Default for HydroSpot {
    fn default() -> Self {
        Self {
            pitch: 0.5,
            yaw: 0.0,
            speed: 0.0,
            color: 0,
            color2: 0,
            gobo: 0,
            gobo_rot: 0,
            shutter: Self::SHUTTER_OPEN,
            alpha: 1.0,
            prism: 0,
            prism_rot: 0,
            focus: 0.5,
            zoom: 0.5,
            frost: 0.0,
            frost2: 0.0,
            dimmer_mode: 0,
            dim_curve: 30, // linear
        }
    }
}

impl RdmDevice for HydroSpot {
    const MANUFACTURER: u16 = 0x1900;
    const MODEL: u16 = 0x1581;
    /// "Extended 22". The others are "Basic 17" and "Standard 20".
    const PERSONALITY: u8 = 3;
}
