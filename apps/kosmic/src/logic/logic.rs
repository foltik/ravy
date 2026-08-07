use itertools::Itertools;
use lib::prelude::*;
use rand::Rng;
use rand::rngs::ThreadRng;

use super::Swatch;
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

    // --- y=1: full on ---
    (1, 1) => look!(Look::new(Energy::On, Movement::Out, Pd(4, 1))),
    (2, 1) => look!(Look::new(Energy::On, Movement::Out, Pd(4, 1))),
    (3, 1) => look!(Look::new(Energy::On, Movement::WaveY, Pd(4, 1))),
    (4, 1) => look!(Look::new(Energy::On, Movement::SnapX, Pd(4, 1))),
    (5, 1) => look!(Look::new(Energy::On, Movement::Whirl, Pd(4, 1))),
    (6, 1) => look!(Look::new(Energy::On, Movement::Twisting, Pd(4, 1))),

    // --- y=2..4: flashing to the beat, slowest row at the bottom ---
    (1, 2) => look!(Look::new(Energy::Swell { pd: Pd(1, 1) }, Movement::WaveY, Pd(4, 1))),
    (2, 2) => look!(Look::new(Energy::Beat { pd: Pd(4, 1) }, Movement::RaisingBeams, Pd(4, 1))),
    (3, 2) => look!(Look::new(Energy::Beat { pd: Pd(4, 1) }, Movement::WaveY, Pd(4, 1))),
    (4, 2) => look!(Look::new(Energy::Beat { pd: Pd(4, 1) }, Movement::UpDownWave, Pd(4, 1))),
    (5, 2) => look!(Look::new(Energy::Beat { pd: Pd(4, 1) }, Movement::Whirl, Pd(4, 1))),
    (6, 2) => look!(Look::new(Energy::Beat { pd: Pd(4, 1) }, Movement::Twisting, Pd(4, 1))),

    (1, 3) => look!(Look::new(Energy::Swell { pd: Pd(1, 2) }, Movement::Square, Pd(4, 1))),
    (2, 3) => look!(Look::new(Energy::Beat { pd: Pd(2, 1) }, Movement::RaisingBeams, Pd(2, 1))),
    (3, 3) => look!(Look::new(Energy::Beat { pd: Pd(2, 1) }, Movement::WaveY, Pd(2, 1))),
    (4, 3) => look!(Look::new(Energy::Beat { pd: Pd(2, 1) }, Movement::UpDownWave, Pd(2, 1))),
    (5, 3) => look!(Look::new(Energy::Beat { pd: Pd(2, 1) }, Movement::Whirl, Pd(2, 1))),
    (6, 3) => look!(Look::new(Energy::Beat { pd: Pd(2, 1) }, Movement::Twisting, Pd(2, 1))),

    (1, 4) => look!(Look::new(Energy::Beat { pd: Pd(1, 1) }, Movement::Square, Pd(1, 1))),
    (2, 4) => look!(Look::new(Energy::Beat { pd: Pd(1, 1) }, Movement::RaisingBeams, Pd(1, 1))),
    (3, 4) => look!(Look::new(Energy::Beat { pd: Pd(1, 1) }, Movement::WaveY, Pd(1, 1))),
    (4, 4) => look!(Look::new(Energy::Beat { pd: Pd(1, 1) }, Movement::UpDownWave, Pd(1, 1))),
    (5, 4) => look!(Look::new(Energy::Beat { pd: Pd(1, 1) }, Movement::Whirl, Pd(1, 1))),
    (6, 4) => look!(Look::new(Energy::Beat { pd: Pd(1, 1) }, Movement::Twisting, Pd(1, 1))),

    // --- y=5: strobes and chases ---
    (0, 5) => look!(Look::new(Energy::Strobe { pd: Pd(1, 8), duty: 1.0 }, Movement::Square, Pd(2, 1)).masked(Mask::Pars)),
    (1, 5) => look!(Look::new(Energy::Alternate { pd: Pd(1, 4) }, Movement::CrossSway, Pd(1, 4)).masked(Mask::Movers)),
    (2, 5) => look!(Look::new(Energy::Strobe { pd: Pd(1, 4), duty: 1.0 }, Movement::Square, Pd(2, 1))),
    (3, 5) => look!(Look::new(Energy::Strobe { pd: Pd(1, 8), duty: 1.0 }, Movement::Square, Pd(2, 1))),
    (4, 5) => look!(Look::new(Energy::Chase { pd: Pd(1, 2) }, Movement::Twisting, Pd(1, 2))),
    (5, 5) => look!(Look::new(Energy::Chase { pd: Pd(1, 1) }, Movement::Twisting, Pd(1, 2)).colored(Swatch::WHITE)),
    (6, 5) => look!(Look::new(Energy::Chase { pd: Pd(1, 2) }, Movement::Twisting, Pd(1, 2)).colored(Swatch::WHITE)),
    (7, 5) => look!(Look::new(Energy::Chase { pd: Pd(1, 4) }, Movement::Twisting, Pd(1, 2)).colored(Swatch::WHITE)),

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

