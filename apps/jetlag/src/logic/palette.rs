use lib::prelude::*;
use rand::Rng;
use rand::rngs::ThreadRng;

use super::State;

/// Every fixture in the classic rig mixes rgbw, so a palette is just where the
/// two colour groups sit: the movers' colour and the pars'.
pub trait Palette: DynClone + Send + Sync + 'static {
    fn name(&self) -> &'static str;
    fn beam_color(&self, s: &State) -> Rgbw;
    fn par_color(&self, s: &State) -> Rgbw;
}
clone_trait_object!(Palette);

///////////////////////// Solid/Split /////////////////////////

#[derive(Clone)]
pub struct Solid(pub Rgbw);
#[rustfmt::skip]
impl Palette for Solid {
    fn name(&self) -> &'static str { "solid" }
    fn beam_color(&self, _: &State) -> Rgbw { self.0 }
    fn par_color(&self, _: &State) -> Rgbw { self.0 }
}

#[derive(Clone)]
pub struct Split(pub Rgbw, pub Rgbw);
#[rustfmt::skip]
impl Palette for Split {
    fn name(&self) -> &'static str { "split" }
    fn beam_color(&self, _: &State) -> Rgbw { self.0 }
    fn par_color(&self, _: &State) -> Rgbw { self.1 }
}

///////////////////////// Rainbow /////////////////////////

#[derive(Clone)]
pub struct Rainbow;
impl Palette for Rainbow {
    fn name(&self) -> &'static str {
        "rainbow"
    }
    fn beam_color(&self, s: &State) -> Rgbw {
        Rgb::hsv(s.phi(16, 1), 1.0, 1.0).into()
    }
    fn par_color(&self, s: &State) -> Rgbw {
        self.beam_color(s)
    }
}

///////////////////////// Cycle /////////////////////////

#[derive(Clone)]
pub struct Cycle<const N: usize>(pub [Rgbw; N]);

impl<const N: usize> Palette for Cycle<N> {
    fn name(&self) -> &'static str {
        "cycle"
    }
    fn beam_color(&self, s: &State) -> Rgbw {
        let fr = s.pd(Pd(1, 2)).ramp(1.0);
        let i = (fr * N as f32).floor() as usize;
        self.0[i]
    }
    fn par_color(&self, s: &State) -> Rgbw {
        self.beam_color(s)
    }
}

///////////////////////// Random /////////////////////////

pub fn random_palette(rng: &mut ThreadRng) -> Box<dyn Palette> {
    match rng.gen_range(0..8) {
        0 => Box::new(Solid(Rgbw::RED)),
        1 => Box::new(Solid(Rgbw::BLUE)),
        2 => Box::new(Solid(Rgbw::CYAN)),
        3 => Box::new(Solid(Rgbw::MINT)),
        4 => Box::new(Solid(Rgbw::MAGENTA)),
        5 => Box::new(Split(Rgbw::WHITE, Rgbw::RED)),
        6 => Box::new(Cycle([Rgbw::BLUE, Rgbw::WHITE])),
        _ => Box::new(Rainbow),
    }
}
