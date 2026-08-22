use itertools::Itertools;
use lib::prelude::*;
use rand::Rng;
use rand::rngs::ThreadRng;

use super::look::*;
use super::palette::*;
use super::special::*;
use crate::lights::Lights;
use crate::logic::{PadBinding, PadOp};
use crate::mirror::{Ctrl, Pad};
use crate::rig::Trim;
use crate::{beat, bind, func, look, palette};

///////////////////////// BINDINGS /////////////////////////

bind! {

perform:
    // Tap to record BPM
    (0, 7) => func!("tap bpm", Rgbw::VIOLET, |s, _| s.bpm_taps.push(s.t)),
    // Tap to calculate BPM and reset phase
    (7, 7) => func!("apply bpm", Rgbw::VIOLET, |s, _| s.apply_bpm()),

    // Manual beats: the left column flashes the pars, the right the movers.
    (0, 0) => beat!(0, Pd(4, 1)),
    (0, 1) => beat!(0, Pd(2, 1)),
    (0, 2) => beat!(0, Pd(1, 1)),
    (0, 3) => beat!(0, Pd(1, 2)),
    (0, 4) => beat!(0, Pd(1, 4)),
    (7, 0) => beat!(1, Pd(4, 1)),
    (7, 1) => beat!(1, Pd(2, 1)),
    (7, 2) => beat!(1, Pd(1, 1)),
    (7, 3) => beat!(1, Pd(1, 2)),
    (7, 4) => beat!(1, Pd(1, 4)),

    // --- y=0: off, and breaks that leave the pars dark ---
    (1, 0) => look!(Look::new(Energy::Off, Movement::Out, Pd(4, 1))),
    (2, 0) => look!(Look::new(Energy::On, Movement::Out, Pd(4, 1)).masked(Mask::MOVERS)),
    (3, 0) => look!(Look::new(Energy::On, Movement::WaveY, Pd(4, 1)).masked(Mask::MOVERS)),
    (4, 0) => look!(Look::new(Energy::On, Movement::Ripple { dir: Dir::Ltr, size: 1.1 }, Pd(32, 1)).masked(Mask::MOVERS)),
    (5, 0) => look!(Look::new(Energy::On, Movement::Carousel, Pd(16, 1)).masked(Mask::MOVERS)),
    (6, 0) => look!(Look::new(Energy::On, Movement::Spin, Pd(8, 1)).masked(Mask::MOVERS)),

    // --- y=1: the quietest looks, a few fixtures at a time ---
    (1, 1) => look!(Look::new(Energy::Swell { pd: Pd(2, 1) }, Movement::Out, Pd(4, 1)).masked(Mask::SPIDERS)),
    (2, 1) => look!(Look::new(Energy::On, Movement::Out, Pd(4, 1)).masked(Mask::ACCENTS)),
    (3, 1) => look!(Look::new(Energy::Swell { pd: Pd(1, 1) }, Movement::Out, Pd(4, 1)).masked(Mask::PAR_SPIDERS)),
    (4, 1) => look!(Look::new(Energy::On, Movement::WaveY, Pd(8, 1)).masked(Mask::BIGS)),
    (5, 1) => look!(Look::new(Energy::On, Movement::CrissCross { pitch: 0.4 }, Pd(4, 1)).masked(Mask::BEAMS)),
    (6, 1) => look!(Look::new(Energy::Swell { pd: Pd(2, 1) }, Movement::Hut { orbit: 0.07 }, Pd(8, 1)).masked(Mask::BEAM_SPIDERS)),

    // --- y=2: the whole rig, but gently ---
    (1, 2) => look!(Look::new(Energy::On, Movement::Out, Pd(4, 1))),
    (2, 2) => look!(Look::new(Energy::On, Movement::Fan { amount: 0.6 }, Pd(16, 1))),
    (3, 2) => look!(Look::new(Energy::Swell { pd: Pd(1, 1) }, Movement::WaveY, Pd(4, 1))),
    (4, 2) => look!(Look::new(Energy::Swell { pd: Pd(2, 1) }, Movement::Carousel, Pd(16, 1)).masked(Mask::BIGS)),
    (5, 2) => look!(Look::new(Energy::Beat { pd: Pd(4, 1) }, Movement::Carousel, Pd(16, 1))),
    (6, 2) => look!(Look::new(Energy::On, Movement::Hut { orbit: 0.07 }, Pd(8, 1))),

    // --- y=3: mid pulses across the subsets ---
    (1, 3) => look!(Look::new(Energy::Swell { pd: Pd(1, 2) }, Movement::Square, Pd(4, 1))),
    (2, 3) => look!(Look::new(Energy::Beat { pd: Pd(2, 1) }, Movement::Square, Pd(2, 1)).masked(Mask::BIGS)),
    (3, 3) => look!(Look::new(Energy::Beat { pd: Pd(2, 1) }, Movement::Ripple { dir: Dir::Ltr, size: 1.1 }, Pd(8, 1)).masked(Mask::BEAMS)),
    (4, 3) => look!(Look::new(Energy::Beat { pd: Pd(2, 1) }, Movement::Out, Pd(2, 1)).masked(Mask::PAR_SPIDERS)),
    (5, 3) => look!(Look::new(Energy::Beat { pd: Pd(2, 1) }, Movement::Carousel, Pd(8, 1))),
    (6, 3) => look!(Look::new(Energy::Beat { pd: Pd(2, 1) }, Movement::Twisting, Pd(2, 1))),

    // --- y=4: every-beat hits, the classics plus solo banks ---
    (1, 4) => look!(Look::new(Energy::Beat { pd: Pd(1, 1) }, Movement::Square, Pd(1, 1))),
    (2, 4) => look!(Look::new(Energy::Beat { pd: Pd(1, 1) }, Movement::Twisting, Pd(1, 1)).masked(Mask::BIGS)),
    (3, 4) => look!(Look::new(Energy::Beat { pd: Pd(1, 1) }, Movement::WaveY, Pd(1, 1))),
    (4, 4) => look!(Look::new(Energy::Beat { pd: Pd(1, 1) }, Movement::SnapX, Pd(1, 1)).masked(Mask::BEAMS)),
    (5, 4) => look!(Look::new(Energy::Beat { pd: Pd(1, 1) }, Movement::Whirl, Pd(1, 1))),
    (6, 4) => look!(Look::new(Energy::Beat { pd: Pd(1, 1) }, Movement::Twisting, Pd(1, 1))),

    // --- y=5: strobes and chases riding the big sweeps ---
    (0, 5) => look!(Look::new(Energy::Strobe { pd: Pd(1, 8), duty: 1.0 }, Movement::Square, Pd(2, 1)).masked(Mask::PARS)),
    (1, 5) => look!(Look::new(Energy::Alternate { pd: Pd(1, 4) }, Movement::Ripple { dir: Dir::Ltr, size: 1.1 }, Pd(8, 1)).masked(Mask::MOVERS)),
    (2, 5) => look!(Look::new(Energy::Strobe { pd: Pd(1, 4), duty: 1.0 }, Movement::Spin, Pd(4, 1))),
    (3, 5) => look!(Look::new(Energy::Strobe { pd: Pd(1, 8), duty: 1.0 }, Movement::Carousel, Pd(8, 1))),
    (4, 5) => look!(Look::new(Energy::Chase { pd: Pd(1, 2) }, Movement::Spin, Pd(4, 1))),
    (5, 5) => look!(Look::new(Energy::Chase { pd: Pd(1, 1) }, Movement::Carousel, Pd(8, 1)).colored(Rgbw::WHITE)),
    (6, 5) => look!(Look::new(Energy::Chase { pd: Pd(1, 2) }, Movement::Spin, Pd(8, 1)).colored(Rgbw::WHITE)),
    (7, 5) => look!(Look::new(Energy::Chase { pd: Pd(1, 4) }, Movement::Twisting, Pd(1, 2)).colored(Rgbw::WHITE)),

}

