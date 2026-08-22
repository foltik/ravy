pub use lib::dmx::device::beam_rgbw_60w::BeamRing;
use lib::prelude::*;

use super::logic::State;

///////////////////////// LOOK /////////////////////////

/// Which fixtures a look lights, one flag per family, so a look can solo or
/// combine any of them. The named presets cover the grid; compose new ones
/// freely.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mask {
    pub beams: bool,
    pub bigs: bool,
    pub pars: bool,
    pub spiders: bool,
    pub bars: bool,
    pub strobe: bool,
}

impl Mask {
    const fn of(beams: bool, bigs: bool, pars: bool, spiders: bool, bars: bool, strobe: bool) -> Self {
        Self { beams, bigs, pars, spiders, bars, strobe }
    }

    pub const ALL: Self = Self::of(true, true, true, true, true, true);
    /// The classic movers group: both beam banks with their accents.
    pub const MOVERS: Self = Self::of(true, true, false, true, true, false);
    /// The classic pars group: the par bed with the strobe.
    pub const PARS: Self = Self::of(false, false, true, false, false, true);
    pub const BEAMS: Self = Self::of(true, false, false, false, false, false);
    pub const BIGS: Self = Self::of(false, true, false, false, false, false);
    pub const SPIDERS: Self = Self::of(false, false, false, true, false, false);
    /// Bars and spiders alone: the glow around a dark rig.
    pub const ACCENTS: Self = Self::of(false, false, false, true, true, false);
    pub const BEAM_SPIDERS: Self = Self::of(true, false, false, true, false, false);
    pub const PAR_SPIDERS: Self = Self::of(false, false, true, true, false, false);

    pub fn mover(self, m: Mover) -> f32 {
        (if m.big { self.bigs } else { self.beams }) as u8 as f32
    }
    pub fn par(self) -> f32 {
        self.pars as u8 as f32
    }
    pub fn spider(self) -> f32 {
        self.spiders as u8 as f32
    }
    pub fn bar(self) -> f32 {
        self.bars as u8 as f32
    }
    pub fn flash(self) -> f32 {
        self.strobe as u8 as f32
    }
    /// Broad looks keep the hut side lights up; solos and blackouts drop them.
    pub fn broad(self) -> bool {
        self == Self::ALL || self == Self::MOVERS || self == Self::PARS
    }
}

/// A partial look: `None` = inherit from the layer below.
#[derive(Clone, Copy, Debug, Default)]
pub struct Look {
    pub energy: Option<Energy>,
    pub movement: Option<Movement>,
    /// How long one pass of the movement takes. Defaults to the energy's own.
    pub movement_pd: Option<Pd>,
    pub color: Option<Rgbw>,
    pub mask: Option<Mask>,
}

impl Look {
    pub const NONE: Self =
        Self { energy: None, movement: None, movement_pd: None, color: None, mask: None };

    pub const fn new(energy: Energy, movement: Movement, movement_pd: Pd) -> Self {
        Self {
            energy: Some(energy),
            movement: Some(movement),
            movement_pd: Some(movement_pd),
            color: None,
            mask: None,
        }
    }

    pub const fn masked(mut self, mask: Mask) -> Self {
        self.mask = Some(mask);
        self
    }

    pub const fn colored(mut self, color: Rgbw) -> Self {
        self.color = Some(color);
        self
    }
}

/// A fully resolved look, ready to render.
pub struct Looked {
    pub energy: Energy,
    pub movement: Movement,
    pub movement_pd: Option<Pd>,
    pub beam: Rgbw,
    pub par: Rgbw,
    pub mask: Mask,
}

impl Looked {
    /// Stack a partial look on top, `None` leaving what is already here.
    pub fn apply(&mut self, o: &Look) {
        if let Some(e) = o.energy {
            self.energy = e;
        }
        if let Some(m) = o.movement {
            self.movement = m;
            self.movement_pd = o.movement_pd;
        }
        if let Some(c) = o.color {
            self.beam = c;
            self.par = c;
        }
        if let Some(m) = o.mask {
            self.mask = m;
        }
    }

    /// How long one pass of the movement takes.
    pub fn pd(&self) -> Pd {
        self.movement_pd.or_else(|| self.energy.pd()).unwrap_or(Pd(4, 1))
    }

    /// Where a mover points. A dark rig has nothing to show, so the movers
    /// park pointing out rather than holding wherever the pattern left them.
    pub fn angles(&self, s: &State, pd: Pd, m: Mover) -> (f32, f32) {
        match self.energy {
            Energy::Off => Movement::Out.angles(s, pd, m),
            _ => self.movement.angles(s, pd, m),
        }
    }
}

