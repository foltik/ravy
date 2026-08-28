use itertools::Itertools;
use lib::prelude::*;
use rand::Rng;
use rand::rngs::ThreadRng;

use super::Swatch;
use super::ambient;
use super::look::*;
use super::palette::*;
use super::special::*;
use crate::blackout::Blackout;
use crate::dmx::Patch;
use crate::home::Home;
use crate::lights::{Lights, Wheels};
use crate::logic::{PadBinding, PadOp};
use crate::mirror::{Ctrl, Pad};
use crate::rig::Trim;
use crate::sim::Movers;
use crate::{beat, bind, func, look, palette};

///////////////////////// BINDINGS /////////////////////////

// The optics the texture pieces carry. Rotation bytes are physical: 200 is a
// ~40 deg/s counter-clockwise grind, 188 a ~17 deg/s clockwise crawl.
const T_POOLS: Texture = Texture { gobo: Gobo::Dots, gobo_rot: 199, focus: 0.25, zoom: 1.0, ..Texture::OPEN };
const T_RAKE: Texture = Texture { gobo: Gobo::Lines, focus: 0.3, zoom: 0.5, ..Texture::OPEN };
const T_FLOWER: Texture = Texture { gobo: Gobo::Flower, prism: Prism::Circular, prism_rot: 0.12, focus: 0.3, zoom: 0.2, ..Texture::OPEN };
const T_HAZE: Texture = Texture { gobo: Gobo::Dots, gobo_rot: 188, focus: 0.35, zoom: 0.25, ..Texture::OPEN };
const T_BREATHE: Texture = Texture { breathe: 1.0, ..Texture::OPEN };
const T_GRIND: Texture = Texture { gobo: Gobo::Lines, gobo_rot: 200, focus: 0.4, ..Texture::OPEN };

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
    (1, 0) => look!(Look::new(Energy::Off, Movement::Home, Pd(4, 1))),
    (2, 0) => look!(Look::new(Energy::On, Movement::Out, Pd(4, 1)).masked(Mask::Movers)),
    (3, 0) => look!(Look::new(Energy::On, Movement::WaveY, Pd(4, 1)).masked(Mask::Movers)),
    (4, 0) => look!(Look::new(Energy::On, Movement::RaisingBeams, Pd(8, 1)).masked(Mask::Movers)),
    (5, 0) => look!(Look::new(Energy::On, Movement::Whirl, Pd(16, 1)).masked(Mask::Movers)),
    (6, 0) => look!(Look::new(Energy::On, Movement::UpDownWave, Pd(4, 1)).masked(Mask::Movers)),

    // --- y=1: still low, but moving: pairs and wanders, the pars only ever
    // sparse through the spice ---
    (1, 1) => look!(Look::new(Energy::On, Movement::Scissor, Pd(8, 1)).masked(Mask::Movers)),
    (2, 1) => look!(Look::new(Energy::On, Movement::Pendulum, Pd(8, 1)).masked(Mask::Movers)),
    (3, 1) => look!(Look::new(Energy::On, Movement::Twisting, Pd(4, 1))),
    (4, 1) => look!(Look::new(Energy::On, Movement::Spinner, Pd(8, 1)).masked(Mask::Movers)),
    (5, 1) => look!(Look::new(Energy::On, Movement::DarthMaul, Pd(8, 1)).masked(Mask::Movers)),
    (6, 1) => look!(Look { texture: Some(T_BREATHE), ..Look::new(Energy::On, Movement::UpDownWave, Pd(8, 1)) }),

    // --- y=2: the hydros' optics, kept low. Gobo pools and rakes on the
    // dance floor, texture in the haze, the beams breathing wide: pieces
    // that leave most of the rig dark. ---
    (1, 2) => look!(Look { texture: Some(T_POOLS), ..Look::new(Energy::On, Movement::FloorCircle, Pd(16, 1)).masked(Mask::Hydros) }),
    (2, 2) => look!(Look { texture: Some(T_RAKE), ..Look::new(Energy::On, Movement::FloorSweep, Pd(8, 1)).masked(Mask::Hydros) }),
    (3, 2) => look!(Look { texture: Some(T_FLOWER), ..Look::new(Energy::On, Movement::FloorCircle, Pd(32, 1)).masked(Mask::Hydros) }),
    (4, 2) => look!(Look { texture: Some(T_HAZE), ..Look::new(Energy::On, Movement::Out, Pd(4, 1)).masked(Mask::Movers) }),
    (5, 2) => look!(Look { texture: Some(T_BREATHE), ..Look::new(Energy::On, Movement::WaveY, Pd(8, 1)).masked(Mask::Movers) }),
    (6, 2) => look!(Look { texture: Some(T_GRIND), ..Look::new(Energy::On, Movement::Backstage, Pd(16, 1)).masked(Mask::Hydros) }),

    // --- y=3: pulses, two rows of near-identical beats boiled down to one:
    // the swells, then the half-note beats. ---
    (1, 3) => look!(Look::new(Energy::Swell { pd: Pd(1, 1) }, Movement::WaveY, Pd(4, 1))),
    (2, 3) => look!(Look::new(Energy::Swell { pd: Pd(1, 2) }, Movement::Square, Pd(4, 1))),
    (3, 3) => look!(Look::new(Energy::Beat { pd: Pd(4, 1) }, Movement::RaisingBeams, Pd(4, 1))),
    (4, 3) => look!(Look::new(Energy::Beat { pd: Pd(2, 1) }, Movement::WaveY, Pd(2, 1))),
    (5, 3) => look!(Look::new(Energy::Beat { pd: Pd(2, 1) }, Movement::Whirl, Pd(2, 1))),
    (6, 3) => look!(Look::new(Energy::Beat { pd: Pd(2, 1) }, Movement::Twisting, Pd(2, 1))),

    (1, 4) => look!(Look::new(Energy::Beat { pd: Pd(1, 1) }, Movement::Square, Pd(1, 1))),
    (2, 4) => look!(Look::new(Energy::Beat { pd: Pd(1, 1) }, Movement::RaisingBeams, Pd(1, 1))),
    (3, 4) => look!(Look::new(Energy::Beat { pd: Pd(1, 1) }, Movement::WaveY, Pd(1, 1))),
    (4, 4) => look!(Look::new(Energy::Beat { pd: Pd(1, 1) }, Movement::UpDownWave, Pd(1, 1))),
    (5, 4) => look!(Look::new(Energy::Beat { pd: Pd(1, 1) }, Movement::Whirl, Pd(1, 1))),
    (6, 4) => look!(Look::new(Energy::Beat { pd: Pd(1, 1) }, Movement::Twisting, Pd(1, 1))),

    // --- y=5: strobes and chases, the rings running wild with them ---
    (0, 5) => look!(Look::new(Energy::Strobe { pd: Pd(1, 8), duty: 1.0 }, Movement::Square, Pd(2, 1)).masked(Mask::Pars)),
    (1, 5) => look!(Look::new(Energy::Alternate { pd: Pd(1, 4) }, Movement::CrossSway, Pd(1, 4)).masked(Mask::Movers).ringed(RingPattern::FlipZigzag)),
    (2, 5) => look!(Look::new(Energy::Strobe { pd: Pd(1, 4), duty: 1.0 }, Movement::Square, Pd(2, 1)).ringed(RingPattern::RotateTwelve)),
    (3, 5) => look!(Look::new(Energy::Strobe { pd: Pd(1, 8), duty: 1.0 }, Movement::Square, Pd(2, 1)).ringed(RingPattern::FlipRandom)),
    (4, 5) => look!(Look::new(Energy::Chase { pd: Pd(1, 2) }, Movement::Twisting, Pd(1, 2)).ringed(RingPattern::RotateSix)),
    (5, 5) => look!(Look::new(Energy::Chase { pd: Pd(1, 1) }, Movement::Twisting, Pd(1, 2)).colored(Swatch::WHITE).ringed(RingPattern::FlipZigzag)),
    (6, 5) => look!(Look::new(Energy::Chase { pd: Pd(1, 2) }, Movement::Twisting, Pd(1, 2)).colored(Swatch::WHITE).ringed(RingPattern::RotateTwelve)),
    (7, 5) => look!(Look::new(Energy::Chase { pd: Pd(1, 4) }, Movement::Twisting, Pd(1, 2)).colored(Swatch::WHITE).ringed(RingPattern::FlipRandom)),

}