///////////////////////// PALETTE BANKS /////////////////////////

/// A bankful of colour, filling y=6 and y=7. The four arrows pick between them,
/// so the fourteen colour buttons are worth fifty-six.
pub struct Bank {
    pub name: &'static str,
    /// What the arrow that picks this bank lights up as.
    pub color: Rgbw,
    pub palettes: Vec<PadBinding>,
}

pub fn banks() -> [Bank; 4] {
    [
        Bank { name: "perform", color: Rgbw::WHITE, palettes: perform_colors() },
        Bank { name: "solid", color: Rgbw::RED, palettes: solid_colors() },
        Bank { name: "split", color: Rgbw::CYAN, palettes: split_colors() },
        Bank { name: "flash", color: Rgbw::MAGENTA, palettes: flash_colors() },
    ]
}

bind! {

perform_colors:
    (0, 6) => palette!(Solid(Rgbw::WHITE)),
    (1, 6) => palette!(Solid(Rgbw::ORANGE)),
    (2, 6) => palette!(Solid(Rgbw::YELLOW)),
    (3, 6) => palette!(Solid(Rgbw::LIME)),
    (4, 6) => palette!(Solid(Rgbw::BLUE)),
    (5, 6) => palette!(Solid(Rgbw::MAGENTA)),
    (6, 6) => palette!(Cycle([Rgbw::LIME, Rgbw::WHITE])),
    (7, 6) => palette!(Cycle([Rgbw::RED, Rgbw::ORANGE, Rgbw::YELLOW, Rgbw::LIME, Rgbw::BLUE, Rgbw::VIOLET])),

    (1, 7) => palette!(Split(Rgbw::WHITE, Rgbw::RED)),
    (2, 7) => palette!(Solid(Rgbw::PEA)),
    (3, 7) => palette!(Split(Rgbw::WHITE, Rgbw::LIME)),
    (4, 7) => palette!(Split(Rgbw::WHITE, Rgbw::BLUE)),
    (5, 7) => palette!(Split(Rgbw::BLUE, Rgbw::VIOLET)),
    (6, 7) => palette!(Cycle([Rgbw::BLUE, Rgbw::WHITE])),
}

bind! {

solid_colors:
    (0, 6) => palette!(Solid(Rgbw::RGBW)),
    (1, 6) => palette!(Solid(Rgbw::RED)),
    (2, 6) => palette!(Solid(Rgbw::LIME)),
    (3, 6) => palette!(Solid(Rgbw::BLUE)),
    (4, 6) => palette!(Split(Rgbw::RED, Rgbw::VIOLET)),
    (5, 6) => palette!(Split(Rgbw::PEA, Rgbw::BLUE)),
    (6, 6) => palette!(Split(Rgbw::ORANGE, Rgbw::MAGENTA)),
    (7, 6) => palette!(Rainbow),

    (1, 7) => palette!(Solid(Rgbw::WHITE)),
    (2, 7) => palette!(Split(Rgbw::HOUSE, Rgbw::BLUE)),
    (3, 7) => palette!(Split(Rgbw::PINK, Rgbw::MINT)),
    (4, 7) => palette!(Split(Rgbw::YELLOW, Rgbw::VIOLET)),
    (5, 7) => palette!(Cycle([Rgbw::BLUE, Rgbw::ORANGE])),
    (6, 7) => palette!(Cycle([Rgbw::MINT, Rgbw::VIOLET, Rgbw::PINK])),
}