///////////////////////// ENERGY /////////////////////////

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Energy {
    Off,
    On,
    Beat { pd: Pd },
    Strobe { pd: Pd, duty: f32 },
    /// Hard chase, one fixture lit at a time.
    Chase { pd: Pd },
    /// Chase with the fixtures fading up and down into each other.
    Swell { pd: Pd },
    /// The two halves of the rig flashing against each other.
    Alternate { pd: Pd },
}

impl Energy {
    pub fn pd(&self) -> Option<Pd> {
        match self {
            Energy::Off | Energy::On => None,
            Energy::Beat { pd }
            | Energy::Strobe { pd, .. }
            | Energy::Chase { pd }
            | Energy::Swell { pd }
            | Energy::Alternate { pd } => Some(*pd),
        }
    }

    /// Brightness envelope, with `fr` staggering chases across fixtures.
    pub fn env(&self, s: &State, fr: f32) -> f32 {
        match self {
            Energy::Off => 0.0,
            Energy::On => 1.0,
            Energy::Beat { pd } => s.pd(pd.mul(2)).ramp(1.0).inv().lerp(0.2..1.0).in_quad(),
            Energy::Strobe { pd, duty } => {
                s.pd(pd.mul(2)).square(1.0, duty.in_exp().lerp(1.0..0.5))
            }
            Energy::Chase { pd } => {
                // Golden-ratio scramble: the order reads shuffled but the
                // hits stay evenly spread, rotated fresh by each press.
                let r = ((fr + (s.seed % 16) as f32 / 16.0) * 6.18).fract();
                s.pd(pd.mul(2)).phase(1.0, r).square(1.0, 0.1)
            }
            Energy::Swell { pd } => s.pd(pd.mul(4)).phase(1.0, fr).tri(1.0),
            Energy::Alternate { pd } => {
                s.pd(*pd).phase(1.0, if fr < 0.5 { 0.0 } else { 0.5 }).square(1.0, 0.33)
            }
        }
    }
}

///////////////////////// STYLES /////////////////////////

/// A curated identity: a family of movements the reroll picks within. Selected
/// in the style browser, rerolled by seed.
pub struct Style {
    pub name: &'static str,
    pub color: Rgbw,
    pub movements: &'static [Movement],
}

impl Style {
    pub fn movement(&self, seed: usize) -> Movement {
        self.movements[seed % self.movements.len()]
    }
}

#[rustfmt::skip]
pub const STYLES: &[Style] = &[
    Style { name: "wash",    color: Rgbw::CYAN,    movements: &[Movement::Out, Movement::WaveY] },
    Style { name: "beams",   color: Rgbw::WHITE,   movements: &[Movement::SpreadOut, Movement::CrissCross { pitch: 0.4 }] },
    Style { name: "cross",   color: Rgbw::ORANGE,  movements: &[Movement::CrossSway, Movement::Cross { pitch: 0.3, angle: None, fanning: None }] },
    Style { name: "whirl",   color: Rgbw::VIOLET,  movements: &[Movement::Whirl, Movement::Spinner] },
    Style { name: "sniper",  color: Rgbw::RED,     movements: &[Movement::Out, Movement::SnapX] },
    Style { name: "square",  color: Rgbw::MAGENTA, movements: &[Movement::Square, Movement::SnapY] },
    Style { name: "riser",   color: Rgbw::MINT,    movements: &[Movement::RaisingBeams] },
    Style { name: "chaos",   color: Rgbw::PINK,    movements: &[Movement::Twisting, Movement::DarthMaul] },
    Style { name: "ripple",  color: Rgbw::BLUE,    movements: &[
        Movement::Ripple { dir: Dir::Ltr, size: 1.1 },
        Movement::Ripple { dir: Dir::Rtl, size: 1.1 },
        Movement::Ripple { dir: Dir::Ltr, size: 1.6 },
        Movement::Ripple { dir: Dir::Rtl, size: 1.6 },
    ] },
];

///////////////////////// MOVERS /////////////////////////

/// One mover, addressed by where it stands on stage.
#[derive(Clone, Copy, Debug)]
pub struct Mover {
    /// True for the BigBeam bank, false for the Beam bank.
    pub big: bool,
    /// Index within the bank.
    pub i: usize,
    /// Stage position, 0.0 leftmost to 1.0 rightmost.
    pub x: f32,
    /// Takes the palette's par-side colour and the left beat, giving every
    /// split a checkerboard across the line instead of one solid wash.
    pub alt: bool,
}

