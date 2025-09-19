use crate::math::TAU_4;

/// Easing functions for floats in `0.0..1.0`, mirroring those from CSS.
///
/// See <https://easings.net/>
pub trait Ease: Sized {
    fn in_quad(self) -> Self;
    fn out_quad(self) -> Self;
    fn inout_quad(self) -> Self;

    fn in_cubic(self) -> Self;
    fn out_cubic(self) -> Self;
    fn inout_cubic(self) -> Self;

    fn in_quartic(self) -> Self;
    fn out_quartic(self) -> Self;
    fn inout_quartic(self) -> Self;

    fn in_exp(self) -> Self;
    fn out_exp(self) -> Self;
    fn inout_exp(self) -> Self;

    fn in_sin(self) -> Self;
    fn out_sin(self) -> Self;
    fn inout_sin(self) -> Self;
}

impl Ease for f32 {
    fn in_quad(self) -> f32 {
        self * self
    }
    fn out_quad(self) -> f32 {
        -(self * (self - 2.))
    }
    fn inout_quad(self) -> f32 {
        if self < 0.5 {
            2. * self * self
        } else {
            (-2. * self * self) + self.mul_add(4., -1.)
        }
    }

    fn in_cubic(self) -> f32 {
        self * self * self
    }
    fn out_cubic(self) -> f32 {
        let y = self - 1.;
        y * y * y + 1.
    }
    fn inout_cubic(self) -> f32 {
        if self < 0.5 {
            4. * self * self * self
        } else {
            let y = self.mul_add(2., -2.);
            (y * y * y).mul_add(0.5, 1.)
        }
    }
    fn in_quartic(self) -> f32 {
        self * self * self * self
    }
    fn out_quartic(self) -> f32 {
        let y = self - 1.;
        (y * y * y).mul_add(1. - self, 1.)
    }
    fn inout_quartic(self) -> f32 {
        if self < 0.5 {
            8. * self * self * self * self
        } else {
            let y = self - 1.;
            (y * y * y * y).mul_add(-8., 1.)
        }
    }
    fn in_sin(self) -> f32 {
        let y = (self - 1.) * TAU_4;
        y.sin() + 1.
    }
    fn out_sin(self) -> f32 {
        (self * TAU_4).sin()
    }
    fn inout_sin(self) -> f32 {
        if self < 0.5 {
            0.5 * (1. - (self * self).mul_add(-4., 1.).sqrt())
        } else {
            0.5 * ((self.mul_add(-2., 3.) * self.mul_add(2., -1.)).sqrt() + 1.)
        }
    }
    fn in_exp(self) -> f32 {
        if self == 0. { 0. } else { (10. * (self - 1.)).exp2() }
    }
    fn out_exp(self) -> f32 {
        if self == 1. { 1. } else { 1. - (-10. * self).exp2() }
    }
    fn inout_exp(self) -> f32 {
        if self == 1. {
            1.
        } else if self == 0. {
            0.
        } else if self < 0.5 {
            self.mul_add(20., -10.).exp2() * 0.5
        } else {
            self.mul_add(-20., 10.).exp2().mul_add(-0.5, 1.)
        }
    }
}