///////////////////////// PALETTE BANKS /////////////////////////

/// A bankful of colour, filling y=6 and y=7. The arrows pick between them.
pub struct Bank {
    pub name: &'static str,
    /// What the arrow that picks this bank lights up as.
    pub color: Rgbw,
    pub palettes: Vec<PadBinding>,
}

/// Warm tones, cool tones, and the strobing cycles; each with a white.
pub fn banks() -> [Bank; 3] {
    [
        Bank { name: "warm", color: Rgbw::ORANGE, palettes: warm_colors() },
        Bank { name: "cool", color: Rgbw::CYAN, palettes: cool_colors() },
        Bank { name: "flash", color: Rgbw::MAGENTA, palettes: flash_colors() },
    ]
}

// Every hue is a Toned: what a press means this time is rolled, solid or
// paired off with one of its partners. The bottom rows park a hydro wheel on
// the half step between two slots, both filters split across one beam.
bind! {

warm_colors:
    (0, 6) => palette!(Toned { base: Swatch::WHITE,   pals: &[Swatch::RED, Swatch::HOUSE, Swatch::CTO] }),
    (1, 6) => palette!(Toned { base: Swatch::RED,     pals: &[Swatch::WHITE, Swatch::ORANGE, Swatch::MAGENTA, Swatch::BLUE] }),
    (2, 6) => palette!(Toned { base: Swatch::ORANGE,  pals: &[Swatch::WHITE, Swatch::RED, Swatch::HOUSE, Swatch::CYAN] }),
    (3, 6) => palette!(Toned { base: Swatch::HOUSE,   pals: &[Swatch::WHITE, Swatch::ORANGE, Swatch::CTO, Swatch::RED] }),
    (4, 6) => palette!(Toned { base: Swatch::YELLOW,  pals: &[Swatch::WHITE, Swatch::PEA, Swatch::ORANGE, Swatch::LAVENDER] }),
    (5, 6) => palette!(Toned { base: Swatch::PINK,    pals: &[Swatch::WHITE, Swatch::MAGENTA, Swatch::LAVENDER, Swatch::CYAN] }),
    (6, 6) => palette!(Toned { base: Swatch::MAGENTA, pals: &[Swatch::WHITE, Swatch::PINK, Swatch::CYAN, Swatch::BLUE] }),
    (7, 6) => palette!(Toned { base: Swatch::CTO,     pals: &[Swatch::WHITE, Swatch::HOUSE, Swatch::RED] }),

    (1, 7) => palette!(Solid(Swatch::RED_BLUE)),
    (2, 7) => palette!(Solid(Swatch::AMBER_ORANGE)),
    (3, 7) => palette!(Solid(Swatch::PEA_YELLOW)),
}

bind! {

cool_colors:
    (0, 6) => palette!(Toned { base: Swatch::WHITE,    pals: &[Swatch::BLUE, Swatch::CYAN, Swatch::LAVENDER] }),
    (1, 6) => palette!(Toned { base: Swatch::BLUE,     pals: &[Swatch::WHITE, Swatch::CYAN, Swatch::VIOLET, Swatch::RED] }),
    (2, 6) => palette!(Toned { base: Swatch::CYAN,     pals: &[Swatch::WHITE, Swatch::BLUE, Swatch::MINT, Swatch::MAGENTA] }),
    (3, 6) => palette!(Toned { base: Swatch::MINT,     pals: &[Swatch::WHITE, Swatch::CYAN, Swatch::PEA, Swatch::BLUE] }),
    (4, 6) => palette!(Toned { base: Swatch::LIME,     pals: &[Swatch::WHITE, Swatch::PEA, Swatch::BLUE, Swatch::HOUSE] }),
    (5, 6) => palette!(Toned { base: Swatch::PEA,      pals: &[Swatch::WHITE, Swatch::LIME, Swatch::YELLOW, Swatch::CYAN] }),
    (6, 6) => palette!(Toned { base: Swatch::LAVENDER, pals: &[Swatch::WHITE, Swatch::VIOLET, Swatch::PINK, Swatch::YELLOW] }),
    (7, 6) => palette!(Toned { base: Swatch::VIOLET,   pals: &[Swatch::WHITE, Swatch::MAGENTA, Swatch::BLUE, Swatch::LAVENDER] }),

    (1, 7) => palette!(Solid(Swatch::MAGENTA_CYAN)),
}

// A Cycle whose steps are all neighbours on the same wheel has the Hydros
// rocking colour and gobo wheels along with it; anything further apart could
// never arrive, so those cycles run without the Hydros at all.
bind! {

flash_colors:
    (0, 6) => palette!(Cycle([Swatch::RED, Swatch::WHITE])),
    (1, 6) => palette!(Cycle([Swatch::BLUE, Swatch::LIME])),
    (2, 6) => palette!(Cycle([Swatch::MAGENTA, Swatch::WHITE])),
    (3, 6) => palette!(Cycle([Swatch::MAGENTA, Swatch::CYAN])),
    (4, 6) => palette!(Cycle([Swatch::CYAN, Swatch::PEA])),
    (5, 6) => palette!(Cycle([Swatch::PEA, Swatch::YELLOW])),
    (6, 6) => palette!(Cycle([Swatch::RED, Swatch::BLUE])),
    (7, 6) => palette!(Cycle([Swatch::ORANGE, Swatch::HOUSE])),

    (1, 7) => palette!(Cycle([Swatch::LIME.open(), Swatch::WHITE])),
    (2, 7) => palette!(Cycle([Swatch::BLUE.open(), Swatch::WHITE])),
    (3, 7) => palette!(Cycle([Swatch::CYAN.open(), Swatch::MAGENTA.open(), Swatch::YELLOW.open()])),
    (4, 7) => palette!(Cycle([Swatch::RED.open(), Swatch::LIME.open(), Swatch::BLUE.open()])),
    (5, 7) => palette!(Cycle([Swatch::WHITE, Swatch::RED.open(), Swatch::WHITE, Swatch::BLUE.open()])),
    (6, 7) => palette!(Cycle([Swatch::RED.open(), Swatch::ORANGE.open(), Swatch::YELLOW.open(), Swatch::LIME.open(), Swatch::BLUE.open(), Swatch::VIOLET.open()])),
}