impl Mover {
    /// Classic per-bank phase, which pairs beam `i` with bigbeam `i`.
    pub fn fr(self) -> f32 {
        self.i as f32 / 4.0
    }
}

/// The movers as they stand on stage, left to right, colour groups mirrored
/// about the center.
pub const MOVERS: [Mover; 8] = [
    Mover { big: true, i: 0, x: 0.0 / 7.0, alt: false },
    Mover { big: false, i: 0, x: 1.0 / 7.0, alt: true },
    Mover { big: false, i: 1, x: 2.0 / 7.0, alt: false },
    Mover { big: true, i: 1, x: 3.0 / 7.0, alt: true },
    Mover { big: true, i: 2, x: 4.0 / 7.0, alt: true },
    Mover { big: false, i: 2, x: 5.0 / 7.0, alt: false },
    Mover { big: false, i: 3, x: 6.0 / 7.0, alt: true },
    Mover { big: true, i: 3, x: 7.0 / 7.0, alt: false },
];

/// Pars in the beam-side colour group, flashing with the right beat, so the
/// par bed checkers the same way the mover line does.
pub const PAR_ALT: [bool; 9] =
    [false, true, false, false, true, false, false, false, false];

/// Which way a semantic pattern travels across the stage line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Dir {
    Ltr,
    Rtl,
}

impl Dir {
    /// Distance along the travel: `x` measured from the end it starts at.
    pub fn along(self, x: f32) -> f32 {
        match self {
            Dir::Ltr => x,
            Dir::Rtl => 1.0 - x,
        }
    }
}

///////////////////////// MOVEMENT /////////////////////////

/// The mslive beam patterns, in the raw normalized pan and tilt they were
/// written in for the classic rig. Classic patterns key off the bank index,
/// driving beam `i` and bigbeam `i` as a pair; semantic ones aim each of the
/// eight movers by its stage position.
#[derive(Clone, Copy, Debug)]
#[allow(unused)]
pub enum Movement {
    Down,
    Out,
    Center,
    SpreadOut,
    SpreadIn,
    Cross { pitch: f32, angle: Option<f32>, fanning: Option<f32> },
    CrissCross { pitch: f32 },
    /// A cross whose pitch and spread breathe in and out over eight passes.
    CrossSway,
    WaveY,
    SnapX,
    SnapY,
    Square,
    Whirl,
    RaisingBeams,
    Twisting,
    DarthMaul,
    Spinner,
    UpDownWave,
    /// A crest running across the stage line and back: each mover dips from
    /// straight up and back as it passes. `dir` is the end it starts from,
    /// `size` the crest width as a fraction of the line: the up-to-down
    /// gradient spans `size / 2`, so over 1.0 reads as a wave arcing across
    /// the rig rather than solo dips.
    Ripple { dir: Dir, size: f32 },
    /// Every mover tracing the same wide circle about a forward-and-up axis,
    /// one revolution per `pd`, spaced evenly around it, so the beams fan
    /// like rotating eyelashes.
    Spin,
    /// The whirl without the blackout: every mover leans a little off
    /// vertical and pans the full yaw range and back, phase-staggered down
    /// the line, so the beams swirl about the zenith continuously.
    Carousel,
    /// A symmetric bloom: each mover leans away from the center of the line,
    /// the outermost by `amount / 2`, the whole V breathing between nearly
    /// closed and fully open over each `pd`.
    Fan { amount: f32 },
    /// The line converging back and up over the hut, outer movers leaning in
    /// hardest, each beam drawing a slow circle of radius `orbit` around its
    /// aim so the teepee shimmers.
    Hut { orbit: f32 },
}

/// Aiming conventions, as the rig stands: pitch 0.5 is straight up, each
/// +0.1 tipping ~19 deg toward the azimuth the yoke faces. Yaw covers 1.5
/// turns; home (0.0) faces the crowd, 0.167 stage-left, 0.333 the hut,
/// 0.5 stage-right.
const YAW_LEFT: f32 = 0.167;
const YAW_HUT: f32 = 0.333;
/// The crowd again, wound a full turn past home, so a pattern can wobble to
/// either side of it without clamping at yaw 0.
const YAW_CROWD: f32 = 0.667;

