//! 30W RGB scanning laser
//!
//! https://www.amazon.com/gp/product/B09LVGQ2GY

use crate::color::Rgb;
use crate::dmx::Device;
use crate::num::Interp;

#[derive(Clone, Copy, Debug)]
pub struct LaserArray {
    pub angle: f32,
    pub brightness: f32,
    pub color: LaserColor,
}

#[derive(Clone, Copy, Debug)]
pub enum LaserColor {
    Red,
    Green,
    Blue,
    Yellow,
    Pink,
    Cyan,
    White,
}

impl LaserColor {
    pub fn byte(self) -> u8 {
        match self {
            Self::Red => 30,
            Self::Green => 42,
            Self::Blue => 68,
            Self::Yellow => 98,
            Self::Pink => 130,
            Self::Cyan => 158,
            Self::White => 188,
        }
    }
}

impl From<u8> for LaserColor {
    fn from(v: u8) -> Self {
        match v {
            0 => Self::Red,
            42 => Self::Green,
            68 => Self::Blue,
            98 => Self::Yellow,
            130 => Self::Pink,
            158 => Self::Cyan,
            /* 188 */ _ => Self::White,
        }
    }
}

impl From<Rgb> for LaserColor {
    fn from(value: Rgb) -> Self {
        match value {
            Rgb::RED => Self::Red,
            Rgb::LIME => Self::Green,
            Rgb::BLUE => Self::Blue,
            Rgb::PEA => Self::Yellow,
            Rgb::MINT => Self::Cyan,
            Rgb::WHITE => Self::White,
            _ => Self::White,
        }
    }
}

impl Default for LaserArray {
    fn default() -> Self {
        Self { angle: 0.0, brightness: 1.0, color: LaserColor::White }
    }
}

impl Device for LaserArray {
    fn channels(&self) -> usize {
        5
    }

    fn encode(&self, buf: &mut [u8]) {
        let min = 0.4;
        let max = 0.52;

        buf[0] = (min + self.angle * (max - min)).byte();
        buf[1] = 0;
        buf[2] = 0;
        buf[3] = self.brightness.byte();
        buf[4] = self.color.byte();
    }
}