///////////////////////// LOOK POOLS /////////////////////////

/// The low end: solos, slow pieces and floor work, the level Auto lives at
/// and Simple's low class draws from.
#[rustfmt::skip]
pub const LOW: &[Look] = &[
    Look::new(Energy::On, Movement::Out, Pd(4, 1)).masked(Mask::Movers),
    Look::new(Energy::On, Movement::WaveY, Pd(4, 1)).masked(Mask::Movers),
    Look::new(Energy::On, Movement::RaisingBeams, Pd(8, 1)).masked(Mask::Movers),
    Look::new(Energy::On, Movement::UpDownWave, Pd(4, 1)).masked(Mask::Movers),
    Look::new(Energy::On, Movement::Scissor, Pd(8, 1)).masked(Mask::Movers),
    Look::new(Energy::On, Movement::Pendulum, Pd(8, 1)).masked(Mask::Movers),
    Look::new(Energy::On, Movement::Twisting, Pd(4, 1)),
    Look { texture: Some(T_BREATHE), ..Look::new(Energy::On, Movement::WaveY, Pd(8, 1)).masked(Mask::Movers) },
    Look { texture: Some(T_GRIND), ..Look::new(Energy::On, Movement::Backstage, Pd(16, 1)).masked(Mask::Hydros) },
    Look { texture: Some(T_POOLS), ..Look::new(Energy::On, Movement::FloorCircle, Pd(16, 1)).masked(Mask::Hydros) },
];

/// The middle: swells and pulses, nothing crazy.
#[rustfmt::skip]
pub const MID: &[Look] = &[
    Look::new(Energy::Swell { pd: Pd(1, 1) }, Movement::WaveY, Pd(4, 1)),
    Look::new(Energy::Swell { pd: Pd(1, 2) }, Movement::Square, Pd(4, 1)),
    Look::new(Energy::Beat { pd: Pd(4, 1) }, Movement::RaisingBeams, Pd(4, 1)),
    Look::new(Energy::Beat { pd: Pd(2, 1) }, Movement::WaveY, Pd(2, 1)),
    Look::new(Energy::Beat { pd: Pd(2, 1) }, Movement::Whirl, Pd(2, 1)),
    Look::new(Energy::Beat { pd: Pd(2, 1) }, Movement::Twisting, Pd(2, 1)),
];

/// The top Simple reaches: light strobes and chases, where the rings hit.
#[rustfmt::skip]
pub const HIGH: &[Look] = &[
    Look::new(Energy::Strobe { pd: Pd(1, 4), duty: 1.0 }, Movement::Square, Pd(2, 1)).ringed(RingPattern::RotateTwelve),
    Look::new(Energy::Strobe { pd: Pd(1, 8), duty: 1.0 }, Movement::Square, Pd(2, 1)).masked(Mask::Pars),
    Look::new(Energy::Chase { pd: Pd(1, 2) }, Movement::Twisting, Pd(1, 2)).ringed(RingPattern::RotateSix),
    Look::new(Energy::Chase { pd: Pd(1, 1) }, Movement::Twisting, Pd(1, 2)).ringed(RingPattern::FlipZigzag),
    Look::new(Energy::Alternate { pd: Pd(1, 4) }, Movement::CrossSway, Pd(1, 4)).masked(Mask::Movers).ringed(RingPattern::FlipZigzag),
];

/// Gaps Auto draws between changes, in beats: multiples of eight, mixed so
/// the swaps drift against the 64-beat measure rather than landing with it.
const AUTO_STEPS: [usize; 7] = [8, 8, 16, 16, 24, 32, 40];

/// One of Simple's four energy classes: the looks a press draws from, and
/// how hard it runs.
pub struct SimpleClass {
    pub color: Rgbw,
    /// Multiplier under the panel's simple dim.
    pub level: f32,
    pub looks: &'static [Look],
}

/// Off / low / mid / high, left to right across the bottom of the pad.
/// Coloured along an ironbow ramp, cold blue up to white-hot yellow.
#[rustfmt::skip]
pub const SIMPLE_CLASSES: [SimpleClass; 4] = [
    // Full off.
    SimpleClass { color: Rgbw(0.0, 0.35, 1.0, 0.0), level: 1.0,
        looks: &[Look::new(Energy::Off, Movement::Home, Pd(4, 1))] },
    SimpleClass { color: Rgbw(0.75, 0.0, 0.95, 0.0), level: 0.7, looks: LOW },
    SimpleClass { color: Rgbw(1.0, 0.2, 0.0, 0.0), level: 1.0, looks: MID },
    SimpleClass { color: Rgbw(1.0, 0.8, 0.0, 0.0), level: 1.0, looks: HIGH },
];

///////////////////////// STATE /////////////////////////

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Mode {
    /// Full control: bindings, browser, specials.
    Perform,
    /// For guest DJs: colours, four energy classes, tap tempo.
    Simple,
    /// Unattended: rerolls itself on a drifting schedule, taps retime it.
    Auto,
    /// The sun runs the rig; the pad is dark.
    Ambient,
}

impl Mode {
    pub fn next(self) -> Self {
        match self {
            Mode::Perform => Mode::Simple,
            Mode::Simple => Mode::Auto,
            Mode::Auto => Mode::Ambient,
            Mode::Ambient => Mode::Perform,
        }
    }
}

#[derive(Resource)]
pub struct State {
    /// Bindings defined just above.
    pub perform: Vec<PadBinding>,
    /// Colour banks, and which of them y=6 and y=7 currently hold.
    pub banks: [Bank; 3],
    pub bank: usize,
    pub mode: Mode,

    /// Simple's colour selectors.
    pub simple: Vec<PadBinding>,
    /// Simple's master multiplier, set from the panel.
    pub simple_dim: f32,
    /// The active energy class, and which of its movements is running.
    pub simple_class: usize,
    pub simple_look: usize,
    /// The phase reset lights only while it is down.
    pub reset_held: bool,

    /// Ambient ignores the wall clock and uses `ambient_hour` instead.
    pub ambient_override: bool,
    pub ambient_hour: f32,
    /// Pin one ambient look instead of following the sun.
    pub ambient_force: ambient::Force,
    /// Dusk's ramp position while pinned.
    pub ambient_fr: f32,

    /// Current color palette.
    pub palette: Box<dyn Palette>,
    /// Rolls the Toned palettes' pairings; bumped by every palette press.
    pub palette_seed: usize,
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
    /// The square armed for tap recording, purple on the pad. Pressing any
    /// other square in the tap surface commits the run.
    pub tap_at: Option<(i8, i8)>,

    /// Manual beat
    pub beat: Option<Beat>,

    /// Global brightness modifier
    pub brightness: f32,