/// How far the ripple's crest tips a mover from straight up, in pitch.
const RIPPLE_DIP: f32 = 0.35;

/// The spin cone: axis facing the crowd, 55 deg off vertical, 40 deg of
/// radius, so the bottom of the circle grazes horizontal and the top stands
/// near vertical. The yaw radius is the pitch radius over sin(axis tilt),
/// which keeps the swing circular. 7/8 of spread spaces the eight movers
/// evenly around the circle, an eighth of a turn apart, so the line always
/// shows the full fan rather than a bunched arc.
const SPIN_PITCH: f32 = 0.79;
const SPIN_R_PITCH: f32 = 0.21;
const SPIN_R_YAW: f32 = 0.09;
const SPIN_SPREAD: f32 = 0.875;

/// How far off vertical the carousel leans its movers.
const CAROUSEL_TILT: f32 = 0.16;

/// The hut aim: every mover leans back-and-inward at the hut, yaw splayed
/// around dead-back so the ends point back-right / back-left, the center
/// straight back, with the outermost leaning hardest.
const HUT_TILT_IN: f32 = 0.12;
const HUT_TILT_OUT: f32 = 0.3;
const HUT_SPLAY: f32 = 0.23;

/// The travelling crest at stage position `x`: 1.0 centered on the crest,
/// falling smoothly to 0.0 half a `size` away. Over one `pd` the crest runs
/// from the `dir` end of the line to the far end and back, reflecting at the
/// ends so the edge movers dip once per visit instead of twice.
pub fn ripple(s: &State, pd: Pd, dir: Dir, size: f32, x: f32) -> f32 {
    let c = s.pd(pd).tri(1.0);
    let d = dir.along(x) - c;
    match d.abs() < size * 0.5 {
        true => 0.5 + 0.5 * (std::f32::consts::TAU * d / size).cos(),
        false => 0.0,
    }
}

fn cross(i: usize, pitch: f32, angle: Option<f32>, fanning: Option<f32>) -> (f32, f32) {
    let a = angle.unwrap_or(0.13);
    let f = if i == 1 || i == 2 { fanning.unwrap_or(1.0) } else { 1. };
    (
        pitch * f,
        match i {
            0 => 0.5 + a,
            1 => 0.5 + a,
            2 => 0.5 - a,
            _ => 0.5 - a,
        } - (0.25 / 1.5),
    )
}

