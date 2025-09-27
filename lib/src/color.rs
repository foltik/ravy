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
        (0.333 * self.0) + (0.333 * self.1) + (0.333 * self.2)
    }

    /// Encode uniform greys as white-only in Rgbw, others as RGB-only.
    pub fn with_w(self) -> Rgbw {
        let Rgb(r, g, b) = self;
        if r == g && g == b { Rgbw(0.0, 0.0, 0.0, r) } else { Rgbw(r, g, b, 0.0) }
    }
}

/// An (r, g, b, w) color.
#[derive(Default, Clone, Copy, PartialEq, Debug)]
pub struct Rgbw(pub f32, pub f32, pub f32, pub f32);

/// 3-channel cosine gradient
#[derive(Debug, Clone)]
pub struct RgbGradient {
    pub dc: Vec3,
    pub amp: Vec3,
    pub freq: Vec3,
    pub phase: Vec3,
}

/// 4-channel cosine gradient
#[derive(Debug, Clone)]
pub struct RgbwGradient {
    pub dc: Vec4,
    pub amp: Vec4,
    pub freq: Vec4,
    pub phase: Vec4,
}

impl RgbwGradient {
    pub const RAINBOW: Self = Self {
        dc: vec4(0.5, 0.5, 0.5, 0.0),
        amp: vec4(1.0, 1.0, 1.0, 0.0),
        freq: vec4(1.0, 1.0, 1.0, 0.0),
        phase: vec4(0.0, 1.0 / 3.0, 2.0 / 3.0, 0.0),
    };

    // Palette::Rainbow => RgbGradient {
    //     dc: vec3(0.731, 1.098, 0.192),
    //     amp: vec3(0.358, 1.090, 0.657),
    //     freq: vec3(1.077, 0.360, 0.328),
    //     phase: vec3(0.965, 2.265, 0.837),
    // },
    // ColorPalette::Rainbow => RgbGradient {
    //     dc: vec3(0.5, 0.5, 0.5),
    //     amp: vec3(0.5, 0.5, 0.5),
    //     freq: vec3(0.8, 0.8, 0.5),
    //     phase: vec3(0.0, 0.2, 0.5),
    // },
}

/*

  color palette ideas:

  paletteCol = palette(var, vec3(0.420, 0.500, 0.500), vec3(0.500, 0.500, 0.500), vec3(0.600, 0.250, 1.200), vec3(0.500, 0.450, 0.500));
  paletteCol = palette(var, vec3(0.500, 0.580, 0.500), vec3(0.190, 0.460, 0.500), vec3(0.760, 0.740, 0.580), vec3(1.000, 0.300, 0.490));
  paletteCol = palette(1.0-var,vec3(0.500, 0.940, 0.900), vec3(0.600, 0.640, 0.350), vec3(0.680, 1.020, 0.405), vec3(0.380, 0.440, 0.095));
  paletteCol = palette(var,vec3(0.500, 0.500, 0.500), vec3(0.500, 0.500, 0.500), vec3(0.500, 0.825, 0.750), vec3(0.500, 0.500, 0.500));
  paletteCol = palette(1.0-var,vec3(0.000, 0.580, 0.453), vec3(0.848, 0.703, 0.110), vec3(0.700, 0.175, 0.542), vec3(0.000, 0.182, 0.915));
  paletteCol = palette(var,vec3(0.129, 0.359, 0.072), vec3(0.933, 0.561, 0.616), vec3(0.334, 1.013, 0.882), vec3(0.597, 0.050, 0.885));
  paletteCol = palette(var,  vec3(-0.060, -0.340, 0.100), vec3(0.940, 0.840, 0.713), vec3(0.600, 0.735, 0.7191), vec3(0.500, 0.260, 0.335));
  paletteCol = palette(var,  vec3(-0.060, -0.340, 0.100), vec3(0.940, 0.840, 0.713), vec3(0.600, 0.735, 0.2191), vec3(0.500, 0.260, 0.335));
  paletteCol = palette(var,vec3(0.000, 0.040, 0.073), vec3(0.000, 0.530, 0.420), vec3(0.485, 0.930, 0.931), vec3(0.400, 0.599, 0.520));
  paletteCol = palette(1.0-var,vec3(1.040, 0.180, 0.260), vec3(0.053, 0.775, 0.330), vec3(0.142, 0.523, 0.800), vec3(0.242, 0.887, 0.000));
*/

impl RgbwGradient {
    pub fn solid(color: Rgbw) -> Self {
        Self { dc: Vec4::from(color), amp: Vec4::ZERO, freq: Vec4::ZERO, phase: Vec4::ZERO }
    }

    pub fn split(start: Rgbw, end: Rgbw) -> Self {
        let start = Vec4::from(start);
        let end = Vec4::from(end);
        Self {
            dc: (start + end) * 0.5,  // midpoint
            amp: (start - end) * 0.5, // half difference
            freq: Vec4::splat(0.5),   // half cycle
            phase: Vec4::ZERO,
        }
    }

    pub fn eval(&self, t: f32) -> Rgbw {
        let t = t.clamp(0.0, 1.0);
        let angle = (self.freq * Vec4::splat(t) + self.phase) * TAU;
        let cos = vec4(angle.x.cos(), angle.y.cos(), angle.z.cos(), angle.w.cos());
        Rgbw::from(self.dc + self.amp * cos)
    }
}

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

    impl From<Vec3> for Rgb {
        fn from(Vec3 { x, y, z }: Vec3) -> Self {
            Self(x, y, z)
        }
    }
    impl From<Rgb> for Vec3 {
        fn from(Rgb(r, g, b): Rgb) -> Self {
            vec3(r, g, b)
        }
    }
    impl From<Vec4> for Rgbw {
        fn from(vec: Vec4) -> Self {
            Self(vec.x, vec.y, vec.z, vec.w)
        }
    }
    impl From<Rgbw> for Vec4 {
        fn from(Rgbw(r, g, b, w): Rgbw) -> Self {
            vec4(r, g, b, w)
        }
    }

    impl From<RgbwGradient> for RgbGradient {
        fn from(g: RgbwGradient) -> Self {
            fn convert(v: Vec4) -> Vec3 {
                Vec3::from(Rgb::from(Rgbw::from(v)))
            }
            RgbGradient {
                dc: convert(g.dc),
                amp: convert(g.amp),
                // w has no rgb meaning for these
                freq: Vec3::new(g.freq.x, g.freq.y, g.freq.z),
                phase: Vec3::new(g.phase.x, g.phase.y, g.phase.z),
            }
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