    /// Optics set by hand on the control surface, over whatever the look asked
    /// for. Prism rotation is signed, still at 0.
    pub outcast_zoom: f32,
    pub hydro_zoom: f32,
    pub colorado_zoom: f32,
    pub prism_rot: f32,
    /// Index into [`RingPattern::ALL`].
    pub ring: usize,
    pub prism: Prism,

    /// Slow sine over both wash zooms.
    pub zoom_wave: bool,
    /// Pad guide, showing what each button does, rather than the visualiser.
    pub debug: bool,
    /// Most recently pressed grid coord, which picks the visualiser's pattern.
    pub x: i8,
    pub y: i8,

    /// Style browser, shown while the right arrow is held.
    pub browse: bool,
    /// Last whole beat Auto saw, and what its latest reroll picked.
    pub auto_beat: usize,
    pub auto_look: usize,
    /// Beat `0..64` the next Auto change fires on, drawn red on the grid.
    pub auto_next: usize,
    /// Changes fired so far, for pacing the palette swaps.
    pub auto_changes: usize,
}

/// One in / one out on both wash zooms, in seconds.
const ZOOM_WAVE: f32 = 6.0;

/// The brightness each button of the right hand column drops to.
const LEVELS: [f32; 8] = [0.0, 0.07, 0.125, 0.3, 0.4, 0.6, 0.8, 1.0];

/// Presses this close to a predicted beat snap `phi` to it (small, so
/// way-off accidental hits don't shift the clock).
const SYNC_MOE: f32 = 0.09;

/// Gaps at least this long end the run of taps counted for BPM.
const TAP_GAP_MAX: f32 = 3.0;

/// The tap surface gives up sooner: a recording square this stale un-arms
/// and its run is discarded.
const TAP_RUN: f32 = 1.5;

impl State {
    pub fn new() -> Self {
        // Simple gets both tone pages at once: warm on the top two rows,
        // cool on the two below them.
        let mut simple = warm_colors();
        simple.extend(cool_colors().into_iter().map(|mut b| {
            b.xy.1 -= 2;
            b
        }));
        Self {
            perform: perform(),
            banks: banks(),
            bank: 0,
            mode: Mode::Perform,
            simple,
            simple_dim: 0.6,
            simple_class: 0,
            simple_look: 0,
            reset_held: false,
            ambient_override: false,
            ambient_hour: 19.0,
            ambient_force: ambient::Force::Auto,
            ambient_fr: 0.5,

            palette: Box::new(Rainbow),
            palette_seed: 0,
            style: 1,
            base: Look::new(Energy::Off, Movement::Home, Pd(4, 1)),
            seed: 0,
            specials: vec![],

            t: 0.0,
            bpm: 120.0,
            bpm_taps: vec![],
            phi: 0.0,
            phi_mul: 1.0,
            tap_at: None,
            beat: None,

            brightness: 1.0,
            outcast_zoom: 0.0,
            hydro_zoom: 0.0,
            colorado_zoom: 0.0,
            prism_rot: 0.0,
            ring: 0,
            prism: Prism::Off,
            zoom_wave: false,
            debug: true,
            x: 0,
            y: 0,
            browse: false,
            auto_beat: 0,
            auto_look: 0,
            auto_next: 8,
            auto_changes: 0,
        }
    }

    /// What a button does: the active bank's colours over the fixed grid.
    pub fn binding(&self, xy: (i8, i8)) -> Option<&PadOp> {
        self.banks[self.bank]
            .palettes
            .iter()
            .chain(&self.perform)
            .find(|b| b.xy == xy)
            .map(|b| &b.op)
    }