// The movers take the first colour, the pars the second.
bind! {

split_colors:
    (0, 6) => palette!(Split(Rgbw::WHITE, Rgbw::RED)),
    (1, 6) => palette!(Split(Rgbw::WHITE, Rgbw::LIME)),
    (2, 6) => palette!(Split(Rgbw::WHITE, Rgbw::BLUE)),
    (3, 6) => palette!(Split(Rgbw::RED, Rgbw::WHITE)),
    (4, 6) => palette!(Split(Rgbw::LIME, Rgbw::WHITE)),
    (5, 6) => palette!(Split(Rgbw::BLUE, Rgbw::WHITE)),
    (6, 6) => palette!(Split(Rgbw::BLUE, Rgbw::VIOLET)),
    (7, 6) => palette!(Split(Rgbw::RED, Rgbw::BLUE)),

    (1, 7) => palette!(Split(Rgbw::BLUE, Rgbw::LIME)),
    (2, 7) => palette!(Split(Rgbw::LIME, Rgbw::BLUE)),
    (3, 7) => palette!(Split(Rgbw::CYAN, Rgbw::MAGENTA)),
    (4, 7) => palette!(Split(Rgbw::MAGENTA, Rgbw::CYAN)),
    (5, 7) => palette!(Split(Rgbw::ORANGE, Rgbw::CYAN)),
    (6, 7) => palette!(Split(Rgbw::PINK, Rgbw::CYAN)),
}

bind! {

flash_colors:
    (0, 6) => palette!(Cycle([Rgbw::RED, Rgbw::WHITE])),
    (1, 6) => palette!(Cycle([Rgbw::BLUE, Rgbw::LIME])),
    (2, 6) => palette!(Cycle([Rgbw::MAGENTA, Rgbw::WHITE])),
    (3, 6) => palette!(Cycle([Rgbw::MAGENTA, Rgbw::CYAN])),
    (4, 6) => palette!(Cycle([Rgbw::CYAN, Rgbw::PEA])),
    (5, 6) => palette!(Cycle([Rgbw::PEA, Rgbw::YELLOW])),
    (6, 6) => palette!(Cycle([Rgbw::RED, Rgbw::BLUE])),
    (7, 6) => palette!(Cycle([Rgbw::ORANGE, Rgbw::HOUSE])),

    (1, 7) => palette!(Cycle([Rgbw::LIME, Rgbw::WHITE])),
    (2, 7) => palette!(Cycle([Rgbw::BLUE, Rgbw::WHITE])),
    (3, 7) => palette!(Cycle([Rgbw::CYAN, Rgbw::MAGENTA, Rgbw::YELLOW])),
    (4, 7) => palette!(Cycle([Rgbw::RED, Rgbw::LIME, Rgbw::BLUE])),
    (5, 7) => palette!(Cycle([Rgbw::WHITE, Rgbw::RED, Rgbw::WHITE, Rgbw::BLUE])),
    (6, 7) => palette!(Cycle([Rgbw::RED, Rgbw::ORANGE, Rgbw::YELLOW, Rgbw::LIME, Rgbw::BLUE, Rgbw::VIOLET])),
}

///////////////////////// EDGE BUTTONS /////////////////////////

// The pad's border, bound like the grid so each button's action and lamp live
// together. Runs in every mode; only lit while the pad guide is up.
bind! {

edge:
    // Top row: the four arrows are the colour banks
    (0, 8) => bank_op(0),
    (1, 8) => bank_op(1),
    (2, 8) => bank_op(2),
    (3, 8) => bank_op(3),
    // Normal / double / half time, which the next grid press drops
    (5, 8) => func!("half / double time", lamp: |s: &State| match s.phi_mul {
        m if m > 1.0 => Rgbw::VIOLET,
        m if m < 1.0 => Rgbw::CYAN,
        _ => Rgbw::WHITE * 0.15,
    }, |s, _| s.phi_mul = match s.phi_mul {
        m if m > 1.0 => 0.5,
        m if m < 1.0 => 1.0,
        _ => 2.0,
    }),
    // Cycle Perform -> Easy -> Auto
    (6, 8) => func!("mode", lamp: |s: &State| match s.mode {
        Mode::Perform => Rgbw::WHITE,
        Mode::Easy => Rgbw::CYAN,
        Mode::Auto => Rgbw::MAGENTA,
    }, |s, _| s.mode = s.mode.next()),
    // Toggle the pad guide, clearing whatever the other side drew
    (7, 8) => func!("pad guide", lamp: |_: &State| Rgbw::WHITE, |s, pad| {
        s.debug = !s.debug;
        pad.send(lib::midi::device::launchpad_x::Output::Clear);
    }),

    // Right column: brightness, lit up to the level in use
    (8, 0) => level_op(0),
    (8, 1) => level_op(1),
    (8, 2) => level_op(2),
    (8, 3) => level_op(3),
    (8, 4) => level_op(4),
    (8, 5) => level_op(5),
    (8, 6) => level_op(6),
    (8, 7) => level_op(7),

    // Beat indicator: white on the bar, the palette's colour on the beat
    (8, 8) => func!("beat", lamp: |s: &State| match s.pd(Pd(1, 1)).bsquare(1.0, 0.1) {
        true => match s.pd(Pd(4, 1)).bsquare(1.0, 0.2) {
            true => Rgbw::WHITE,
            false => s.palette.beam_color(s),
        },
        false => Rgbw::BLACK,
    }, |_, _| {}),

}

