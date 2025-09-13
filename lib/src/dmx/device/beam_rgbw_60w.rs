//! 90W RGBW moving head
//!
//! TODO: amazon link

use crate::color::Rgbw;
use crate::dmx::Device;
use crate::num::Interp;

#[derive(Clone, Copy, Debug)]
pub struct Beam {
    pub mode: BeamMode,
    pub ring: BeamRing,

    pub pitch: f64,
    pub yaw: f64,
    pub speed: f64,

    pub color: Rgbw,
    pub alpha: f64,
}

#[derive(Clone, Copy, Debug)]
pub enum BeamMode {
    Manual,
    ColorCycle,
    Auto,
}

#[derive(Clone, Copy, Debug, Default)]
pub enum BeamRing {
    #[default]
    Off,

    Red,
    Green,
    Blue,
    Yellow,
    Purple,
    Teal,
    White,

    RedYellow,
    RedPurple,
    RedWhite,

    GreenYellow,
    GreenBlue,
    GreenWhite,

    BluePurple,
    BlueTeal,
    BlueWhite,

    Cycle,
    Raw(u8),
}

impl Device for Beam {
    fn channels(&self) -> usize {
        15
    }

    fn encode(&self, buf: &mut [u8]) {
        let Rgbw(r, g, b, w) = self.color;

        buf[0] = self.yaw.byte();
        // buf[0] = (self.yaw * (2.0 / 3.0)).byte();
        // buf[0] = self.yaw.lerp((1.0 / 3.0)..1.0).byte();
        // buf[1]: yaw fine
        buf[2] = self.pitch.byte();
        // buf[3]: pitch fine
        buf[4] = (1.0 - self.speed).byte();
        buf[5] = self.alpha.byte();
        // buf[6]: strobe
        buf[7] = r.byte();
        buf[8] = g.byte();
        buf[9] = b.byte();
        buf[10] = w.byte();
        // buf[11]: color preset
        buf[12] = self.mode.byte();
        // buf[13]: auto pitch/yaw, reset
        buf[14] = self.ring.byte();
    }
}

impl BeamRing {
    pub fn byte(&self) -> u8 {
        match self {
            BeamRing::Off => 0,

            BeamRing::Red => 4,
            BeamRing::Green => 22,
            BeamRing::Blue => 36,
            BeamRing::Yellow => 56,
            BeamRing::Purple => 74,
            BeamRing::Teal => 84,
            BeamRing::White => 104,

            BeamRing::RedYellow => 116,
            BeamRing::RedPurple => 128,
            BeamRing::RedWhite => 140,

            BeamRing::GreenYellow => 156,
            BeamRing::GreenBlue => 176,
            BeamRing::GreenWhite => 192,

            BeamRing::BluePurple => 206,
            BeamRing::BlueTeal => 216,
            BeamRing::BlueWhite => 242,

            BeamRing::Cycle => 248,
            BeamRing::Raw(i) => *i,
        }
    }
}

impl BeamMode {
    pub fn byte(&self) -> u8 {
        match self {
            BeamMode::Manual => 0,
            BeamMode::ColorCycle => 159,
            BeamMode::Auto => 60,
        }
    }
}

impl Default for Beam {
    fn default() -> Self {
        Self {
            mode: BeamMode::Manual,
            ring: BeamRing::Off,

            pitch: 0.0,
            yaw: 0.33,
            speed: 1.0,

            color: Rgbw::BLACK,
            alpha: 1.0,
        }
    }
}