// A Cycle runs in half a beat, so the Hydros can only follow one where the two
// slots are neighbours on the same wheel; anything further would not arrive,
// and the wheels stay open instead.
bind! {

perform_colors:
    (0, 6) => palette!(Solid(Swatch::WHITE)),
    (1, 6) => palette!(Solid(Swatch::ORANGE)),
    (2, 6) => palette!(Solid(Swatch::YELLOW)),
    (3, 6) => palette!(Solid(Swatch::LIME)),
    (4, 6) => palette!(Solid(Swatch::BLUE)),
    (5, 6) => palette!(Solid(Swatch::MAGENTA)),
    (6, 6) => palette!(Cycle([Swatch::LIME.open(), Swatch::WHITE])),
    (7, 6) => palette!(Cycle([Swatch::RED.open(), Swatch::ORANGE.open(), Swatch::YELLOW.open(), Swatch::LIME.open(), Swatch::BLUE.open(), Swatch::VIOLET.open()])),

    (1, 7) => palette!(Split(Swatch::WHITE, Rgbw::RED)),
    (2, 7) => palette!(Solid(Swatch::PEA)),
    (3, 7) => palette!(Split(Swatch::WHITE, Rgbw::LIME)),
    (4, 7) => palette!(Split(Swatch::WHITE, Rgbw::BLUE)),
    (5, 7) => palette!(Split(Swatch::BLUE, Rgbw::VIOLET)),
    (6, 7) => palette!(Cycle([Swatch::BLUE.open(), Swatch::WHITE])),
}

bind! {

solid_colors:
    (0, 6) => palette!(Solid(Swatch::RGBW)),
    (1, 6) => palette!(Solid(Swatch::RED)),
    (2, 6) => palette!(Solid(Swatch::HOUSE)),
    (3, 6) => palette!(Solid(Swatch::MINT)),
    (4, 6) => palette!(Solid(Swatch::CYAN)),
    (5, 6) => palette!(Solid(Swatch::VIOLET)),
    (6, 6) => palette!(Solid(Swatch::PINK)),
    (7, 6) => palette!(Rainbow),

    (1, 7) => palette!(Solid(Swatch::WHITE)),
    (2, 7) => palette!(Solid(Swatch::ORANGE)),
    (3, 7) => palette!(Solid(Swatch::YELLOW)),
    (4, 7) => palette!(Solid(Swatch::LIME)),
    (5, 7) => palette!(Solid(Swatch::BLUE)),
    (6, 7) => palette!(Solid(Swatch::MAGENTA)),
}

// The movers take the first colour, the pars the second.
bind! {

split_colors:
    (0, 6) => palette!(Split(Swatch::WHITE, Rgbw::RED)),
    (1, 6) => palette!(Split(Swatch::WHITE, Rgbw::LIME)),
    (2, 6) => palette!(Split(Swatch::WHITE, Rgbw::BLUE)),
    (3, 6) => palette!(Split(Swatch::RED, Rgbw::WHITE)),
    (4, 6) => palette!(Split(Swatch::LIME, Rgbw::WHITE)),
    (5, 6) => palette!(Split(Swatch::BLUE, Rgbw::WHITE)),
    (6, 6) => palette!(Split(Swatch::BLUE, Rgbw::VIOLET)),
    (7, 6) => palette!(Split(Swatch::RED, Rgbw::BLUE)),

    (1, 7) => palette!(Split(Swatch::BLUE, Rgbw::LIME)),
    (2, 7) => palette!(Split(Swatch::LIME, Rgbw::BLUE)),
    (3, 7) => palette!(Split(Swatch::CYAN, Rgbw::MAGENTA)),
    (4, 7) => palette!(Split(Swatch::MAGENTA, Rgbw::CYAN)),
    (5, 7) => palette!(Split(Swatch::ORANGE, Rgbw::CYAN)),
    (6, 7) => palette!(Split(Swatch::PINK, Rgbw::CYAN)),
}

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
    /// Full control: bindings, browser, specials.
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
    /// Last whole beat Auto rerolled on, and what it picked.
    pub auto_beat: usize,
    pub auto_look: usize,
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