/// The arrow that picks colour bank `i`, lit in the bank's own colour.
fn bank_op(i: usize) -> PadOp {
    func!(format!("{} colors", banks()[i].name), lamp: move |s: &State| {
        s.banks[i].color * if s.bank == i { 1.0 } else { 0.15 }
    }, move |s, _| s.bank = i)
}

/// The button that drops brightness to `LEVELS[i]`.
fn level_op(i: usize) -> PadOp {
    func!(format!("brightness {:.0}%", LEVELS[i] * 100.0), lamp: move |s: &State| {
        match LEVELS[i] <= s.brightness {
            true => Rgbw::WHITE * LEVELS[i].max(0.15),
            false => Rgbw::BLACK,
        }
    }, move |s, _| s.brightness = LEVELS[i])
}

/// What Auto picks between, one press of the grid's own looks.
#[rustfmt::skip]
const AUTO_LOOKS: &[Look] = &[
    Look::new(Energy::Swell { pd: Pd(1, 1) }, Movement::WaveY, Pd(4, 1)),
    Look::new(Energy::Beat { pd: Pd(4, 1) }, Movement::RaisingBeams, Pd(4, 1)),
    Look::new(Energy::Beat { pd: Pd(4, 1) }, Movement::WaveY, Pd(4, 1)),
    Look::new(Energy::Beat { pd: Pd(4, 1) }, Movement::UpDownWave, Pd(4, 1)),
    Look::new(Energy::Beat { pd: Pd(4, 1) }, Movement::Whirl, Pd(4, 1)),
    Look::new(Energy::Beat { pd: Pd(4, 1) }, Movement::Twisting, Pd(4, 1)),
];

///////////////////////// STATE /////////////////////////

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Mode {
    /// Full control: bindings, specials.
    Perform,
    /// Quadrants for randos: chill / hype / new color / new style.
    Easy,
    /// Unattended: rerolls itself, presses nudge it along.
    Auto,
}

impl Mode {
    pub fn next(self) -> Self {
        match self {
            Mode::Perform => Mode::Easy,
            Mode::Easy => Mode::Auto,
            Mode::Auto => Mode::Perform,
        }
    }
}

#[derive(Resource)]
pub struct State {
    /// Bindings defined just above.
    pub perform: Vec<PadBinding>,
    /// The border buttons, bound the same way as the grid.
    pub edge: Vec<PadBinding>,
    /// Colour banks, and which of them y=6 and y=7 currently hold.
    pub banks: [Bank; 4],
    pub bank: usize,
    pub mode: Mode,

    /// Current color palette.
    pub palette: Box<dyn Palette>,
    /// Current style, an index into STYLES.
    pub style: usize,
    /// What the grid last set: energy, movement and how fast it runs.
    pub base: Look,
    /// Variation seed: bumped by reroll, picks within the style's movements.
    pub seed: usize,
    /// Active specials, later entries override earlier.
    pub specials: Vec<Active>,

    /// Total time elapsed since startup in seconds
    pub t: f32,
    /// Current approximately matched BPM
    pub bpm: f32,
    /// Timestamps when the beatmatch button was tapped
    pub bpm_taps: Vec<f32>,
    /// Current fractional beat number at the current `bpm`. Ranges from `0..64` and wraps around
    pub phi: f32,
    /// Bpm multiplier, e.g. 0.5 for half-time, 2.0 for double-time.
    pub phi_mul: f32,

    /// Manual beat
    pub beat: Option<Beat>,

    /// Global brightness modifier
    pub brightness: f32,

    /// DJ booth work light: Par.7 and Par.8 held pure white.
    pub booth: bool,

    /// Pad guide, showing what each button does, rather than the visualiser.
    pub debug: bool,
    /// Most recently pressed grid coord, which picks the visualiser's pattern.
    pub x: i8,
    pub y: i8,

    /// Last whole beat Auto rerolled on, and what it picked.
    pub auto_beat: usize,
    pub auto_look: usize,
}

/// The brightness each button of the right hand column drops to.
const LEVELS: [f32; 8] = [0.0, 0.07, 0.125, 0.3, 0.4, 0.6, 0.8, 1.0];

/// Presses this close to a predicted beat snap `phi` to it (small, so
/// way-off accidental hits don't shift the clock).
const SYNC_MOE: f32 = 0.09;

/// Gaps at least this long end the run of taps counted for BPM.
const TAP_GAP_MAX: f32 = 3.0;

impl State {
    pub fn new() -> Self {
        Self {
            perform: perform(),
            edge: edge(),
            banks: banks(),
            bank: 0,
            mode: Mode::Perform,

            palette: Box::new(Rainbow),
            style: 1,
            base: Look::new(Energy::Off, Movement::Out, Pd(4, 1)),
            seed: 0,
            specials: vec![],

            t: 0.0,
            bpm: 120.0,
            bpm_taps: vec![],
            phi: 0.0,
            phi_mul: 1.0,
            beat: None,

            brightness: 1.0,
            booth: false,
            debug: true,
            x: 0,
            y: 0,
            auto_beat: 0,
            auto_look: 0,
        }
    }