    pub fn simple_binding(&self, xy: (i8, i8)) -> Option<&PadOp> {
        self.simple.iter().find(|b| b.xy == xy).map(|b| &b.op)
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
            texture: style.texture,
            beam: self.palette.beam_color(self),
            par: self.palette.par_color(self),
            colored: false,
            ring_pattern: RingPattern::Off,
            mask: Mask::All,
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

    /// A press on the tap surface: the first square pressed arms and records
    /// the run, any other square commits it.
    pub fn tap(&mut self, xy: (i8, i8)) {
        match self.tap_at {
            Some(at) if at != xy => self.commit_tap(),
            _ => {
                self.tap_at = Some(xy);
                match self.bpm_taps.last() {
                    Some(last) if self.t - last < TAP_RUN => {}
                    _ => self.bpm_taps.clear(),
                }
                self.bpm_taps.push(self.t);
            }
        }
    }

    /// Commit the recorded run: the mean interval is the bpm, and this press
    /// is the downbeat.
    fn commit_tap(&mut self) {
        if let Some(&last) = self.bpm_taps.last()
            && self.t - last < TAP_RUN
        {
            self.bpm_taps.push(self.t);
        }
        let taps = &self.bpm_taps;
        if taps.len() >= 2 {
            let dt = (taps[taps.len() - 1] - taps[0]) / (taps.len() - 1) as f32;
            if (0.24..=2.0).contains(&dt) {
                self.bpm = 60.0 / dt;
                info!("Committed bpm={:.2} from {} taps", self.bpm, taps.len());
            }
        }
        self.bpm_taps.clear();
        self.tap_at = None;
        self.phi = 0.0;
    }

    /// Whether a run of taps is being recorded.
    pub fn tapping(&self) -> bool {
        self.tap_at.is_some()
            || self.bpm_taps.last().is_some_and(|last| self.t - last < TAP_RUN)
    }

    /// Switch modes, landing somewhere sane rather than on whatever the last
    /// mode left running: Simple opens dark, Auto starts its measure on one
    /// of its own low looks.
    pub fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
        match mode {
            Mode::Simple => reroll(self, 0),
            Mode::Auto => {
                self.phi = 0.0;
                self.auto_beat = 0;
                let mut rng = ThreadRng::default();
                self.auto_look = rng.gen_range(0..LOW.len());
                self.base = LOW[self.auto_look];
                schedule(self, &mut rng);
            }
            _ => {}
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
    // Ambient keeps its own slow clock; phase there is decoration.
    let bpm = match s.mode {
        Mode::Ambient => ambient::BPM,
        _ => s.bpm,
    };
    s.phi = (s.phi + (bpm / 60.0) * s.phi_mul * dt) % 64.0;

    // An abandoned recording clears itself.
    if s.tap_at.is_some() && s.bpm_taps.last().is_none_or(|last| s.t - last >= TAP_RUN) {
        s.tap_at = None;
        s.bpm_taps.clear();
    }

    // Expire one-shots
    let (t, bpm) = (s.t, s.bpm);
    s.specials.retain(|a| match a.special.fire {
        Fire::OneShot { pd } => (t - a.t0) * (bpm / 60.0) < pd.fr(),
        _ => true,
    });

    // Auto: fire the booked change, then book the next.
    if let Mode::Auto = s.mode {
        let beat = s.phi as usize;
        if beat != s.auto_beat {
            s.auto_beat = beat;
            if beat == s.auto_next {
                let mut rng = ThreadRng::default();
                s.seed += 1;
                // Always another low look: little switches, never a jarring
                // jump in energy. Never the one already running.
                let mut i = rng.gen_range(0..LOW.len());
                if i == s.auto_look {
                    i = (i + 1) % LOW.len();
                }
                s.auto_look = i;
                s.base = LOW[i];
                s.auto_changes += 1;
                if s.auto_changes % 3 == 0 {
                    s.palette = random_palette(&mut rng);
                }
                schedule(s, &mut rng);
            }
        }
    }
}

/// Book the next Auto change off the current beat.
fn schedule(s: &mut State, rng: &mut ThreadRng) {
    s.auto_next = (s.auto_beat + AUTO_STEPS[rng.gen_range(0..AUTO_STEPS.len())]) % 64;
}

/// A class press in Simple: a random look out of the class's pool, never
/// repeating the one already running.
fn reroll(s: &mut State, class: usize) {
    let c = &SIMPLE_CLASSES[class];
    let mut rng = ThreadRng::default();
    let mut m = rng.gen_range(0..c.looks.len());
    if class == s.simple_class && m == s.simple_look && c.looks.len() > 1 {
        m = (m + 1) % c.looks.len();
    }
    s.simple_class = class;
    s.simple_look = m;
    s.seed += 1;
    s.base = c.looks[m];
}

///////////////////////// SPICE /////////////////////////

/// How the six pars share a look's energy: same period, dealt differently.
#[derive(Clone, Copy, PartialEq)]
enum ParSpice {
    /// The flat field.
    Together,
    /// The pattern walks across the row.
    Wave,
    /// Evens against odds.
    Alternate,
    /// The middle leads, the ends trail.
    Ends,
    /// A random few carry it; the rest stay dark.
    Sparse,
}

/// Fine detail rerolled with the seed, on top of whatever look a button
/// names: a touch of gobo in the haze, a hair of zoom, the spin hurried or
/// turned round, the pars dealt a new arrangement. Seasoning only — nothing
/// here changes the look's energy, so a button still means what it meant.
struct Spice {
    /// Onto an open gate under a calm look; usually stays open itself.
    gobo: Gobo,
    gobo_rot: u8,
    /// Extra width for looks holding the thin-beam baseline.
    zoom: f32,
    /// Over the look's own prism rotation.
    prism_mul: f32,
    pars: ParSpice,
    /// Which pars a Sparse deal lights.
    par_gate: [f32; 6],
    par_zoom: f32,
}

fn spice(seed: usize) -> Spice {
    use rand::prelude::*;
    let mut rng = StdRng::seed_from_u64(seed as u64);
    let mut par_gate = [0.0; 6];
    for g in &mut par_gate {
        *g = rng.gen_bool(0.4) as u8 as f32;
    }
    if par_gate.iter().all(|g| *g == 0.0) {
        (par_gate[2], par_gate[3]) = (1.0, 1.0);
    }
    Spice {
        // Dots or the flower when it lands: the two that still read in the
        // beam haze on an upward beam.
        gobo: [Gobo::Open, Gobo::Open, Gobo::Open, Gobo::Dots, Gobo::Flower]
            [rng.gen_range(0..5)],
        // 17-40 deg/s, either way round.
        gobo_rot: [187, 188, 199, 200][rng.gen_range(0..4)],
        zoom: [0.0, 0.0, 0.0, 0.1, 0.2][rng.gen_range(0..5)],
        prism_mul: [1.0, 1.0, 0.6, 1.6, -1.0][rng.gen_range(0..5)],
        pars: [
            ParSpice::Together,
            ParSpice::Together,
            ParSpice::Wave,
            ParSpice::Wave,
            ParSpice::Alternate,
            ParSpice::Ends,
            ParSpice::Sparse,
            ParSpice::Sparse,
        ][rng.gen_range(0..8)],
        par_gate,
        par_zoom: [0.0, 0.0, 0.0, 0.5, 1.0][rng.gen_range(0..5)],
    }
}

///////////////////////// LIGHTS /////////////////////////

pub fn render_lights(
    s: Res<State>,
    mut l: ResMut<Lights>,
    movers: Res<Movers>,
    patch: Res<Patch>,
    blackout: Res<Blackout>,
    home: Res<Home>,
    trim: Res<Trim>,
    time: Res<Time>,
    mut wheels: ResMut<Wheels>,
    mut universe: ResMut<Universe>,
) {
    let s: &State = &s;
    if let Mode::Ambient = s.mode {
        ambient::render(
            s,
            &mut l,
            &movers,
            &patch,
            &blackout,
            &home,
            &trim,
            time.delta_secs(),
            &mut wheels,
            &mut universe,
        );
        return;
    }

    let look = s.look();
    let pd = look.pd();

    // Simple runs everything under its own ceiling.
    let master = s.brightness
        * match s.mode {
            Mode::Simple => s.simple_dim * SIMPLE_CLASSES[s.simple_class].level,
            _ => 1.0,
        };

    // Left column flashes the pars, right column the movers; only Perform has
    // the buttons, so a beat left behind never dims the other modes.
    let (beat0, beat1) = match s.mode {
        Mode::Perform => (s.beat_fr0().unwrap_or(1.0), s.beat_fr1().unwrap_or(1.0)),
        _ => (1.0, 1.0),
    };
    let (outcasts_mask, hydros_mask, pars_mask) =
        (look.mask.outcasts(), look.mask.hydros(), look.mask.pars());

    // Spice stays off anything hard: a strobe is exactly its button.
    let spice = spice(s.seed);
    let calm = !matches!(
        look.energy,
        Energy::Strobe { .. } | Energy::Chase { .. } | Energy::Alternate { .. }
    );
    let widen = match calm && look.texture.zoom == 0.0 && look.texture.breathe == 0.0 {
        true => spice.zoom,
        false => 0.0,
    };

    l.reset();

    // Movers: outcasts are 0..2, hydros 2..4
    for (i, o) in l.outcasts.iter_mut().enumerate() {
        let fr = i as f32 / 4.0;
        let rest = &home.movers[i];
        let (pitch, yaw) = look.angles(s, pd, i, fr, &movers.at[i], rest);
        o.pitch = pitch;
        o.yaw = yaw;
        o.zoom = (look.zoom_at(s, pd, fr, rest) + widen).max(s.outcast_zoom);
        // The look's ring animation, until one is picked by hand; it runs
        // busy under a strobe and crawls under anything calm.
        o.ring_pattern = match s.ring {
            0 => look.ring_pattern,
            _ => RingPattern::ALL[s.ring],
        };
        o.ring_pattern_speed = match calm {
            true => 0.2,
            false => 0.85,
        };

        let beam = match look.colored {
            true => look.beam,
            false => s.palette.beam_color_at(s, i),
        };
        let env = look.energy.env(s, fr) * look.movement.mask(s, pd, i, fr) * outcasts_mask;
        let col = beam.rgbw * env * master * beat1 * trim.global;
        match look.texture.ring {
            RingMode::Both => {
                o.ring = col * trim.ring;
                o.center = col * trim.beam;
            }
            RingMode::Ring => o.ring = col * trim.ring,
            RingMode::Center => o.center = col * trim.beam,
        }
    }

    for (i, h) in l.hydros.iter_mut().enumerate() {
        let (i, fr) = (i + 2, (i + 2) as f32 / 4.0);
        let rest = &home.movers[i];
        let (pitch, yaw) = look.angles(s, pd, i, fr, &movers.at[i], rest);
        h.pitch = pitch;
        h.yaw = yaw;

        // No mixing: the wheels carry the colour and the brightest component
        // of what the palette asked for is all the dimmer can say about it.
        let beam = match look.colored {
            true => look.beam,
            false => s.palette.beam_color_at(s, i),
        };
        let env = look.energy.env(s, fr) * look.movement.mask(s, pd, i, fr) * hydros_mask;
        let Swatch { rgbw: Rgbw(r, g, b, w), one, two, spot } = beam;
        let lit = r.max(g).max(b).max(w) * spot as u8 as f32;
        h.alpha = lit * env * master * beat1 * trim.global * trim.hydro;
        h.color = one;
        h.color2 = two;
        // Hold the beam down while a wheel crosses to a slot it is not already
        // beside, since the fixture will not blank itself for it.
        h.color_mask = true;
        // An open gate under a calm look takes the spice's texture, the one
        // detail the grid cannot spare buttons for. Under a wheel-rocking
        // cycle it rocks the gobo wheel instead, a slot out and back at the
        // same rate, so the strobe cuts into the texture too.
        match (look.texture.gobo, s.palette.wheel_strobing(), calm) {
            (Gobo::Open, true, _) => {
                h.gobo = match s.pd(Pd(1, 2)).ramp(1.0) < 0.5 {
                    true => Gobo::Open,
                    false => Gobo::Dots,
                };
                h.gobo_rot = 0;
            }
            (Gobo::Open, _, true) => {
                h.gobo = spice.gobo;
                h.gobo_rot = spice.gobo_rot;
            }
            (gobo, ..) => {
                h.gobo = gobo;
                h.gobo_rot = look.texture.gobo_rot;
            }
        }
        h.gobo_shake = look.texture.gobo_shake;
        // The look's optics, with the control surface on top of them: its
        // buttons force a prism, its faders only ever widen or spin further.
        h.prism = match s.prism {
            Prism::Off => look.texture.prism,
            prism => prism,
        };
        // A mover hung the other way round has to spin its prism the other way
        // for the pair to read as one gesture.
        h.prism_rot = (look.texture.prism_rot * spice.prism_mul + s.prism_rot) * rest.sign().1;
        h.focus = look.texture.focus;
        h.zoom = (look.zoom_at(s, pd, fr, rest) + widen).max(s.hydro_zoom);
        h.frost = look.texture.frost;
    }

    for (i, p) in l.pars.iter_mut().enumerate() {
        let fr = i as f32 / 6.0;
        let par = match look.colored {
            true => look.par,
            false => s.palette.par_color_at(s, i),
        };
        // The spice's arrangement: the look's own energy and period, dealt
        // across the six out of step, in halves, or onto a sparse few.
        let (offset, gain) = match spice.pars {
            ParSpice::Together => (0.0, 1.0),
            ParSpice::Wave => (fr * 0.75, 1.0),
            ParSpice::Alternate => ((i % 2) as f32 * 0.5, 1.0),
            ParSpice::Ends => ((i as f32 - 2.5).abs() * 0.2, 1.0),
            ParSpice::Sparse => (fr * 0.5, spice.par_gate[i]),
        };
        let env = match look.energy {
            // On has no beat to deal, so the arrangement rides a slow swell.
            Energy::On if spice.pars != ParSpice::Together => {
                gain * s.pd(Pd(8, 1)).phase(1.0, offset * 2.0).fsin(1.0).lerp(0.15..1.0)
            }
            e => gain * e.env_at(s, fr, offset),
        };
        p.color = par * (env * pars_mask) * master * beat0 * trim.global * trim.colorado;
        p.zoom = spice.par_zoom.max(s.colorado_zoom);
    }

    if s.zoom_wave {
        let z = (s.t / ZOOM_WAVE).fsin(1.0);
        for o in &mut l.outcasts {
            o.zoom = z;
        }
        for p in &mut l.pars {
            p.zoom = z;
        }
    }

    wheels.mask(&mut l.hydros, time.delta_secs());
    l.encode(&patch, &blackout, &home, &movers, &mut universe);
}

///////////////////////// PAD INPUT /////////////////////////

pub fn on_pad(mut s: ResMut<State>, mut pad: ResMut<Pad>) {
    use lib::midi::device::launchpad_x::types::*;
    use lib::midi::device::launchpad_x::{Input, Output};

    let s: &mut State = &mut *s;
    let pad: &mut Pad = &mut *pad;

    for input in pad.recv() {
        match input {
            // Colour banks; in Auto the top-left key is the phase reset.
            // Warm, cool, then the cycles on both lower arrows.
            Input::Up(true) => match s.mode {
                Mode::Auto => s.phi = 0.0,
                _ => s.bank = 0,
            },
            Input::Down(true) => s.bank = 1,
            Input::Left(true) => s.bank = 2,
            Input::Right(true) => s.bank = 2,
            // Cycle Perform -> Simple -> Auto -> Ambient
            Input::Capture(true) => s.set_mode(s.mode.next()),
            Input::Session(true) => s.zoom_wave = !s.zoom_wave,
            // Normal / double / half time, which the next grid press drops
            Input::Note(true) => {
                s.phi_mul = match s.phi_mul {
                    m if m > 1.0 => 0.5,
                    m if m < 1.0 => 1.0,
                    _ => 2.0,
                }
            }
            // Toggle the pad guide
            Input::Custom(true) => {
                s.debug = !s.debug;
                pad.send(Output::Clear);
            }
            // Pop hold specials
            Input::Release(i) => {
                let Coord(x, y) = i.into();
                s.specials.retain(|a| !(matches!(a.special.fire, Fire::Hold) && a.xy == (x, y)));
                if (6..=7).contains(&x) && (2..=3).contains(&y) {
                    s.reset_held = false;
                }
            }
            _ => {}
        }

        // Right column: brightness, except Auto jumps to that measure row.
        if let Some(level) = match input {
            Input::Record(true) => Some(0),
            Input::Solo(true) => Some(1),
            Input::Mute(true) => Some(2),
            Input::Stop(true) => Some(3),
            Input::B(true) => Some(4),
            Input::A(true) => Some(5),
            Input::Pan(true) => Some(6),
            Input::Volume(true) => Some(7),
            _ => None,
        } {
            match s.mode {
                // Place the booked change on that row: at its start, or on a
                // second press its second 4-beat bar.
                Mode::Auto => {
                    let start = (7 - level) * 8;
                    s.auto_next = match s.auto_next == start {
                        true => start + 4,
                        false => start,
                    };
                }
                Mode::Ambient => {}
                _ => s.brightness = LEVELS[level],
            }
        }

        let Some((x, y)) = input.xy() else { continue };
        if x > 7 || y > 7 {
            continue;
        }
        debug!("Pad({x}, {y})");
        (s.x, s.y) = (x, y);
        s.phi_mul = 1.0;

        // Style browser, opened from the control surface
        if s.browse {
            let i = (y * 8 + x) as usize;
            if i < STYLES.len() {
                s.style = i;
                info!("style: {}", STYLES[i].name);
            }
            continue;
        }

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
                        // Fresh spice with every press, repeat or not.
                        s.seed += 1;
                        s.resync();
                    }
                    PadOp::Palette(palette) => {
                        s.palette = palette;
                        // Reroll the tone's pairing; pressing again browses.
                        s.palette_seed += 1;
                    }
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
            Mode::Simple => {
                if let Some(PadOp::Palette(palette)) = s.simple_binding((x, y)).cloned() {
                    s.palette = palette;
                    s.palette_seed += 1;
                } else if y < 2 {
                    reroll(s, (x / 2) as usize);
                    s.resync();
                } else if (2..=3).contains(&y) && (0..=3).contains(&x) {
                    s.tap((x, y));
                } else if (2..=3).contains(&y) && (6..=7).contains(&x) {
                    s.reset_held = true;
                    s.phi = 0.0;
                }
            }
            // The whole grid is the tap
            Mode::Auto => s.tap((x, y)),
            Mode::Ambient => {}
        }
    }
}

