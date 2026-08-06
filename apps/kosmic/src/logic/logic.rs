use itertools::Itertools;
use lib::prelude::*;
use rand::Rng;
use rand::rngs::ThreadRng;

use super::look::*;
use super::palette::*;
use super::special::*;
use crate::dmx::Patch;
use crate::lights::Lights;
use crate::logic::{PadBinding, PadOp};
use crate::mirror::{Ctrl, Pad};
use crate::sim::Movers;
use crate::{beat, bind, energy, func, palette, special};

///////////////////////// BINDINGS /////////////////////////

bind! {

perform:
    // Tap to record BPM
    (0, 7) => func!("tap bpm", Rgbw::VIOLET, |s, _| s.bpm_taps.push(s.t)),
    // Tap to calculate BPM and reset phase
    (7, 7) => func!("apply bpm", Rgbw::VIOLET, |s, _| s.apply_bpm()),

    // Beats
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

    // --- Palettes ---
    // Whites
    (0, 6) => palette!(Solid(Rgbw::WHITE)),
    (0, 5) => palette!(Solid(Rgbw::RGBW)),

    // Reds
    (1, 5) => palette!(Solid(Rgbw::RED)),
    (1, 6) => palette!(Solid(Rgbw::ORANGE)),
    (1, 7) => palette!(Split(Rgbw::WHITE, Rgbw::RED)),

    (2, 5) => palette!(Solid(Rgbw::HOUSE)),
    (2, 6) => palette!(Solid(Rgbw::YELLOW)),
    (2, 7) => palette!(Solid(Rgbw::PEA)),

    (3, 5) => palette!(Solid(Rgbw::MINT)),
    (3, 6) => palette!(Solid(Rgbw::LIME)),
    (3, 7) => palette!(Split(Rgbw::WHITE, Rgbw::LIME)),

    (4, 5) => palette!(Solid(Rgbw::CYAN)),
    (4, 6) => palette!(Solid(Rgbw::BLUE)),
    (4, 7) => palette!(Split(Rgbw::WHITE, Rgbw::BLUE)),

    (5, 5) => palette!(Solid(Rgbw::VIOLET)),
    (5, 6) => palette!(Solid(Rgbw::MAGENTA)),
    (5, 7) => palette!(Split(Rgbw::BLUE, Rgbw::VIOLET)),

    (6, 5) => palette!(Cycle([Rgbw::RED, Rgbw::WHITE])),
    (6, 6) => palette!(Cycle([Rgbw::LIME, Rgbw::WHITE])),
    (6, 7) => palette!(Cycle([Rgbw::BLUE, Rgbw::WHITE])),

    // Rainbows
    (7, 5) => palette!(Rainbow),
    (7, 6) => palette!(Cycle([Rgbw::RED, Rgbw::ORANGE, Rgbw::YELLOW, Rgbw::LIME, Rgbw::BLUE, Rgbw::VIOLET])),

    // --- x=1: Energy ladder. Re-press the active step to reroll. ---
    (1, 0) => energy!(Off),
    (1, 1) => energy!(On),
    (1, 2) => energy!(Beat { pd: Pd(2, 1) }),
    (1, 3) => energy!(Beat { pd: Pd(1, 1) }),
    (1, 4) => energy!(Strobe { pd: Pd(1, 4), duty: 1.0 }),

    // --- x=2..6: Specials ---
    // y=4: holds
    (2, 4) => special!(BLACKOUT),
    (3, 4) => special!(WHITE_STROBE),
    (4, 4) => special!(SNIPER),
    // y=3: one-shots
    (2, 3) => special!(RISER),
    (3, 3) => special!(WHIRL),
    // y=2: latches
    (2, 2) => special!(PRISM),
    (3, 2) => special!(GOBO),
    (4, 2) => special!(DJ),
}

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
    pub mode: Mode,

    /// Current color palette.
    pub palette: Box<dyn Palette>,
    /// Current style, an index into STYLES.
    pub style: usize,
    /// Current energy level.
    pub energy: Energy,
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

    /// Whether the 4 directional buttons (shift) are held.
    pub shift: [bool; 4],
    /// Style browser, shown while shift 0 is held.
    pub browse: bool,
    /// Last 8-beat bar Auto mode rerolled on.
    pub auto_bar: usize,
}

/// Presses this close to a predicted beat snap `phi` to it (small, so
/// way-off accidental hits don't shift the clock).
const SYNC_MOE: f32 = 0.09;

