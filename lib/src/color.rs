use crate::prelude::*;

/// An (r, g, b) color.
#[derive(Default, Clone, Copy, Debug, PartialEq)]
pub struct Rgb(pub f32, pub f32, pub f32);

impl Rgb {
    /// Generate an HSV color. All parameters range from `0.0..1.0`.
    pub fn hsv(hue: f32, sat: f32, val: f32) -> Self {
        let r = val * sat.lerp(1.0..(((hue + (3.0 / 3.0)).fract() * 6.0 - 3.0).abs() - 1.0).clamp(0.0, 1.0));
        let g = val * sat.lerp(1.0..(((hue + (2.0 / 3.0)).fract() * 6.0 - 3.0).abs() - 1.0).clamp(0.0, 1.0));
        let b = val * sat.lerp(1.0..(((hue + (1.0 / 3.0)).fract() * 6.0 - 3.0).abs() - 1.0).clamp(0.0, 1.0));

        Self(r, g, b)
    }

    /// CIE luminance (assuming linear sRGB)
    pub fn luminance(&self) -> f32 {
        (0.2126 * self.0) + (0.7152 * self.1) + (0.0722 * self.2)
    }
}

/// An (r, g, b, w) color.
#[derive(Default, Clone, Copy, PartialEq, Debug)]
pub struct Rgbw(pub f32, pub f32, pub f32, pub f32);

/// Conversions
mod conv {
    use super::*;

    impl From<Rgbw> for Rgb {
        /// Convert Rgbw -> Rgb.
        fn from(Rgbw(mut r, mut g, mut b, w): Rgbw) -> Rgb {
            // Add white to each channel
            r += w;
            g += w;
            b += w;

            // Normalize back to [0.0, 1.0]
            let max = r.max(g).max(b);
            if max > 1.0 {
                r /= max;
                g /= max;
                b /= max;
            }

            Self(r, g, b)
        }
    }
    impl From<Rgb> for Rgbw {
        /// Convert Rgb -> Rgbw.
        fn from(Rgb(r, g, b): Rgb) -> Self {
            Self(r, g, b, 0.0)
        }
    }

    impl Into<egui::Color32> for Rgbw {
        fn into(self) -> egui::Color32 {
            Rgb::from(self).into()
        }
    }
    impl Into<egui::Color32> for Rgb {
        fn into(self) -> egui::Color32 {
            let Rgb(r, g, b) = self;
            egui::Color32::from_rgba_premultiplied(r.byte(), g.byte(), b.byte(), 255)
        }
    }
}

/// Operators
mod ops {
    use std::ops::{Add, AddAssign, Mul, MulAssign};

    use super::*;

    // Rgb * f32 -> Rgb, with each color channel scaled.
    impl Mul<f32> for Rgb {
        type Output = Rgb;
        fn mul(self, fr: f32) -> Rgb {
            Self(self.0 * fr, self.1 * fr, self.2 * fr)
        }
    }
    impl MulAssign<f32> for Rgb {
        fn mul_assign(&mut self, rhs: f32) {
            *self = *self * rhs;
        }
    }

    // Rgbw * f32 -> Rgbw, with each color channel scaled.
    impl Mul<f32> for Rgbw {
        type Output = Rgbw;
        fn mul(self, fr: f32) -> Rgbw {
            Self(self.0 * fr, self.1 * fr, self.2 * fr, self.3 * fr)
        }
    }
    impl MulAssign<f32> for Rgbw {
        fn mul_assign(&mut self, rhs: f32) {
            *self = *self * rhs;
        }
    }