///////////////////////// PAD OUTPUT /////////////////////////

pub fn render_pad(s: Res<State>, mut pad: ResMut<Pad>, mut was: Local<Option<Mode>>) {
    use lib::midi::device::launchpad_x::Output;
    use lib::midi::device::launchpad_x::types::*;

    let s: &State = &s;

    // A mode leaves nothing behind for the next one's layout.
    if *was != Some(s.mode) {
        *was = Some(s.mode);
        pad.send(Output::Clear);
    }

    let mut batch: Vec<(Pos, Color)> = vec![];
    let rgb = |Rgb(r, g, b): Rgb| Color::Rgb(r, g, b);
    let mut set = |x, y, color: Rgbw| batch.push((Coord(x, y).into(), rgb(color.into())));

    if s.browse {
        // Style browser
        for (i, style) in STYLES.iter().enumerate() {
            let fr = if i == s.style { s.pd(Pd(1, 1)).ramp(1.0).inv().lerp(0.4..1.0) } else { 0.15 };
            set((i % 8) as i8, (i / 8) as i8, style.color * fr);
        }
    } else {
        match s.mode {
            Mode::Perform if !s.debug => visualise(s, &mut set),
            Mode::Perform => {
                let beam = s.palette.beam_color(s).rgbw;
                for PadBinding { xy: (x, y), op } in
                    s.perform.iter().chain(&s.banks[s.bank].palettes)
                {
                    match op {
                        PadOp::Energy(e) => {
                            let on = Some(*e) == s.base.energy;
                            let fr = if on { e.env(s, 0.0).max(0.3) } else { 0.1 };
                            set(*x, *y, Rgbw::WHITE * fr);
                        }
                        // Each button beats at the rate it would drive the rig
                        // at, in the colour it would use; one that leaves
                        // families dark sits dimmer, so the low end reads low.
                        PadOp::Look(look) => {
                            let col = look.color.map_or(beam, |c| c.rgbw);
                            let env = look.energy.map_or(0.0, |e| e.env(s, 0.0));
                            let breadth = match look.mask {
                                Some(Mask::Hydros) => 0.4,
                                Some(Mask::Movers | Mask::Pars) => 0.7,
                                _ => 1.0,
                            };
                            set(*x, *y, col * (env.max(0.15) * breadth));
                        }
                        PadOp::Palette(palette) => set(*x, *y, palette.pad_color(s)),
                        PadOp::Special(sp) => {
                            let active = s.specials.iter().any(|a| a.xy == (*x, *y));
                            set(*x, *y, sp.color * if active { 1.0 } else { 0.2 });
                        }
                        PadOp::Beat { .. } => set(*x, *y, Rgbw::VIOLET),
                        PadOp::Func { color, .. } => set(*x, *y, *color),
                    }
                }

                // Beat buttons: upwards propagating wave at BPM
                for i in 0..=4 {
                    let col = Rgbw::VIOLET * (s.phi - i as f32 * 0.2).fsin(2.0);
                    set(0, i, col);
                    set(7, i, col);
                }
            }
            Mode::Simple => simple_pad(s, &mut set),
            Mode::Auto => auto_pad(s, &mut set),
            // Dark: the show is the sun's.
            Mode::Ambient => {}
        }
    }

    if let Mode::Perform = s.mode {
        // Top row: the four arrows are the colour banks
        let lit = |on: bool, col: Rgbw| if on { col } else { Rgbw::BLACK };
        for (i, bank) in s.banks.iter().enumerate() {
            set(i as i8, 8, bank.color * if i == s.bank { 1.0 } else { 0.15 });
        }
        set(4, 8, lit(s.zoom_wave, Rgbw::VIOLET));
        set(
            5,
            8,
            match s.phi_mul {
                m if m > 1.0 => Rgbw::VIOLET,
                m if m < 1.0 => Rgbw::CYAN,
                _ => Rgbw::WHITE * 0.15,
            },
        );
        set(6, 8, lit(s.debug, Rgbw::WHITE));
        set(7, 8, mode_color(s.mode));

        // Right column: brightness, lit up to the level in use
        for (y, level) in LEVELS.iter().enumerate() {
            set(8, y as i8, lit(*level <= s.brightness, Rgbw::WHITE * level.max(0.15)));
        }

        // Beat indicator
        set(
            8,
            8,
            match s.pd(Pd(1, 1)).bsquare(1.0, 0.1) {
                true => match s.pd(Pd(4, 1)).bsquare(1.0, 0.2) {
                    // White on the first beat of each bar
                    true => Rgbw::WHITE,
                    // The palette's colour on every other beat
                    false => s.palette.beam_color(s).rgbw,
                },
                false => Rgbw::BLACK,
            },
        );
    } else if s.mode != Mode::Ambient {
        // Just the mode key, so the way back out is findable.
        set(7, 8, mode_color(s.mode));
    }

    pad.send(Output::Batch(batch));
}