impl State {
    pub fn new() -> Self {
        Self {
            perform: perform(),
            banks: banks(),
            bank: 0,
            mode: Mode::Perform,

            palette: Box::new(Rainbow),
            style: 1,
            base: Look::new(Energy::Off, Movement::Home, Pd(4, 1)),
            seed: 0,
            specials: vec![],

            t: 0.0,
            bpm: 120.0,
            bpm_taps: vec![],
            phi: 0.0,
            phi_mul: 1.0,
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
    let look = s.look();
    let pd = look.pd();

    // Left column flashes the pars, right column the movers.
    let beat0 = s.beat_fr0().unwrap_or(1.0);
    let beat1 = s.beat_fr1().unwrap_or(1.0);
    let (movers_mask, pars_mask) = (look.mask.movers(), look.mask.pars());

    l.reset();

    // Movers: outcasts are 0..2, hydros 2..4
    for (i, o) in l.outcasts.iter_mut().enumerate() {
        let fr = i as f32 / 4.0;
        let rest = &home.movers[i];
        let (pitch, yaw) = look.angles(s, pd, i, fr, &movers.at[i], rest);
        o.pitch = pitch;
        o.yaw = yaw;
        o.zoom = s.outcast_zoom;
        o.ring_pattern = RingPattern::ALL[s.ring];

        let env = look.energy.env(s, fr) * look.movement.mask(s, pd, i, fr) * movers_mask;
        let col = look.beam.rgbw * env * s.brightness * beat1 * trim.global;
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
        let env = look.energy.env(s, fr) * look.movement.mask(s, pd, i, fr) * movers_mask;
        let Swatch { rgbw: Rgbw(r, g, b, w), one, two, spot } = look.beam;
        let lit = r.max(g).max(b).max(w) * spot as u8 as f32;
        h.alpha = lit * env * s.brightness * beat1 * trim.global * trim.hydro;
        h.color = one;
        h.color2 = two;
        // Hold the beam down while a wheel crosses to a slot it is not already
        // beside, since the fixture will not blank itself for it.
        h.color_mask = true;
        h.gobo = look.texture.gobo;
        h.gobo_shake = look.texture.gobo_shake;
        h.gobo_rot = look.texture.gobo_rot;
        h.prism = s.prism;
        // A mover hung the other way round has to spin its prism the other way
        // for the pair to read as one gesture.
        h.prism_rot = s.prism_rot * rest.sign().1;
        h.focus = look.texture.focus;
        h.zoom = s.hydro_zoom;
        h.frost = look.texture.frost;
    }

    for (i, p) in l.pars.iter_mut().enumerate() {
        let fr = i as f32 / 6.0;
        let env = look.energy.env(s, fr) * pars_mask;
        p.color = look.par * env * s.brightness * beat0 * trim.global * trim.colorado;
        p.zoom = s.colorado_zoom;
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
            // Colour banks
            Input::Up(true) => s.bank = 0,
            Input::Down(true) => s.bank = 1,
            Input::Left(true) => s.bank = 2,
            Input::Right(true) => s.bank = 3,
            // Toggle the pad guide
            Input::Capture(true) => {
                s.debug = !s.debug;
                pad.send(Output::Clear);
            }
            Input::Session(true) => s.zoom_wave = !s.zoom_wave,
            // Normal / double / half time, which the next grid press drops
            Input::Note(true) => {
                s.phi_mul = match s.phi_mul {
                    m if m > 1.0 => 0.5,
                    m if m < 1.0 => 1.0,
                    _ => 2.0,
                }
            }
            // Cycle Perform -> Easy -> Auto
            Input::Custom(true) => s.mode = s.mode.next(),
            // Pop hold specials
            Input::Release(i) => {
                let Coord(x, y) = i.into();
                s.specials.retain(|a| !(matches!(a.special.fire, Fire::Hold) && a.xy == (x, y)));
            }
            _ => {}
        }

        // Right column: brightness
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
            s.brightness = LEVELS[level];
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
                        // at, in the colour it would use.
                        PadOp::Look(look) => {
                            let col = look.color.map_or(beam, |c| c.rgbw);
                            let env = look.energy.map_or(0.0, |e| e.env(s, 0.0));
                            set(*x, *y, col * env.max(0.15));
                        }
                        PadOp::Palette(palette) => set(*x, *y, palette.beam_color(s).rgbw),
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
            Mode::Easy => {
                let look = s.look();
                for x in 0..8 {
                    for y in 0..8 {
                        let col = match (x < 4, y < 4) {
                            (true, true) => Rgbw::BLUE * s.pd(Pd(2, 1)).ramp(1.0).inv(),
                            (false, true) => Rgbw::RED * s.pd(Pd(1, 1)).ramp(1.0).inv(),
                            (true, false) => look.beam.rgbw,
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
                        set(x, y, look.beam.rgbw * env);
                    }
                }
            }
        }
    }

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
    set(
        6,
        8,
        match s.mode {
            Mode::Perform => Rgbw::WHITE,
            Mode::Easy => Rgbw::CYAN,
            Mode::Auto => Rgbw::MAGENTA,
        },
    );
    set(7, 8, lit(s.debug, Rgbw::WHITE));

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

    pad.send(Output::Batch(batch));
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
