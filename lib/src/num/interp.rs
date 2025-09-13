use crate::num::{Range, TAU_2};

pub trait Interp: Sized {
    /// 0 below the threshold and 1 above it
    fn step(self, threshold: Self) -> Self;
    /// false below the threshold and true above it
    fn bstep(self, threshold: Self) -> bool;

    /// Clip into a range, e.g. 0..1
    fn clip<R: Into<Range>>(self, range: R) -> Self;

    /// Modulo with support for negative nuambers, aka Euclidean remainder
    fn fmod(self, v: Self) -> Self;
    /// Modulo by a value, then divide by it
    fn fmod_div(self, v: f64) -> Self;
    /// Interpolate self in 0..1 onto another range
    fn lerp<R: Into<Range>>(self, onto: R) -> Self;
    /// Interpolate self from a range onto 0..1
    fn ilerp<R: Into<Range>>(self, from: R) -> Self;
    /// Map self from a range onto another range
    fn map<R0: Into<Range>, R1: Into<Range>>(self, from: R0, onto: R1) -> Self {
        self.ilerp(from).lerp(onto)
    }
    /// Add `fr` of a periodic `pd` to self
    fn phase(self, pd: Self, fr: Self) -> Self;

    /// Invert self in range
    fn invert<R: Into<Range>>(self, from: R) -> Self {
        let range = from.into();
        self.map(range, range.invert())
    }
    /// Invert self in 0..1
    fn inv(self) -> Self {
        self.invert(0..1)
    }

    /// Project self onto a line, y=mx+b style
    fn line(self, slope: Self, intercept: Self) -> Self;
    /// Project self in 0..1 onto 0..[0.5-amt/2, 0.5+amt/2]..1
    /// https://www.desmos.com/calculator/i6cdluzrj4
    fn cover(self, amt: Self) -> Self;

    fn ssin(self, pd: Self) -> Self;
    fn scos(self, pd: Self) -> Self;
    /// sin(ish), but domain and range are 0..1
    /// https://www.desmos.com/calculator/c8xbmebyiy
    fn fsin(self, pd: Self) -> Self;
    /// cos(ish), but domain and range are 0..1
    /// https://www.desmos.com/calculator/7efgxmnpoe
    fn fcos(self, pd: Self) -> Self;
    /// Triangle wave
    /// https://www.desmos.com/calculator/psso6ibqq7
    fn tri(self, pd: Self) -> Self;
    /// Ramp (saw wave)
    /// https://www.desmos.com/calculator/v4dlv296h3
    fn ramp(self, pd: Self) -> Self;
    /// Square wave
    /// https://www.desmos.com/calculator/fsfuxn4xvg
    fn square(self, pd: Self, duty: Self) -> Self;
    fn negsquare(self, pd: Self, duty: Self) -> Self;
    /// Square wave, but booleans
    fn bsquare(self, pd: Self, duty: Self) -> bool;

    fn trapazoid(self, pd: Self, ramp: Self) -> Self;

    /// Convert 0..1 to 0..255u8
    fn byte(self) -> u8;
    /// Convert 0..1 to 0..127u8
    fn midi_byte(self) -> u8;
}

impl Interp for f64 {
    fn step(self, threshold: f64) -> f64 {
        if self < threshold {
            0.0
        } else {
            1.0
        }
    }
    fn bstep(self, threshold: f64) -> bool {
        self.step(threshold) == 1.0
    }

    fn clip<R: Into<Range>>(self, range: R) -> f64 {
        let range = range.into();
        self.clamp(range.lo, range.hi)
    }

    fn fmod(self, v: f64) -> f64 {
        self.rem_euclid(v)
    }
    fn fmod_div(self, v: f64) -> f64 {
        self.fmod(v) / v
    }

    fn lerp<R: Into<Range>>(self, onto: R) -> f64 {
        let (i, j) = onto.into().bounds();
        i + self.clamp(0.0, 1.0) * (j - i)
    }
    fn ilerp<R: Into<Range>>(self, from: R) -> f64 {
        let (i, j) = from.into().bounds();
        (self - i) / (j - i)
    }
    fn phase(self, pd: f64, fr: f64) -> f64 {
        (self + (fr * pd)).rem_euclid(pd)
    }

    fn line(self, slope: f64, intercept: f64) -> f64 {
        (self * slope) + intercept
    }
    fn cover(self, amt: f64) -> f64 {
        self.line(amt, (1.0 - amt) / 2.0)
    }

    fn ssin(self, pd: f64) -> f64 {
        let t = (2.0 * TAU_2 * self) / pd;
        t.sin()
    }
    fn scos(self, pd: f64) -> f64 {
        let t = (2.0 * TAU_2 * self) / pd;
        t.cos()
    }
    fn fsin(self, pd: f64) -> f64 {
        let t = (2.0 * TAU_2 * self) / pd + (TAU_2 / 2.0);
        0.5 * t.sin() + 0.5
    }
    fn fcos(self, pd: f64) -> f64 {
        self.phase(pd, 0.5).fsin(pd)
    }
    fn ramp(self, pd: f64) -> f64 {
        self.fmod(pd) / pd
    }
    fn tri(self, pd: f64) -> f64 {
        let ramp = (2.0 * self - pd).fmod(2.0 * pd);
        (ramp - pd).abs() / pd
    }
    fn square(self, pd: f64, duty: f64) -> f64 {
        1.0 - self.fmod(pd).step(pd * duty)
    }
    fn negsquare(self, pd: f64, duty: f64) -> f64 {
        2.0 * (1.0 - self.fmod(pd).step(pd * duty)) - 1.0
    }
    fn bsquare(self, pd: f64, duty: f64) -> bool {
        self.square(pd, duty) == 1.0
    }

    fn trapazoid(self, pd: f64, ramp_dur: f64) -> Self {
        if self < ramp_dur {
            self / ramp_dur
        } else if self < pd - ramp_dur {
            1.0
        } else {
            1.0 - (self - pd + ramp_dur) / ramp_dur
        }
    }

    fn byte(self) -> u8 {
        self.clamp(0.0, 1.0).lerp(0..255) as u8
    }
    fn midi_byte(self) -> u8 {
        self.clamp(0.0, 1.0).lerp(0..127) as u8
    }
}