impl State {
    pub fn new() -> Self {
        Self {
            perform: perform(),
            mode: Mode::Perform,

            palette: Box::new(Rainbow),
            style: 0,
            energy: Energy::Off,
            seed: 0,
            specials: vec![],

            t: 0.0,
            bpm: 120.0,
            bpm_taps: vec![],
            phi: 0.0,
            phi_mul: 1.0,
            beat: None,

            brightness: 1.0,
            shift: [false; 4],
            browse: false,
            auto_bar: 0,
        }
    }

    pub fn phi(&self, n: usize, d: usize) -> f32 {
        self.pd(Pd(n, d))
    }
    pub fn pd(&self, pd: Pd) -> f32 {
        self.phi.fmod_div(pd.fr())
    }

    /// Resolve the base look (energy + style + palette) through the specials stack.
    pub fn look(&self) -> Looked {
        let style = &STYLES[self.style];
        let mut look = Looked {
            energy: self.energy,
            movement: style.movement(self.seed),
            texture: style.texture,
            beam: self.palette.beam_color(self),
            par: self.palette.par_color(self),
        };
        for a in &self.specials {
            let o = &a.special.look;
            if let Some(e) = o.energy {
                look.energy = e;
            }
            if let Some(m) = o.movement {
                look.movement = m;
            }
            if let Some(t) = o.texture {
                look.texture = t;
            }
            if let Some(c) = o.color {
                look.beam = c;
                look.par = c;
            }
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
        match self.bpm_taps.len() {
            0 => self.phi = 0.0,
            1 => self.bpm_taps.clear(),
            n => {
                let dts = self.bpm_taps.drain(..).tuple_windows().map(|(t0, t1)| t1 - t0);
                let dt = dts.sum::<f32>() / (n as f32 - 1.0);
                self.bpm = 60.0 / dt;
                self.phi = 0.0;
                info!("Calculated bpm={:.2} from {n} samples", self.bpm);
            }
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

    // Auto: reroll every 8 beats, bigger swaps less often
    if let Mode::Auto = s.mode {
        let bar = (s.phi / 8.0) as usize;
        if bar != s.auto_bar {
            s.auto_bar = bar;
            s.seed += 1;
            let mut rng = ThreadRng::default();
            if bar % 2 == 0 {
                s.palette = random_palette(&mut rng);
            }
            if bar % 4 == 0 {
                s.style = rng.gen_range(0..STYLES.len());
            }
        }
    }
}

///////////////////////// LIGHTS /////////////////////////

// TODO: measure the Hydro color wheel slot bytes, then map to the nearest slot.
fn hydro_color(_c: Rgbw) -> u8 {
    0
}

pub fn render_lights(
    s: Res<State>,
    mut l: ResMut<Lights>,
    movers: Res<Movers>,
    patch: Res<Patch>,
    mut universe: ResMut<Universe>,
) {
    let s: &State = &s;
    let look = s.look();
    let pd = look.energy.pd().unwrap_or(Pd(4, 1));

    let beat0 = s.beat_fr0().unwrap_or(1.0);
    let beat1 = s.beat_fr1().unwrap_or(1.0);

    l.reset();

    // Movers: outcasts are 0..2, hydros 2..4
    for (i, o) in l.outcasts.iter_mut().enumerate() {
        let fr = i as f32 / 4.0;
        let (pitch, yaw) = look.movement.angles(s, pd, i, fr, &movers.0[i]);
        o.pitch = pitch;
        o.yaw = yaw;
        o.zoom = look.texture.zoom;

        let env = look.energy.env(s, fr) * look.movement.mask(s, pd, i, fr);
        let col = look.beam * env * s.brightness * beat0;
        match look.texture.ring {
            RingMode::Both => {
                o.ring = col;
                o.center = col;
            }
            RingMode::Ring => o.ring = col,
            RingMode::Center => o.center = col,
        }
    }

    for (i, h) in l.hydros.iter_mut().enumerate() {
        let (i, fr) = (i + 2, (i + 2) as f32 / 4.0);
        let (pitch, yaw) = look.movement.angles(s, pd, i, fr, &movers.0[i]);
        h.pitch = pitch;
        h.yaw = yaw;

        // No RGB: brightest component drives the dimmer, wheels take the texture
        let env = look.energy.env(s, fr) * look.movement.mask(s, pd, i, fr);
        let Rgbw(r, g, b, w) = look.beam;
        h.alpha = r.max(g).max(b).max(w) * env * s.brightness * beat0;
        h.color = hydro_color(look.beam);
        h.gobo = look.texture.gobo;
        h.gobo_rot = look.texture.gobo_rot;
        h.prism = look.texture.prism;
        h.prism_rot = look.texture.prism_rot;
        h.focus = look.texture.focus;
        h.zoom = look.texture.zoom;
        h.frost = look.texture.frost;
    }

    for (i, p) in l.pars.iter_mut().enumerate() {
        let fr = i as f32 / 6.0;
        let env = look.energy.env(s, fr);
        p.color = look.par * env * s.brightness * beat1;
        p.zoom = look.texture.zoom;
    }

    l.encode(&patch, &mut universe);
}

///////////////////////// PAD INPUT /////////////////////////

pub fn on_pad(mut s: ResMut<State>, mut pad: ResMut<Pad>) {
    use lib::midi::device::launchpad_x::Input;
    use lib::midi::device::launchpad_x::types::*;

    let s: &mut State = &mut *s;
    let pad: &mut Pad = &mut *pad;

    for input in pad.recv() {
        match input {
            Input::Up(b) => {
                s.shift[0] = b;
                s.browse = b;
            }
            Input::Down(b) => s.shift[1] = b,
            Input::Left(b) => s.shift[2] = b,
            Input::Right(b) => s.shift[3] = b,
            // Cycle Perform -> Easy -> Auto
            Input::Custom(true) => s.mode = s.mode.next(),
            // Pop hold specials
            Input::Release(i) => {
                let Coord(x, y) = i.into();
                s.specials.retain(|a| !(matches!(a.special.fire, Fire::Hold) && a.xy == (x, y)));
            }
            _ => {}
        }

        let Some((x, y)) = input.xy() else { continue };
        debug!("Pad({x}, {y})");

        // Right column: brightness
        if x == 8 && y < 8 {
            s.brightness = y as f32 / 7.0;
            continue;
        }
        if x > 7 || y > 7 {
            continue;
        }

        // Style browser while shift 0 is held
        if s.browse {
            let i = (y * 8 + x) as usize;
            if i < STYLES.len() {
                s.style = i;
                info!("style: {}", STYLES[i].name);
            }
            continue;
        }

        match s.mode {
            Mode::Perform => {
                let Some(op) = s.perform.iter().find(|b| b.xy == (x, y)).map(|b| b.op.clone()) else {
                    continue;
                };

                match op {
                    PadOp::Func { func, .. } => func(s, pad),
                    PadOp::Energy(e) => {
                        if e == s.energy {
                            s.seed += 1;
                        } else {
                            s.energy = e;
                        }
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
            Mode::Easy => {
                let mut rng = ThreadRng::default();
                match (x < 4, y < 4) {
                    (true, true) => s.energy = Energy::Beat { pd: Pd(2, 1) },
                    (false, true) => s.energy = Energy::Beat { pd: Pd(1, 1) },
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
            Mode::Perform => {
                for PadBinding { xy: (x, y), op } in &s.perform {
                    match op {
                        PadOp::Energy(e) => {
                            let fr = if *e == s.energy { e.env(s, 0.0).max(0.3) } else { 0.1 };
                            set(*x, *y, Rgbw::WHITE * fr);
                        }
                        PadOp::Palette(palette) => set(*x, *y, palette.beam_color(s)),
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
    }

    // Shift 0: style browser
    set(0, 8, if s.browse { Rgbw::WHITE } else { Rgbw::BLACK });

    // Mode light
    set(
        6,
        8,
        match s.mode {
            Mode::Perform => Rgbw::WHITE,
            Mode::Easy => Rgbw::CYAN,
            Mode::Auto => Rgbw::MAGENTA,
        },
    );

    // Right column: brightness
    for y in 0..8 {
        set(8, y, Rgbw::WHITE * (y as f32 / 7.0));
    }

    // Beat indicator
    set(
        8,
        8,
        match s.pd(Pd(1, 1)).bsquare(1.0, 0.1) {
            true => match s.pd(Pd(4, 1)).bsquare(1.0, 0.2) {
                // Purple on the first beat of each bar
                true => Rgbw::VIOLET,
                // White on every other beat
                false => Rgbw::WHITE,
            },
            false => Rgbw::BLACK,
        },
    );

    pad.send(Output::Batch(batch));
}

///////////////////////// CTRL /////////////////////////

pub fn on_ctrl(mut s: ResMut<State>, mut ctrl: ResMut<Ctrl>) {
    use lib::midi::device::launch_control_xl::Input;

    for input in ctrl.recv() {
        debug!("ctrl: {input:?}");

        match input {
            Input::Slider(7, fr) => s.brightness = fr,
            _ => {}
        }
    }
}
