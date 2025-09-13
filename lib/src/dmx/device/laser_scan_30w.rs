//! 30W RGB scanning laser
//!
//! https://www.amazon.com/gp/product/B09LVGQ2GY

use crate::color::Rgb;
use crate::dmx::Device;
use crate::num::Interp;

#[derive(Clone, Copy, Debug)]
pub struct Laser {
    pub on: bool,

    pub pattern: LaserPattern,
    pub color: LaserColor,
    pub stroke: LaserStroke,

    pub rotate: f64,
    pub xflip: f64,
    pub yflip: f64,
    pub x: f64,
    pub y: f64,
    pub size: f64,
}

#[derive(Clone, Copy, Debug)]
pub enum LaserColor {
    Raw(u8),

    Rgb(bool, bool, bool),
    Mix(usize),
}

#[derive(Clone, Copy, Debug)]
pub enum LaserStroke {
    Solid(f64),
    Dots(f64),
}

#[derive(Clone, Copy, Debug)]
pub enum LaserPattern {
    Raw(u8),

    Square,
    SquareWide,
    SquareXWide,
    SquareBlock,
    Circle,
    CircleWide,
    CircleDash,
    CircleQuad,
    CircleCircle,
    CircleSquare,
    CircleX,
    CircleY,
    LineX,
    LineY,
    LineXY,
    LineDX,
    LineDY,
    Line2X,
    Line2Y,
    LinePenta,
    LineStair,
    Tri,
    TriX,
    TriY,
    Tri3d,
    TriTri,
    TriCircle,
    TriWing,
    TriArch,
    Penta,
    Squig1,
    Squig2,
    Three,
    Two,
    One,
    Music,
    Tree,
    Star,
    Sin,
    Heart,
    Elephant,
    Apple,
    Plus,
    PlusOval,
    PlusArrow,
    PlusDia,
    Arrow,
    ArrowInvert,
    Hourglass1,
    Hourglass2,
}

impl LaserColor {
    pub const RED: Self = LaserColor::Rgb(true, false, false);
    pub const GREEN: Self = LaserColor::Rgb(false, true, false);
    pub const BLUE: Self = LaserColor::Rgb(false, false, true);
    pub const RGB: Self = LaserColor::Rgb(true, true, true);

    pub fn byte(self) -> u8 {
        match self {
            LaserColor::Raw(i) => i,

            LaserColor::Rgb(r, g, b) => match (r, g, b) {
                (true, false, false) => 76,  // R
                (false, true, false) => 98,  // G
                (false, false, true) => 116, // B

                (true, true, false) => 86,  // RG
                (true, false, true) => 122, // RB
                (false, true, true) => 104, // BG

                (true, true, true) => 64,   // RGB
                (false, false, false) => 0, // unused
            },

            LaserColor::Mix(i) => match i % 7 {
                0 => 0,
                1 => 10,
                2 => 20,
                3 => 28,
                4 => 38,
                5 => 50,
                6 => 58,
                _ => 0,
            },
        }
    }

    pub fn from_rgb(rgb: Rgb) -> Self {
        match rgb {
            Rgb::RED => Self::RED,
            Rgb::LIME => Self::GREEN,
            Rgb::BLUE => Self::BLUE,
            _ => Self::Rgb(true, true, true),
        }
    }
}

impl LaserStroke {
    pub fn byte(self) -> u8 {
        match self {
            LaserStroke::Solid(fr) => fr.inv().lerp(0..127) as u8,
            LaserStroke::Dots(fr) => fr.inv().lerp(128..255) as u8,
        }
    }
}

impl LaserPattern {
    pub fn byte(self) -> u8 {
        match self {
            LaserPattern::Raw(i) => i,

            LaserPattern::Square => 0,
            LaserPattern::SquareWide => 232,
            LaserPattern::SquareXWide => 255,
            LaserPattern::SquareBlock => 224,
            LaserPattern::Circle => 6,
            LaserPattern::CircleWide => 82,
            LaserPattern::CircleDash => 138,
            LaserPattern::CircleQuad => 144,
            LaserPattern::CircleCircle => 146,
            LaserPattern::CircleSquare => 162,
            LaserPattern::CircleX => 26,
            LaserPattern::CircleY => 32,
            LaserPattern::LineX => 12,
            LaserPattern::LineY => 16,
            LaserPattern::LineXY => 22,
            LaserPattern::LineDX => 46,
            LaserPattern::LineDY => 52,
            LaserPattern::Line2X => 56,
            LaserPattern::Line2Y => 62,
            LaserPattern::LinePenta => 172,
            LaserPattern::LineStair => 182,
            LaserPattern::Tri => 36,
            LaserPattern::TriX => 42,
            LaserPattern::TriY => 100,
            LaserPattern::Tri3d => 152,
            LaserPattern::TriTri => 168,
            LaserPattern::TriCircle => 214,
            LaserPattern::TriWing => 218,
            LaserPattern::TriArch => 224,
            LaserPattern::Penta => 186,
            LaserPattern::Squig1 => 66,
            LaserPattern::Squig2 => 72,
            LaserPattern::Three => 94,
            LaserPattern::Two => 112,
            LaserPattern::One => 116,
            LaserPattern::Music => 76,
            LaserPattern::Tree => 86,
            LaserPattern::Star => 104,
            LaserPattern::Sin => 108,
            LaserPattern::Heart => 122,
            LaserPattern::Elephant => 126,
            LaserPattern::Apple => 132,
            LaserPattern::Plus => 156,
            LaserPattern::PlusOval => 194,
            LaserPattern::PlusArrow => 196,
            LaserPattern::PlusDia => 250,
            LaserPattern::Arrow => 204,
            LaserPattern::ArrowInvert => 228,
            LaserPattern::Hourglass1 => 238,
            LaserPattern::Hourglass2 => 210,
        }
    }
}

impl Default for Laser {
    fn default() -> Self {
        Self {
            on: false,
            pattern: LaserPattern::Raw(0),
            stroke: LaserStroke::Solid(1.0),
            color: LaserColor::RGB,

            rotate: 0.0,
            xflip: 0.0,
            yflip: 0.0,
            x: 0.0,
            y: 0.0,
            size: 0.0,
        }
    }
}

impl Device for Laser {
    fn channels(&self) -> usize {
        10
    }

    fn encode(&self, buf: &mut [u8]) {
        buf[0] = if self.on { 64 } else { 0 };
        buf[1] = self.pattern.byte();
        buf[2] = self.rotate.lerp(0..127) as u8;
        buf[3] = self.yflip.lerp(0..127) as u8;
        buf[4] = self.xflip.lerp(0..127) as u8;
        buf[5] = self.x.lerp(0..127) as u8;
        buf[6] = self.y.lerp(0..127) as u8;
        buf[7] = self.size.lerp(0..63) as u8;
        buf[8] = self.color.byte();
        buf[9] = self.stroke.byte();
    }
}