    /// What a button does: the active bank's colours over the fixed grid.
    pub fn binding(&self, xy: (i8, i8)) -> Option<&PadOp> {
        self.banks[self.bank]
            .palettes
            .iter()
            .chain(&self.perform)
            .chain(&self.edge)
            .find(|b| b.xy == xy)
            .map(|b| &b.op)
    }

    pub fn phi(&self, n: usize, d: usize) -> f32 {
        self.pd(Pd(n, d))
    }
    pub fn pd(&self, pd: Pd) -> f32 {
        self.phi.fmod_div(pd.fr())
    }

    /// Resolve the style and palette through the grid's look, then the specials
    /// stack on top of that.
    pub fn look(&self) -> Looked {
        let style = &STYLES[self.style];
        let mut look = Looked {
            energy: Energy::Off,
            movement: style.movement(self.seed),
            movement_pd: None,
            beam: self.palette.beam_color(self),
            par: self.palette.par_color(self),
            mask: Mask::ALL,
        };
        look.apply(&self.base);
        for a in &self.specials {
            look.apply(&a.special.look);
        }
        look
    }

    /// Snap `phi` to the nearest beat if this press landed within SYNC_MOE of it.
    pub fn resync(&mut self) {
        let err = self.phi - self.phi.round();
        if (err * (60.0 / self.bpm)).abs() < SYNC_MOE {
            self.phi = self.phi.round();
        }
    }

    pub fn apply_bpm(&mut self) {
        let taps = self.bpm_taps.drain(..).collect_vec();
        self.phi = 0.0;
        // Newest to oldest, keeping only the run of taps unbroken by a long gap.
        // Anchoring at `t` discards the whole run if the last tap is already stale.
        let dts = std::iter::once(self.t)
            .chain(taps.into_iter().rev())
            .tuple_windows()
            .map(|(t1, t0)| t1 - t0)
            .take_while(|dt| *dt < TAP_GAP_MAX)
            .skip(1)
            .collect_vec();
        let n = dts.len();
        if n > 0 {
            let dt = dts.iter().sum::<f32>() / n as f32;
            self.bpm = 60.0 / dt;
            info!("Calculated bpm={:.2} from {} samples", self.bpm, n + 1);
        }
    }

    fn beat_fr(&self, t: f32, pd: Pd) -> f32 {
        let dt = self.t - t;
        let len = (60.0 / self.bpm) * pd.fr();
        if dt >= len { 0.0 } else { (dt / len).ramp(1.0).inv().in_quad() }
    }
    pub fn beat_fr0(&self) -> Option<f32> {
        self.beat.map(|Beat { t0, pd0, .. }| self.beat_fr(t0, pd0))
    }
    pub fn beat_fr1(&self) -> Option<f32> {
        self.beat.map(|Beat { t1, pd1, .. }| self.beat_fr(t1, pd1))
    }
}

///////////////////////// MANUAL BEAT /////////////////////////

#[derive(Clone, Copy)]
pub struct Beat {
    /// Time of left press
    pub t0: f32,
    /// Time of right press
    pub t1: f32,

    /// Duration of left beat.
    pub pd0: Pd,
    /// Duration of right beat.
    pub pd1: Pd,
}

///////////////////////// TICK /////////////////////////

pub fn tick(mut s: ResMut<State>, time: Res<Time>) {
    let s: &mut State = &mut *s;
    let dt = time.delta_secs();

    s.t += dt;
    s.phi = (s.phi + (s.bpm / 60.0) * s.phi_mul * dt) % 64.0;

    // Expire one-shots
    let (t, bpm) = (s.t, s.bpm);
    s.specials.retain(|a| match a.special.fire {
        Fire::OneShot { pd } => (t - a.t0) * (bpm / 60.0) < pd.fr(),
        _ => true,
    });

    // Auto: a new look every 16 beats, a new colour with it every 32
    if let Mode::Auto = s.mode {
        let beat = s.phi as usize;
        if beat != s.auto_beat {
            s.auto_beat = beat;
            if beat % 16 == 0 {
                let mut rng = ThreadRng::default();
                s.seed += 1;
                // Never the one already running, or the reroll shows nothing.
                let mut i = rng.gen_range(0..AUTO_LOOKS.len());
                if i == s.auto_look {
                    i = (i + 1) % AUTO_LOOKS.len();
                }
                s.auto_look = i;
                s.base = AUTO_LOOKS[i];
                if beat % 32 == 0 {
                    s.palette = random_palette(&mut rng);
                }
            }
        }
    }
}

///////////////////////// LIGHTS /////////////////////////