impl Movement {
    /// Calculate (pitch, yaw) for the given pattern.
    pub fn angles(self, s: &State, pd: Pd, m: Mover) -> (f32, f32) {
        let (i, fr) = (m.i, m.fr());
        match self {
            Movement::Down => (0.0, 0.0),
            Movement::Out => (0.5, 0.0),
            Movement::Center => (
                0.85,
                match i {
                    0 => 0.05,
                    1 => 0.7,
                    2 => 0.63,
                    _ => 0.6,
                },
            ),
            Movement::SpreadOut => (
                0.0,
                match i {
                    0 => 0.5 - 0.05,
                    1 => 0.5 - 0.02,
                    2 => 0.5 + 0.02,
                    _ => 0.5 + 0.05,
                } - (0.25 / 1.5),
            ),
            Movement::SpreadIn => (
                0.0,
                match i {
                    0 => 0.5 + 0.09,
                    1 => 0.5 + 0.07,
                    2 => 0.5 - 0.07,
                    _ => 0.5 - 0.09,
                } - (0.25 / 1.5),
            ),
            Movement::Cross { pitch, angle, fanning } => cross(i, pitch, angle, fanning),
            Movement::CrissCross { pitch } => (
                pitch,
                match i {
                    0 => 0.5 + 0.08,
                    1 => 0.5 - 0.05,
                    2 => 0.5 + 0.05,
                    _ => 0.5 - 0.08,
                } - (0.25 / 1.5),
            ),
            Movement::CrossSway => {
                let t = s.pd(pd.mul(8)).fsin(1.0);
                cross(i, (1.0 - t) * 0.3 + 0.1, Some(t * 0.2 - 0.1), Some(1.5))
            }
            Movement::SnapY => {
                let t = s.pd(pd.mul(4)).square(1.0, 0.5);
                let pitch = 0.3
                    * match i % 2 == 0 {
                        true => t,
                        false => 1.0 - t,
                    };
                (pitch, 0.5)
            }
            Movement::SnapX => {
                let t = s.pd(pd.mul(4)).negsquare(1.0, 0.5);
                let pitch = 0.3 * s.pd(pd.mul(2)).square(1.0, 0.5);
                let yaw = 0.5
                    + 0.13
                        * match i > 1 {
                            true => t,
                            false => -t,
                        };
                (pitch, yaw)
            }
            Movement::WaveY => {
                let t = s.pd(pd.mul(4)).tri(1.0);
                let pitch = 0.15 + 0.40 * t;
                (1.0 - pitch, 0.0)
            }
            Movement::Square => {
                let t_pitch = s.pd(pd.mul(4)).phase(1.0, 0.25).square(1.0, 0.5);
                let t_yaw = match i % 2 == 0 {
                    true => s.pd(pd.mul(4)).negsquare(1.0, 0.5),
                    false => s.pd(pd.mul(4)).phase(1.0, 0.5).negsquare(1.0, 0.5),
                };
                let pitch = 0.1
                    + 0.25
                        * match i % 2 == 0 {
                            true => t_pitch,
                            false => 1.0 - t_pitch,
                        };
                let yaw = 0.5 + 0.08 * t_yaw;
                (pitch, yaw - 0.25 / 1.5)
            }
            Movement::Whirl => {
                let angle = (s.pd(pd) + fr * 1.5) % 1.0;
                match WhirlState::from_angle(angle) {
                    WhirlState::FullyResetting { pitch, yaw } => (pitch, yaw),
                    WhirlState::DoingSubrotation { pitch, yaw, .. } => (pitch, yaw),
                }
            }
            Movement::RaisingBeams => {
                let angle = (s.pd(pd.mul(2)) + fr * 2.0) % 1.0;
                let pitch = if angle < 0.7 {
                    0.5 - angle / 0.7 * 0.5
                } else if angle < 0.9 {
                    0.6
                } else {
                    0.5 - (angle - 0.9)
                };
                let pitch = 1.0 - pitch;

                (pitch * 0.9, 0.0)
            }
            Movement::Twisting => {
                use rand::prelude::*;
                let seed = (s.pd(pd.mul(512)) * 255.) as u8;
                let mut seed_array = [seed; 32];
                seed_array[0] = i as u8;
                let mut rng = rand::prelude::StdRng::from_seed(seed_array);
                let yaw = rng.sample(rand::distributions::Uniform::new(0.0, 1.0));
                let pitch = rng.sample(rand::distributions::Uniform::new(0.2, 0.8));
                (pitch, yaw)
            }
            Movement::DarthMaul => (
                0.2,
                match i {
                    _ if i % 2 == 0 => s.pd(pd.mul(8)).tri(1.0).lerp(0.2..0.8),
                    _ if i % 2 == 1 => s.pd(pd.mul(8)).tri(1.0).lerp(0.2..0.8) + 0.66,
                    _ => 0.0,
                },
            ),
            Movement::Spinner => (0.3, s.pd(Pd(8, 1)).phase(1.0, fr * 0.1).square(1.0, 0.5)),
            Movement::UpDownWave => (0.2, s.pd(Pd(8, 1)).phase(1.0, fr * 0.1).square(1.0, 0.5)),
            Movement::Ripple { dir, size } => {
                (0.5 + RIPPLE_DIP * ripple(s, pd, dir, size, m.x), 0.0)
            }
            Movement::Spin => {
                let θ = std::f32::consts::TAU * (s.pd(pd) + m.x * SPIN_SPREAD);
                (SPIN_PITCH + SPIN_R_PITCH * θ.cos(), YAW_CROWD + SPIN_R_YAW * θ.sin())
            }
            Movement::Carousel => {
                let t = (s.pd(pd) + m.x * SPIN_SPREAD) % 1.0;
                (0.5 + CAROUSEL_TILT, t.tri(1.0))
            }
            Movement::Fan { amount } => {
                // Facing stage-left, positive pitch leans left: the left end
                // leans left, the right end right, opening across the stage.
                let breathe = s.pd(pd).fsin(1.0).lerp(0.3..1.0);
                (0.5 - amount * breathe * (m.x - 0.5), YAW_LEFT)
            }
            Movement::Hut { orbit } => {
                let θ = std::f32::consts::TAU * (s.pd(pd) + m.x);
                let out = (m.x - 0.5).abs() * 2.0;
                let pitch = 0.5 + out.lerp(HUT_TILT_IN..HUT_TILT_OUT) + orbit * θ.cos();
                let yaw = YAW_HUT - HUT_SPLAY * (m.x - 0.5) + orbit * 0.6 * θ.sin();
                (pitch, yaw)
            }
        }
    }

