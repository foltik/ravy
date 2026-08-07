//! Chauvet Rogue Outcast 1 BeamWash
//!
//! 37CH personality: no pixel access, ring as one 16-bit RGB cell, center as
//! one 16-bit RGBW cell. Set with `rdm.py personality UID 7`.
//!
//! https://www.chauvetprofessional.com/products/rogue-outcast-1-beamwash/

use crate::gdtf::{GdtfDevice, motion};
use crate::prelude::*;

#[derive(Clone, Copy, Debug, Component)]
pub struct OutcastBeamwash {
    /// Tilt, `0..1` across 260°.
    pub pitch: f32,
    /// Pan, `0..1` across 540°, turning the opposite way to the Hydro.
    pub yaw: f32,
    /// Pan/tilt speed, `0..1` = fastest to slowest.
    pub speed: f32,

    /// Ring color. The ring has no white emitter: white is folded into RGB.
    pub ring: Rgbw,
    /// Ring dimmer.
    pub ring_alpha: f32,
    /// Raw ring strobe chart byte. 0-19 shutter closed, 20-24 open.
    pub ring_strobe: u8,

    /// Animation the fixture runs across the ring's 12 segments.
    pub ring_pattern: OutcastBeamwashRingPattern,
    /// Ring pattern speed, `0..1` = slowest to fastest.
    pub ring_pattern_speed: f32,

    /// Center color.
    pub center: Rgbw,
    /// Center dimmer.
    pub center_alpha: f32,
    /// Raw center strobe chart byte. 0-19 shutter closed, 20-24 open.
    pub center_strobe: u8,

    /// Zoom, `0..1` = in to out.
    pub zoom: f32,
}

/// The LED ring patterns worth keeping, of the 42 the fixture has. `Rotate*` is
/// named for how many lit segments it spaces around the ring, except the first
/// two, which are one block of three and one of five.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum OutcastBeamwashRingPattern {
    #[default]
    Off,
    RotateSmall,
    RotateBig,
    RotateQuadrant,
    RotateTwo,
    RotateThree,
    RotateFour,
    RotateSix,
    RotateTwelve,
    FlipZigzag,
    FlipRotate,
    FlipRandom,
    BounceX,
    BounceY,
}

impl OutcastBeamwashRingPattern {
    pub const ALL: [Self; 14] = [
        Self::Off,
        Self::RotateSmall,
        Self::RotateBig,
        Self::RotateQuadrant,
        Self::RotateTwo,
        Self::RotateThree,
        Self::RotateFour,
        Self::RotateSix,
        Self::RotateTwelve,
        Self::FlipZigzag,
        Self::FlipRotate,
        Self::FlipRandom,
        Self::BounceX,
        Self::BounceY,
    ];

    pub fn byte(self) -> u8 {
        match self {
            Self::Off => 0,
            Self::RotateSmall => 21,
            Self::RotateBig => 31,
            Self::RotateQuadrant => 66,
            Self::RotateTwo => 86,
            Self::RotateThree => 91,
            Self::RotateFour => 96,
            Self::RotateSix => 106,
            Self::RotateTwelve => 101,
            Self::FlipZigzag => 81,
            Self::FlipRotate => 71,
            Self::FlipRandom => 61,
            Self::BounceX => 46,
            Self::BounceY => 41,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Off => "none",
            Self::RotateSmall => "rotate small",
            Self::RotateBig => "rotate big",
            Self::RotateQuadrant => "rotate quadrant",
            Self::RotateTwo => "rotate two",
            Self::RotateThree => "rotate three",
            Self::RotateFour => "rotate four",
            Self::RotateSix => "rotate six",
            Self::RotateTwelve => "rotate twelve",
            Self::FlipZigzag => "flip zigzag",
            Self::FlipRotate => "flip rotate",
            Self::FlipRandom => "flip random",
            Self::BounceX => "bounce x",
            Self::BounceY => "bounce y",
        }
    }
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
        // 6 CTC, 7/8 preset colors, 9 pattern, 12 macro delay, and 13-15
        // background stay 0 = no function
        dmx[9] = self.ring_pattern.byte(); //   10
        // 11 ring pattern speed: 0 is the fastest, 127 the slowest
        dmx[10] = (127.0 * (1.0 - self.ring_pattern_speed.clamp(0.0, 1.0))) as u8;
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
            ring_pattern: OutcastBeamwashRingPattern::Off,
            ring_pattern_speed: 0.5,
            center: Rgbw::BLACK,
            center_alpha: 1.0,
            center_strobe: Self::SHUTTER_OPEN,
            zoom: 0.5,
        }
    }
}

impl GdtfDevice for OutcastBeamwash {
    const KIND: &'static str = "OutcastBeamwash";
    const MODE: &'static str = "37Ch Mode";

    /// Pan runs backwards, and tilt is 260 rather than the round 270.
    const TRAVEL: [f32; 2] = [-540.0, 260.0];

    /// Measured by eye against the real head. The firmware disagrees: its motor
    /// board counts 1478*16384 units across pan's 540 deg and 711*16384 across
    /// tilt's 260 deg, and caps them at 6192 and 8192 units per 1 ms tick, which
    /// works out at 138 deg/s pan and 183 deg/s tilt. Both are far below what is
    /// fitted here, so one of the two is wrong; a stopwatch on a full sweep
    /// would settle it.
    fn motion() -> [motion::Params; 3] {
        [
            motion::Params {
                v_max: 500.0,
                a_max: 800.0,
                j_max: 50_000.0,
                small: 0.0,
                ..motion::Params::pan()
            },
            motion::Params {
                v_max: 470.0,
                a_max: 1500.0,
                j_max: 70_000.0,
                small: 0.0,
                ..motion::Params::tilt()
            },
            motion::Params { v_max: 180.0, a_max: 500.0, j_max: 0.0, ..motion::Params::zoom() },
        ]
    }
}

impl RdmDevice for OutcastBeamwash {
    const MANUFACTURER: u16 = 0x21A4;
    const MODEL: u16 = 0x0731;
    const PERSONALITY: u8 = 7;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patterns_are_distinct() {
        let mut bytes: Vec<u8> = OutcastBeamwashRingPattern::ALL.map(|p| p.byte()).to_vec();
        bytes.sort();
        let count = bytes.len();
        bytes.dedup();
        assert_eq!(bytes.len(), count);
    }
}