pub fn render_lights(
    s: Res<State>,
    mut l: ResMut<Lights>,
    trim: Res<Trim>,
    dmx: Res<crate::dmx::Dmx>,
    mut universe: ResMut<Universe>,
) {
    let s: &State = &s;
    let look = s.look();
    let pd = look.pd();

    // Left column flashes the pars' colour group, right column the movers'.
    let beat0 = s.beat_fr0().unwrap_or(1.0);
    let beat1 = s.beat_fr1().unwrap_or(1.0);
    let pars_mask = look.mask.par();

    // Hut side lights: both bars and par 6 hold a dim wash on the hut
    // through every broad look, dropping out for solos and blackouts.
    let side = match look.energy {
        Energy::Off => 0.0,
        _ if look.mask.broad() => trim.hut,
        _ => 0.0,
    };

    l.reset();

    // The movers in stage order: classic movements still drive the 60w Beams
    // and 90w BigBeams as the four mslive pairs, semantic ones aim each of
    // the eight by its place in the line. Colour and beat follow the mover's
    // checkerboard group, brightness its bank's trim.
    for m in MOVERS {
        let (pitch, yaw) = look.angles(s, pd, m);
        let env = look.energy.env(s, m.x) * look.movement.mask(s, pd, m) * look.mask.mover(m);
        let (group, beat) = match m.alt {
            true => (look.par, beat0),
            false => (look.beam, beat1),
        };
        let col = group * env * s.brightness * beat * trim.global;

        if m.big {
            let big = &mut l.bigbeams[m.i];
            big.pitch = pitch;
            big.yaw = yaw;
            big.speed = 1.0;
            big.color = col * trim.bigbeam;
        } else {
            let beam = &mut l.beams[m.i];
            beam.pitch = pitch;
            beam.yaw = yaw;
            beam.speed = 1.0;
            beam.color = col * trim.beam;
            // Ring control isn't wired up yet; only the dmx debug panel drives it.
            beam.ring = BeamRing::Off;
        }
    }

    for (i, p) in l.pars.iter_mut().enumerate() {
        let fr = i as f32 / 9.0;
        let env = look.energy.env(s, fr) * pars_mask;
        let (group, beat) = match PAR_ALT[i] {
            true => (look.beam, beat1),
            false => (look.par, beat0),
        };
        let mut lit = env * beat;
        if i == 6 {
            lit = lit.max(side);
        }
        p.color = group * lit * s.brightness * trim.global * trim.par;
    }

    // The booth work light: the two pars inside the DJ booth run full white
    // over whatever the show is doing, past every trim.
    if s.booth {
        for i in [7, 8] {
            l.pars[i].color = Rgbw::WHITE;
        }
    }

    // Spiders follow the movers' colour, both banks, with the gesture that
    // fits the energy.
    let spider_pattern = SpiderPattern::of(look.energy);
    for (i, sp) in l.spiders.iter_mut().enumerate() {
        let fr = i as f32 / 2.0;
        let (pos0, pos1) = spider_pattern.pos(s, i);
        sp.pos0 = pos0;
        sp.pos1 = pos1;
        let env = look.energy.env(s, fr) * look.mask.spider();
        let col = look.beam * env * s.brightness * beat1 * trim.global * trim.spider;
        sp.color0 = col;
        sp.color1 = col;
        sp.alpha = 1.0;
    }

    // Bars sit with the movers' colour group, floored at the hut wash.
    for (i, bar) in l.bars.iter_mut().enumerate() {
        let fr = i as f32 / 2.0;
        let env = look.energy.env(s, fr) * look.mask.bar();
        bar.color = look.beam * (env * beat1).max(side) * s.brightness * trim.global * trim.bar;
        bar.alpha = 1.0;
    }

    // The strobe flashes with the pars.
    let env = look.energy.env(s, 0.0) * look.mask.flash();
    l.strobe.color = Rgb::from(look.par) * env * s.brightness * beat0 * trim.global * trim.strobe;
    l.strobe.alpha = 1.0;

    l.encode(&mut universe, &dmx.addressing());
}

///////////////////////// PAD INPUT /////////////////////////

pub fn on_pad(mut s: ResMut<State>, mut pad: ResMut<Pad>) {
    use lib::midi::device::launchpad_x::Input;
    use lib::midi::device::launchpad_x::types::*;

    let s: &mut State = &mut *s;
    let pad: &mut Pad = &mut *pad;

    for input in pad.recv() {
        // Pop hold specials
        if let Input::Release(i) = input {
            let Coord(x, y) = i.into();
            s.specials.retain(|a| !(matches!(a.special.fire, Fire::Hold) && a.xy == (x, y)));
        }

        let Some((x, y)) = input.xy() else { continue };

        // The border runs its binding in every mode; only the grid is moded.
        if x > 7 || y > 7 {
            if let Some(PadOp::Func { func, .. }) = s.binding((x, y)).cloned() {
                func(s, pad);
            }
            continue;
        }
        debug!("Pad({x}, {y})");
        (s.x, s.y) = (x, y);
        s.phi_mul = 1.0;

        // A manual beat outlives the beat and palette buttons only; anything
        // else is a new look and clears it.
        let is_beat = (x == 0 || x == 7) && y < 5;
        let is_palette = y == 6 || (y == 7 && x > 0 && x < 7);
        if !is_beat && !is_palette {
            s.beat = None;
        }

        match s.mode {
            Mode::Perform => {
                let Some(op) = s.binding((x, y)).cloned() else {
                    continue;
                };

                match op {
                    PadOp::Func { func, .. } => func(s, pad),
                    PadOp::Energy(e) => {
                        if Some(e) == s.base.energy {
                            s.seed += 1;
                        } else {
                            s.base.energy = Some(e);
                        }
                        s.resync();
                    }
                    PadOp::Look(look) => {
                        s.base = look;
                        // Every press rerolls whatever is seeded, e.g. the
                        // chase order.
                        s.seed += 1;
                        s.resync();
                    }
                    PadOp::Palette(palette) => s.palette = palette,
                    PadOp::Special(sp) => {
                        let latched =
                            matches!(sp.fire, Fire::Latch) && s.specials.iter().any(|a| a.xy == (x, y));
                        s.specials.retain(|a| a.xy != (x, y));
                        if !latched {
                            s.specials.push(Active { xy: (x, y), t0: s.t, special: sp });
                        }
                    }
                    PadOp::Beat { side, pd } => {
                        s.resync();
                        match &mut s.beat {
                            Some(beat) => match side {
                                0 => {
                                    beat.t0 = s.t;
                                    beat.pd0 = pd;
                                }
                                _ => {
                                    beat.t1 = s.t;
                                    beat.pd1 = pd;
                                }
                            },
                            None => match side {
                                0 => s.beat = Some(Beat { t0: s.t, t1: 0.0, pd0: pd, pd1: pd }),
                                _ => s.beat = Some(Beat { t0: 0.0, t1: s.t, pd0: pd, pd1: pd }),
                            },
                        }
                    }
                }
            }
            // Movement is left to the style, so the quadrant that rerolls it
            // has something to change.
            Mode::Easy => {
                let mut rng = ThreadRng::default();
                let energy = |e| Look { energy: Some(e), ..Look::NONE };
                match (x < 4, y < 4) {
                    (true, true) => s.base = energy(Energy::Beat { pd: Pd(2, 1) }),
                    (false, true) => s.base = energy(Energy::Beat { pd: Pd(1, 1) }),
                    (true, false) => s.palette = random_palette(&mut rng),
                    (false, false) => s.style = rng.gen_range(0..STYLES.len()),
                }
                s.seed += 1;
                s.resync();
            }
            // Any press = a nudge
            Mode::Auto => {
                s.seed += 1;
                s.resync();
            }
        }
    }
}