    /// Per-fixture brightness mask for patterns that hide the reset sweep.
    pub fn mask(self, s: &State, pd: Pd, m: Mover) -> f32 {
        let fr = m.fr();
        match self {
            Movement::Whirl => {
                let angle = (s.pd(pd) + fr * 1.5) % 1.0;
                let warmup = 0.1;
                match WhirlState::from_angle(angle) {
                    WhirlState::FullyResetting { .. } => 0.0,
                    WhirlState::DoingSubrotation { percentage, .. } => {
                        if percentage < warmup {
                            0.0
                        } else {
                            ((percentage - warmup) / (1.0 - warmup)).trapazoid(1.0, 1.0 / 16.0).powf(2.0)
                        }
                    }
                }
            }
            Movement::RaisingBeams => {
                let angle = (s.pd(pd) + fr * 2.0) % 1.0;
                if angle < 0.45 { (angle - 0.1).trapazoid(0.5, 0.1) } else { 0.0 }
            }
            _ => 1.0,
        }
    }
}

pub enum WhirlState {
    FullyResetting { pitch: f32, yaw: f32 },
    DoingSubrotation { pitch: f32, yaw: f32, percentage: f32 },
}
impl WhirlState {
    pub fn from_angle(angle: f32) -> Self {
        let pitch = 0.4;
        let full_reset_sector = 0.25;
        if angle > 1.0 - full_reset_sector {
            WhirlState::FullyResetting { pitch, yaw: 1.0 }
        } else {
            let rot_angle = angle / (1.0 - full_reset_sector);
            WhirlState::DoingSubrotation { pitch, yaw: 1.0 - rot_angle, percentage: rot_angle }
        }
    }
}

///////////////////////// SPIDERS /////////////////////////

/// The mslive spider gestures: where the two banks of a spider sit and how
/// they move against each other.
#[derive(Clone, Copy, Debug, PartialEq)]
#[allow(unused)]
pub enum SpiderPattern {
    Up,
    Down,
    Wave { pd: Pd },
    Alternate { pd: Pd },
    Snap { pd: Pd },
}

impl SpiderPattern {
    /// (pos0, pos1) for spider `i`.
    pub fn pos(self, s: &State, i: usize) -> (f32, f32) {
        match self {
            SpiderPattern::Up => (0.0, 0.52),
            SpiderPattern::Down => (0.67, 0.52),
            SpiderPattern::Wave { pd } => {
                let fr = s.pd(pd.mul(2)).tri(1.0);
                (fr, 1.0 - fr)
            }
            SpiderPattern::Alternate { pd } => {
                let t = s.pd(pd.mul(2));
                let t = match i {
                    0 => t,
                    _ => t.phase(1.0, 0.5),
                };
                let fr = t.tri(1.0);
                (fr, fr)
            }
            SpiderPattern::Snap { pd } => {
                let t = s.pd(pd.mul(2));
                let t = match i {
                    0 => t,
                    _ => t.phase(1.0, 0.5),
                };
                let fr = t.square(1.0, 0.5);
                (fr, fr)
            }
        }
    }

    /// The gesture that fits an energy, which is how mslive paired them.
    pub fn of(energy: Energy) -> Self {
        match energy {
            Energy::Off => SpiderPattern::Down,
            Energy::On => SpiderPattern::Wave { pd: Pd(4, 1) },
            Energy::Beat { pd } => SpiderPattern::Alternate { pd: pd.mul(2) },
            Energy::Swell { pd } => SpiderPattern::Wave { pd: pd.mul(2) },
            Energy::Strobe { .. } | Energy::Chase { .. } | Energy::Alternate { .. } => {
                SpiderPattern::Alternate { pd: Pd(2, 1) }
            }
        }
    }
}

///////////////////////// RINGS /////////////////////////

/// The Beam ring colours worth a button, in two rows of eight.
pub const RINGS: [BeamRing; 16] = [
    BeamRing::Off,
    BeamRing::Red,
    BeamRing::Green,
    BeamRing::Blue,
    BeamRing::Yellow,
    BeamRing::Purple,
    BeamRing::Teal,
    BeamRing::White,
    BeamRing::RedYellow,
    BeamRing::RedPurple,
    BeamRing::RedWhite,
    BeamRing::GreenYellow,
    BeamRing::GreenBlue,
    BeamRing::BluePurple,
    BeamRing::BlueWhite,
    BeamRing::Cycle,
];