fn mode_color(mode: Mode) -> Rgbw {
    match mode {
        Mode::Perform => Rgbw::WHITE,
        Mode::Simple => Rgbw::CYAN,
        Mode::Auto => Rgbw::MAGENTA,
        Mode::Ambient => Rgbw::BLACK,
    }
}

/// Simple's guide: colours up top, the energy classes across the bottom, the
/// tap bar and phase reset between them.
fn simple_pad(s: &State, set: &mut impl FnMut(i8, i8, Rgbw)) {
    for PadBinding { xy: (x, y), op } in &s.simple {
        if let PadOp::Palette(palette) = op {
            set(*x, *y, palette.pad_color(s));
        }
    }

    // Each class flashes at the rate it stands for, whatever is running:
    // off sits dim, then slow pulse, quarter notes, strobe. The active one
    // is full on.
    for (i, class) in SIMPLE_CLASSES.iter().enumerate() {
        let env = match i {
            0 => 0.35,
            1 => s.pd(Pd(4, 1)).fsin(1.0).lerp(0.25..1.0),
            2 => s.pd(Pd(1, 1)).inv().in_quad().lerp(0.2..1.0),
            _ => s.pd(Pd(1, 4)).square(1.0, 0.5).lerp(0.3..1.0),
        };
        let fr = env
            * match i == s.simple_class {
                true => 1.0,
                false => 0.3,
            };
        for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            set(i as i8 * 2 + dx, dy, class.color * fr);
        }
    }

    tap_bar(s, set, 0, 2, 6, 2);
}