///////////////////////// PAD OUTPUT /////////////////////////

pub fn render_pad(s: Res<State>, mut pad: ResMut<Pad>) {
    use lib::midi::device::launchpad_x::Output;
    use lib::midi::device::launchpad_x::types::*;

    let s: &State = &s;

    let mut batch: Vec<(Pos, Color)> = vec![];
    let rgb = |Rgb(r, g, b): Rgb| Color::Rgb(r, g, b);
    let mut set = |x, y, color: Rgbw| batch.push((Coord(x, y).into(), rgb(color.into())));

    match s.mode {
        Mode::Perform if !s.debug => visualise(s, &mut set),
        Mode::Perform => {
            let beam = s.palette.beam_color(s);
            for PadBinding { xy: (x, y), op } in s.perform.iter().chain(&s.banks[s.bank].palettes)
            {
                match op {
                    PadOp::Energy(e) => {
                        let on = Some(*e) == s.base.energy;
                        let fr = if on { e.env(s, 0.0).max(0.3) } else { 0.1 };
                        set(*x, *y, Rgbw::WHITE * fr);
                    }
                    // Each button beats at the rate it would drive the rig
                    // at, in the colour it would use: the left half of the
                    // grid in the movers' colour, the right half the pars',
                    // so the palette's two tones read at a glance.
                    PadOp::Look(look) => {
                        let side = if *x <= 3 { beam } else { s.palette.par_color(s) };
                        let col = look.color.unwrap_or(side);
                        let env = look.energy.map_or(0.0, |e| e.env(s, 0.0));
                        set(*x, *y, col * env.max(0.15));
                    }
                    PadOp::Palette(palette) => set(*x, *y, palette.beam_color(s)),
                    PadOp::Special(sp) => {
                        let active = s.specials.iter().any(|a| a.xy == (*x, *y));
                        set(*x, *y, sp.color * if active { 1.0 } else { 0.2 });
                    }
                    PadOp::Beat { .. } => set(*x, *y, Rgbw::VIOLET),
                    PadOp::Func { lamp, .. } => set(*x, *y, lamp(s)),
                }
            }

            // Beat buttons: upwards propagating wave at BPM
            for i in 0..=4 {
                let col = Rgbw::WHITE * (s.phi - i as f32 * 0.2).fsin(2.0);
                set(0, i, col);
                set(7, i, col);
            }
        }
        Mode::Easy => {
            let look = s.look();
            for x in 0..8 {
                for y in 0..8 {
                    let col = match (x < 4, y < 4) {
                        (true, true) => Rgbw::BLUE * s.pd(Pd(2, 1)).ramp(1.0).inv(),
                        (false, true) => Rgbw::RED * s.pd(Pd(1, 1)).ramp(1.0).inv(),
                        (true, false) => look.beam,
                        (false, false) => STYLES[s.style].color,
                    };
                    set(x, y, col * 0.5);
                }
            }
        }
        Mode::Auto => {
            let look = s.look();
            let env = look.energy.env(s, 0.0);
            for x in 0..8 {
                for y in 0..8 {
                    set(x, y, look.beam * env);
                }
            }
        }
    }

    // The border only lights with the pad guide; the visualiser keeps it
    // dark. The guide toggle clears the pad, so nothing stale lingers.
    if s.debug {
        for PadBinding { xy: (x, y), op } in &s.edge {
            if let PadOp::Func { lamp, .. } = op {
                set(*x, *y, lamp(s));
            }
        }
    }

    pad.send(Output::Batch(batch));
}

///////////////////////// PAD VISUALISER /////////////////////////

