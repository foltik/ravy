use lib::prelude::*;

use super::State;

pub trait Palette: DynClone + Send + Sync + 'static {
    fn beam_color(&self, s: &State) -> Rgbw;
    fn spot_color(&self, s: &State) -> Rgbw;
    fn gradient(&self, s: &State) -> RgbwGradient;
}
clone_trait_object!(Palette);

///////////////////////// Solid/Split /////////////////////////

#[derive(Clone)]
pub struct Solid(pub Rgbw);
#[rustfmt::skip]
impl Palette for Solid {
    fn beam_color(&self, _: &State) -> Rgbw { self.0 }
    fn spot_color(&self, _: &State) -> Rgbw { self.0 }
    fn gradient(&self, _: &State) -> RgbwGradient { RgbwGradient::split(Rgbw::BLACK, self.0) }
}

#[derive(Clone)]
pub struct Split(pub Rgbw, pub Rgbw);
#[rustfmt::skip]
impl Palette for Split {
    fn beam_color(&self, _: &State) -> Rgbw { self.0 }
    fn spot_color(&self, _: &State) -> Rgbw { self.1 }
    fn gradient(&self, _: &State) -> RgbwGradient { RgbwGradient::split(self.0, self.1) }
}

///////////////////////// Rainbow /////////////////////////

#[derive(Clone)]
pub struct Rainbow;
impl Palette for Rainbow {
    fn beam_color(&self, s: &State) -> Rgbw {
        Rgb::hsv(s.phi(16, 1), 1.0, 1.0).into()
    }
    fn spot_color(&self, s: &State) -> Rgbw {
        self.beam_color(s)
    }
    fn gradient(&self, _: &State) -> RgbwGradient {
        RgbwGradient::RAINBOW
    }
}

///////////////////////// Cycle /////////////////////////

#[derive(Clone)]
pub struct Cycle<const N: usize>(pub [Rgbw; N]);

// Osc([Rgbw::RED, Rgbw::LIME, Rgbw::BLUE])
// Osc([Rgbw::RED, Rgbw::ORANGE, Rgbw::YELLOW, Rgbw::LIME, Rgbw::MINT, Rgbw::CYAN, Rgbw::BLUE, Rgbw::MAGENTA])
// Osc([Rgbw::RED, Rgbw::WHITE])
// Osc([Rgbw::GREEN, Rgbw::WHITE])
// Osc([Rgbw::BLUE, Rgbw::WHITE])

impl<const N: usize> Palette for Cycle<N> {
    fn beam_color(&self, s: &State) -> Rgbw {
        let fr = s.pd(Pd(1, 2)).ramp(1.0);
        let i = (fr * N as f32).floor() as usize;
        self.0[i]
    }
    fn spot_color(&self, s: &State) -> Rgbw {
        self.beam_color(s)
    }
    fn gradient(&self, s: &State) -> RgbwGradient {
        RgbwGradient::solid(self.beam_color(s))
    }
}
