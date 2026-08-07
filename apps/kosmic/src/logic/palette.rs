use lib::prelude::*;
use rand::Rng;
use rand::rngs::ThreadRng;

use super::{State, Swatch};

pub trait Palette: DynClone + Send + Sync + 'static {
    fn name(&self) -> &'static str;
    /// The movers' colour, which for the Hydros is where their wheels sit.
    fn beam_color(&self, s: &State) -> Swatch;
    /// The pars mix, so they only need the rgbw.
    fn par_color(&self, s: &State) -> Rgbw;
}
clone_trait_object!(Palette);

///////////////////////// Solid/Split /////////////////////////

#[derive(Clone)]
pub struct Solid(pub Swatch);
#[rustfmt::skip]
impl Palette for Solid {
    fn name(&self) -> &'static str { "solid" }
    fn beam_color(&self, _: &State) -> Swatch { self.0 }
    fn par_color(&self, _: &State) -> Rgbw { self.0.rgbw }
}

#[derive(Clone)]
pub struct Split(pub Swatch, pub Rgbw);
#[rustfmt::skip]
impl Palette for Split {
    fn name(&self) -> &'static str { "split" }
    fn beam_color(&self, _: &State) -> Swatch { self.0 }
    fn par_color(&self, _: &State) -> Rgbw { self.1 }
}

///////////////////////// Rainbow /////////////////////////

#[derive(Clone)]
pub struct Rainbow;
impl Palette for Rainbow {
    fn name(&self) -> &'static str {
        "rainbow"
    }
    fn beam_color(&self, s: &State) -> Swatch {
        // The slots are not in hue order, so a Hydro chasing the sweep blanks
        // on 8 of the 11 steps a revolution takes. It sits the sweep out.
        Swatch { rgbw: Rgb::hsv(s.phi(16, 1), 1.0, 1.0).into(), ..Swatch::WHITE }.dark()
    }
    fn par_color(&self, s: &State) -> Rgbw {
        self.beam_color(s).rgbw
    }
}

///////////////////////// Cycle /////////////////////////

#[derive(Clone)]
pub struct Cycle<const N: usize>(pub [Swatch; N]);

impl<const N: usize> Palette for Cycle<N> {
    fn name(&self) -> &'static str {
        "cycle"
    }
    fn beam_color(&self, s: &State) -> Swatch {
        let fr = s.pd(Pd(1, 2)).ramp(1.0);
        let i = (fr * N as f32).floor() as usize;
        self.0[i]
    }
    fn par_color(&self, s: &State) -> Rgbw {
        self.beam_color(s).rgbw
    }
}

///////////////////////// Random /////////////////////////

pub fn random_palette(rng: &mut ThreadRng) -> Box<dyn Palette> {
    match rng.gen_range(0..8) {
        0 => Box::new(Solid(Swatch::RED)),
        1 => Box::new(Solid(Swatch::BLUE)),
        2 => Box::new(Solid(Swatch::CYAN)),
        3 => Box::new(Solid(Swatch::MINT)),
        4 => Box::new(Solid(Swatch::MAGENTA)),
        5 => Box::new(Split(Swatch::WHITE, Rgbw::RED)),
        6 => Box::new(Cycle([Swatch::BLUE, Swatch::WHITE])),
        _ => Box::new(Rainbow),
    }
}