/// The pad as decoration rather than a guide: whatever the rig is doing,
/// drawn across the grid.
fn visualise(s: &State, set: &mut impl FnMut(i8, i8, Rgbw)) {
    let look = s.look();
    let col = look.beam;

    // A manual beat takes over: two waves crossing the grid from either side.
    if let Some(Beat { t0, t1, pd0, pd1 }) = s.beat {
        let edge = |t: f32, pd: Pd, from: f32| {
            let len = (60.0 / s.bpm) * pd.div(2).fr();
            let t = (s.t - t) / len - from;
            if (0.0..1.0).contains(&t) { t.ramp(1.0).inv().in_quad() } else { 0.0 }
        };
        for x in 0..8 {
            let fr0 = edge(t0, pd0, x as f32 / 8.0);
            let fr1 = edge(t1, pd1, 1.0 - x as f32 / 8.0 - 0.125);
            for y in 0..8 {
                set(x, y, col * fr0.max(fr1));
            }
        }
        return;
    }

    match (look.energy, look.movement) {
        (Energy::Off, _) => fill(set, Rgbw::BLACK),
        // The crest crossing the grid just as it crosses the rig.
        (_, Movement::Ripple { dir, size }) => {
            let env = look.energy.env(s, 0.0);
            for x in 0..8 {
                let crest = ripple(s, look.pd(), dir, size, x as f32 / 7.0);
                for y in 0..8 {
                    set(x, y, col * env * crest.lerp(0.1..1.0));
                }
            }
        }
        (_, Movement::Spin) => spiral(set, s.t, col * look.energy.env(s, 0.0), 6.0),
        (Energy::On, Movement::Whirl | Movement::Carousel) => spiral(set, s.t, col, 8.0),
        (Energy::On, Movement::RaisingBeams | Movement::WaveY | Movement::UpDownWave) => {
            for x in 0..8 {
                for y in 0..8 {
                    let env = s.phi(4, 1).ramp(1.0).phase(1.0, y as f32 / 16.0).out_exp();
                    set(x, y, col * env.inv());
                }
            }
        }
        (Energy::On, _) => fill(set, col),
        // The wave the last press picked, running whichever way it was bound.
        (Energy::Beat { .. }, _) if s.x == 5 => spiral(set, s.t, col, -8.0),
        (Energy::Beat { .. }, _) => {
            let wave = |t: f32| col * t.fsin(2.0).inout_exp();
            for i in 0..8 {
                for j in 0..8 {
                    let (i, j) = (i as f32, j as f32);
                    let (x, y, t) = match s.x {
                        1 => (j, i, s.phi - i * 0.125),
                        2 => (i, j, s.phi - i * 0.125),
                        3 => (i, j, s.phi - i * 0.125 + j * 0.125),
                        4 => (j, i, s.phi - i * 0.125 + j * 0.125),
                        _ => (j, i, s.phi + i * 0.125),
                    };
                    set(x as i8, y as i8, wave(t));
                }
            }
        }
        (Energy::Strobe { .. }, _) => fill(set, col * look.energy.env(s, 0.0)),
        (Energy::Chase { pd }, _) => {
            let env = s.pd(pd).square(1.0, 0.6);
            spiral(set, s.t, Rgbw::WHITE * env, 12.0);
        }
        (Energy::Alternate { .. }, _) => spiral(set, s.t, col, 12.0),
        _ => fill(set, Rgbw::BLACK),
    }
}

fn fill(set: &mut impl FnMut(i8, i8, Rgbw), col: Rgbw) {
    for x in 0..8 {
        for y in 0..8 {
            set(x, y, col);
        }
    }
}

fn spiral(set: &mut impl FnMut(i8, i8, Rgbw), t: f32, col: Rgbw, speed: f32) {
    for x in 0..8 {
        for y in 0..8 {
            let (u, v) = ((x as f32 / 7.0) * 2.0 - 1.0, (y as f32 / 7.0) * 2.0 - 1.0);
            let (r, theta) = ((u * u + v * v).sqrt(), v.atan2(u));
            let fr = ((2.0 / r) + (2.0 * theta) + (speed * t)).sin();
            set(x, y, col * fr.map(-1.0..1.0, 0.0..1.0).inout_quad());
        }
    }
}

///////////////////////// CTRL /////////////////////////

pub fn on_ctrl(mut s: ResMut<State>, mut ctrl: ResMut<Ctrl>, mut trim: ResMut<Trim>) {
    use lib::midi::device::launch_control_xl::Input;

    for input in ctrl.recv() {
        debug!("ctrl: {input:?}");

        match input {
            Input::Slider(0, fr) => s.brightness = fr,
            Input::Slider(1, fr) => trim.beam = fr,
            Input::Slider(2, fr) => trim.bigbeam = fr,
            Input::Slider(3, fr) => trim.par = fr,
            Input::Slider(4, fr) => trim.spider = fr,
            Input::Slider(5, fr) => trim.bar = fr,
            Input::Slider(6, fr) => trim.strobe = fr,
            Input::Slider(7, fr) => trim.global = fr,

            // The booth work light.
            Input::Record(true) => s.booth = !s.booth,
            _ => {}
        }
    }
}

///////////////////////// CTRL OUTPUT /////////////////////////

/// The Record lamp, showing the booth work light's state. Resent whenever it
/// or the connection changes, since the device clears its LEDs when it comes
/// up.
pub fn render_ctrl(s: Res<State>, mut ctrl: ResMut<Ctrl>, mut last: Local<Option<(bool, bool)>>) {
    use lib::midi::device::launch_control_xl::types::*;
    use lib::midi::device::launch_control_xl::{Led, Output};

    let now = (s.booth, ctrl.connected());
    if *last == Some(now) {
        return;
    }
    *last = Some(now);

    let mut batch = vec![];

    // Record only lights amber, so the toggle reads by brightness.
    let booth_brightness = if s.booth { Brightness::High } else { Brightness::Low };
    batch.push((Led::Record, Color::Amber, booth_brightness));

    ctrl.send(Output::Batch(batch));
}