    // Normalized sum
    impl Add<Rgbw> for Rgbw {
        type Output = Rgbw;
        fn add(self, rhs: Rgbw) -> Self::Output {
            // sum channels
            let mut r = self.0 + rhs.0;
            let mut g = self.1 + rhs.1;
            let mut b = self.2 + rhs.2;
            let mut w = self.3 + rhs.3;

            // normalize so the brightest channel is at most 1.0
            let max = r.max(g).max(b).max(w);
            if max > 1.0 {
                r /= max;
                g /= max;
                b /= max;
                w /= max;
            }

            Rgbw(r, g, b, w)
        }
    }
    impl AddAssign for Rgbw {
        fn add_assign(&mut self, rhs: Self) {
            *self = *self + rhs;
        }
    }
}

mod consts {
    use super::*;

    #[rustfmt::skip]
    impl Rgb {
        pub const BLACK:   Self = Self(0.0,   0.0,   0.0);
        pub const WHITE:   Self = Self(1.0,   1.0,   1.0);
        pub const RGB:     Self = Self(1.0,   1.0,   1.0);
        pub const HOUSE:   Self = Self(1.0,   0.48,  0.0);
        pub const RED:     Self = Self(1.0,   0.0,   0.0);
        pub const ORANGE:  Self = Self(1.0,   0.251, 0.0);
        pub const YELLOW:  Self = Self(1.0,   1.0,   0.0);
        pub const PEA:     Self = Self(0.533, 1.0,   0.0);
        pub const LIME:    Self = Self(0.0,   1.0,   0.0);
        pub const MINT:    Self = Self(0.0,   1.0,   0.267);
        pub const CYAN:    Self = Self(0.0,   0.8,   1.0);
        pub const BLUE:    Self = Self(0.0,   0.0,   1.0);
        pub const VIOLET:  Self = Self(0.533, 0.0,   1.0);
        pub const MAGENTA: Self = Self(1.0,   0.0,   1.0);
        pub const PINK:    Self = Self(1.0,   0.38,  0.8);
    }

    #[rustfmt::skip]
    impl Rgbw {
        pub const BLACK:   Self = Self(0.0,   0.0,   0.0,   0.0);
        pub const WHITE:   Self = Self(0.0,   0.0,   0.0,   1.0);
        pub const RGB:     Self = Self(1.0,   1.0,   1.0,   0.0);
        pub const RGBW:    Self = Self(1.0,   1.0,   1.0,   1.0);
        pub const HOUSE:   Self = Self(1.0,   0.48,  0.0,   0.0);
        pub const RED:     Self = Self(1.0,   0.0,   0.0,   0.0);
        pub const ORANGE:  Self = Self(1.0,   0.251, 0.0,   0.0);
        pub const YELLOW:  Self = Self(1.0,   1.0,   0.0,   0.0);
        pub const PEA:     Self = Self(0.533, 1.0,   0.0,   0.0);
        pub const LIME:    Self = Self(0.0,   1.0,   0.0,   0.0);
        pub const MINT:    Self = Self(0.0,   1.0,   0.267, 0.0);
        pub const CYAN:    Self = Self(0.0,   0.8,   1.0,   0.0);
        pub const BLUE:    Self = Self(0.0,   0.0,   1.0,   0.0);
        pub const VIOLET:  Self = Self(0.533, 0.0,   1.0,   0.0);
        pub const MAGENTA: Self = Self(1.0,   0.0,   1.0,   0.0);
        pub const PINK:    Self = Self(1.0,   0.38,  0.8,   0.0);
    }
}

mod rand_ {
    use rand::Rng;
    use rand::distributions::{Distribution, Standard};

    use super::*;

    impl Distribution<Rgb> for Standard {
        fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> Rgb {
            match rng.gen_range(0..=11) {
                0 => Rgb::RED,
                1 => Rgb::ORANGE,
                2 => Rgb::YELLOW,
                3 => Rgb::PEA,
                4 => Rgb::LIME,
                5 => Rgb::MINT,
                6 => Rgb::CYAN,
                7 => Rgb::BLUE,
                8 => Rgb::VIOLET,
                9 => Rgb::MAGENTA,
                10 => Rgb::PINK,
                11 => Rgb::WHITE,
                _ => unreachable!(),
            }
        }
    }
}