/// The tap bar, 4x2 at `(x0, y0)`: dim white with a solid white column
/// walking the four beats of the bar; the square being recorded on holds dim
/// violet and flashes bright with each tap. The phase reset, `rw` wide at
/// `rx`, sits dim so it is findable and lights full while held.
fn tap_bar(s: &State, set: &mut impl FnMut(i8, i8, Rgbw), x0: i8, y0: i8, rx: i8, rw: i8) {
    let beat = s.phi as usize % 4;
    for i in 0..4i8 {
        for y in [y0, y0 + 1] {
            let x = x0 + i;
            let col = match s.tap_at == Some((x, y)) {
                true => tap_glow(s),
                false => match i as usize == beat {
                    true => Rgbw::WHITE,
                    false => Rgbw::WHITE * 0.08,
                },
            };
            set(x, y, col);
        }
    }

    let held = match s.reset_held {
        true => Rgbw::WHITE,
        false => Rgbw::WHITE * 0.1,
    };
    for dx in 0..rw {
        set(rx + dx, y0, held);
        set(rx + dx, y0 + 1, held);
    }
}

/// The square being recorded on: dim violet, flashing bright with each tap.
fn tap_glow(s: &State) -> Rgbw {
    let flash = s.bpm_taps.last().map_or(0.0, |last| (1.0 - (s.t - last) / 0.15).max(0.0));
    Rgbw::VIOLET * flash.lerp(0.35..1.0)
}

/// Auto's guide: the 64-beat measure across the whole pad, top left to
/// bottom right, a crosshair on the current beat and the booked change in
/// red. The grid is the tap surface, the right column places the change on
/// its row, and the top-left key resets the phase.
fn auto_pad(s: &State, set: &mut impl FnMut(i8, i8, Rgbw)) {
    let beat = s.phi as usize % 64;
    for b in 0..64usize {
        let (x, y) = ((b % 8) as i8, 7 - (b / 8) as i8);
        let col = match () {
            _ if s.tap_at == Some((x, y)) => tap_glow(s),
            // The booked change, beating red wherever the cursor is.
            _ if b == s.auto_next => Rgbw::RED * s.phi.fract().inv().lerp(0.4..1.0),
            _ if b == beat => Rgbw::WHITE,
            // Crosshair: the current row and the current dot's column.
            _ if b / 8 == beat / 8 => Rgbw::WHITE * 0.25,
            _ if b % 8 == beat % 8 => Rgbw::WHITE * 0.25,
            _ => Rgbw::WHITE * 0.06,
        };
        set(x, y, col);
    }

    // Right column: place the change on that row; the row holding it reads
    // red, the playing row white.
    for y in 0..8i8 {
        let row = (7 - y) as usize;
        let col = match () {
            _ if s.auto_next / 8 == row => Rgbw::RED * 0.5,
            _ if beat / 8 == row => Rgbw::WHITE * 0.25,
            _ => Rgbw::WHITE * 0.08,
        };
        set(8, y, col);
    }

    // Top left: phase reset.
    set(0, 8, Rgbw::WHITE * 0.3);
}

///////////////////////// PAD VISUALISER /////////////////////////

/// The pad as decoration rather than a guide: whatever the rig is doing,
/// drawn across the grid.
fn visualise(s: &State, set: &mut impl FnMut(i8, i8, Rgbw)) {
    let look = s.look();
    let col = look.beam.rgbw;

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
        (Energy::On, Movement::Whirl) => spiral(set, s.t, col, 8.0),
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

/// Ring patterns reachable from the focus row, then the control row below it.
const RING_ROWS: [usize; 2] = [0, 8];

pub fn on_ctrl(mut s: ResMut<State>, mut ctrl: ResMut<Ctrl>, mut trim: ResMut<Trim>) {
    use lib::midi::device::launch_control_xl::Input;

    for input in ctrl.recv() {
        debug!("ctrl: {input:?}");

        match input {
            Input::Slider(0, fr) => s.brightness = fr,
            Input::Slider(1, fr) => s.outcast_zoom = fr,
            Input::Slider(2, fr) => s.hydro_zoom = fr,
            Input::Slider(3, fr) => s.colorado_zoom = fr,
            Input::Slider(4, fr) => s.prism_rot = fr.lerp(-1.0..1.0),
            // The outcast's ring and beam move together from one fader.
            Input::Slider(5, fr) => (trim.ring, trim.beam) = (fr, fr),
            Input::Slider(6, fr) => trim.hydro = fr,
            Input::Slider(7, fr) => trim.colorado = fr,

            Input::Focus(i, true) => s.ring = ring_at(RING_ROWS[0], i),
            Input::Control(i, true) => s.ring = ring_at(RING_ROWS[1], i),

            Input::Mute(true) => s.prism = Prism::Off,
            Input::Solo(true) => s.prism = Prism::Linear,
            Input::Record(true) => s.prism = Prism::Circular,

            // The pad's arrows are the colour banks, so the style browser
            // opens from here.
            Input::Device(true) => s.browse = !s.browse,
            _ => {}
        }
    }
}

/// The pattern button `i` of a row selects, or the row's first where the row
/// runs off the end of the chart.
fn ring_at(row: usize, i: u8) -> usize {
    let at = row + i as usize;
    match at < RingPattern::ALL.len() {
        true => at,
        false => row,
    }
}

///////////////////////// CTRL OUTPUT /////////////////////////

/// Light the buttons that select something, and brighten the one in use.
/// Resent whenever the selection or the connection changes, since the device
/// clears its LEDs when it comes up.
pub fn render_ctrl(
    s: Res<State>,
    mut ctrl: ResMut<Ctrl>,
    mut last: Local<Option<(usize, Prism, bool, bool)>>,
) {
    use lib::midi::device::launch_control_xl::types::*;
    use lib::midi::device::launch_control_xl::{Led, Output};

    let now = (s.ring, s.prism, s.browse, ctrl.connected());
    if *last == Some(now) {
        return;
    }
    *last = Some(now);

    let lamp = |available: bool, active: bool| match (available, active) {
        (_, true) => (Color::Green, Brightness::High),
        (true, false) => (Color::Red, Brightness::Low),
        _ => (Color::Red, Brightness::Off),
    };

    let mut batch = vec![];
    for (row, led) in RING_ROWS.iter().zip([Led::Focus as fn(u8) -> Led, Led::Control]) {
        for i in 0..8u8 {
            let at = row + i as usize;
            let (color, brightness) = lamp(at < RingPattern::ALL.len(), at == s.ring);
            batch.push((led(i), color, brightness));
        }
    }
    for (led, prism) in [(Led::Mute, Prism::Off), (Led::Solo, Prism::Linear), (Led::Record, Prism::Circular)] {
        let (color, brightness) = lamp(true, prism == s.prism);
        batch.push((led, color, brightness));
    }
    let (color, brightness) = lamp(true, s.browse);
    batch.push((Led::Device, color, brightness));

    ctrl.send(Output::Batch(batch));
}
