use lib::prelude::*;
use rand::Rng;
use rand::rngs::ThreadRng;

use super::{State, Swatch, Wheel1, Wheel2};

pub trait Palette: DynClone + Send + Sync + 'static {
    fn name(&self) -> &'static str;
    /// The movers' colour, which for the Hydros is where their wheels sit.
    fn beam_color(&self, s: &State) -> Swatch;
    /// The pars mix, so they only need the rgbw.
    fn par_color(&self, s: &State) -> Rgbw;
    /// Mover `i` of 4, for palettes that vary across the rig.
    fn beam_color_at(&self, s: &State, _i: usize) -> Swatch {
        self.beam_color(s)
    }
    /// Par `i` of 6.
    fn par_color_at(&self, s: &State, _i: usize) -> Rgbw {
        self.par_color(s)
    }
    /// What the pad button shows. Two-colour palettes take turns, two beats
    /// apiece, so the pair reads at a glance.
    fn pad_color(&self, s: &State) -> Rgbw {
        self.beam_color(s).rgbw
    }
    /// Whether the hydros are rocking a colour wheel with this palette, so
    /// the render can cut the strobe into their gobo wheel too.
    fn wheel_strobing(&self) -> bool {
        false
    }
}
clone_trait_object!(Palette);

/// Which half of a two-beat toggle a two-colour button is showing.
fn taking_turns(s: &State) -> usize {
    (s.pd(Pd(2, 1)) >= 0.5) as usize
}

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
    fn pad_color(&self, s: &State) -> Rgbw { [self.0.rgbw, self.1][taking_turns(s)] }
}

/// Two colours woven across each family, so the rig is never one flat field.
/// A pair a single slot apart on the same wheel puts the hydros on both sides
/// of it at once.
#[derive(Clone)]
pub struct Duo(pub Swatch, pub Swatch);
#[rustfmt::skip]
impl Palette for Duo {
    fn name(&self) -> &'static str { "duo" }
    fn beam_color(&self, _: &State) -> Swatch { self.0 }
    fn par_color(&self, _: &State) -> Rgbw { self.0.rgbw }
    fn beam_color_at(&self, _: &State, i: usize) -> Swatch { [self.0, self.1][i % 2] }
    fn par_color_at(&self, _: &State, i: usize) -> Rgbw { [self.0.rgbw, self.1.rgbw][i % 2] }
    fn pad_color(&self, s: &State) -> Rgbw { [self.0.rgbw, self.1.rgbw][taking_turns(s)] }
}

///////////////////////// Toned /////////////////////////

/// A hue and the company it may keep. Each press rolls what the button means
/// this time: solid, or paired with one of its partners, the pair dealt onto
/// one of a few symmetric slices of the rig.
#[derive(Clone)]
pub struct Toned {
    pub base: Swatch,
    pub pals: &'static [Swatch],
}

impl Toned {
    /// `(a, b, deal)`: the pair and how it is dealt. 0 solid; 1 movers a,
    /// pars b; 2 the reverse; 3 woven by index; 4 the hydros carry b as an
    /// accent.
    fn roll(&self, s: &State) -> (Swatch, Swatch, usize) {
        use rand::prelude::*;
        let mut rng = StdRng::seed_from_u64(s.palette_seed as u64);
        if self.pals.is_empty() || rng.gen_range(0..3) == 0 {
            return (self.base, self.base, 0);
        }
        let pal = self.pals[rng.gen_range(0..self.pals.len())];
        (self.base, pal, 1 + rng.gen_range(0..4))
    }
}

impl Palette for Toned {
    fn name(&self) -> &'static str {
        "tone"
    }
    fn beam_color(&self, s: &State) -> Swatch {
        self.beam_color_at(s, 0)
    }
    fn par_color(&self, s: &State) -> Rgbw {
        self.par_color_at(s, 0)
    }
    fn beam_color_at(&self, s: &State, i: usize) -> Swatch {
        let (a, b, deal) = self.roll(s);
        match deal {
            2 => b,
            3 => [a, b][i % 2],
            4 if i >= 2 => b,
            _ => a,
        }
    }
    fn par_color_at(&self, s: &State, i: usize) -> Rgbw {
        let (a, b, deal) = self.roll(s);
        match deal {
            1 => b.rgbw,
            3 => [a.rgbw, b.rgbw][i % 2],
            _ => a.rgbw,
        }
    }
    fn pad_color(&self, s: &State) -> Rgbw {
        let (a, b, deal) = self.roll(s);
        match deal {
            0 => a.rgbw,
            _ => [a.rgbw, b.rgbw][taking_turns(s)],
        }
    }
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

impl<const N: usize> Cycle<N> {
    /// Whether the hydros strobe along: some step engages a wheel, and no
    /// step is more than a slot from the last — the one move a wheel makes
    /// in ~100ms with nothing blanked, which at the cycle's quarter-beat
    /// steps is a perpetual rock between neighbours. Anything further apart
    /// could never arrive, so those cycles carry open swatches and the
    /// hydros sit them out.
    fn wheel_strobe(&self) -> bool {
        let engaged =
            self.0.iter().any(|c| c.one != Wheel1::Open || c.two != Wheel2::Open);
        engaged
            && (0..N).all(|k| {
                let (a, b) = (&self.0[k], &self.0[(k + 1) % N]);
                near(a.one as i32, b.one as i32) && near(a.two as i32, b.two as i32)
            })
    }
}

/// Whether two wheel positions are within a slot of each other, round the
/// wheel: 18 half steps, a whole slot two apart.
fn near(a: i32, b: i32) -> bool {
    let d = (a - b).rem_euclid(18);
    d.min(18 - d) <= 2
}

impl<const N: usize> Palette for Cycle<N> {
    fn name(&self) -> &'static str {
        "cycle"
    }
    fn beam_color(&self, s: &State) -> Swatch {
        let fr = s.pd(Pd(1, 2)).ramp(1.0);
        let i = (fr * N as f32).floor() as usize;
        // The hydros only join a cycle their wheels can follow; white along
        // for the ride just dulls the flash, so otherwise they sit it out.
        match self.wheel_strobe() {
            true => self.0[i],
            false => self.0[i].dark(),
        }
    }
    fn par_color(&self, s: &State) -> Rgbw {
        self.beam_color(s).rgbw
    }
    fn wheel_strobing(&self) -> bool {
        self.wheel_strobe()
    }
}

///////////////////////// Random /////////////////////////

/// Auto's pool: calm colours only, nothing that flashes or cycles on its
/// own, since a swap can land at any moment.
pub fn random_palette(rng: &mut ThreadRng) -> Box<dyn Palette> {
    match rng.gen_range(0..9) {
        0 => Box::new(Solid(Swatch::RED)),
        1 => Box::new(Solid(Swatch::BLUE)),
        2 => Box::new(Solid(Swatch::CYAN)),
        3 => Box::new(Solid(Swatch::MINT)),
        4 => Box::new(Solid(Swatch::MAGENTA)),
        5 => Box::new(Split(Swatch::WHITE, Rgbw::RED)),
        6 => Box::new(Duo(Swatch::BLUE, Swatch::MINT)),
        7 => Box::new(Duo(Swatch::RED, Swatch::ORANGE)),
        _ => Box::new(Duo(Swatch::CYAN, Swatch::MAGENTA)),
    }
}
