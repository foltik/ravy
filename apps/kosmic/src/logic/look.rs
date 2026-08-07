pub use lib::lights::fixture::HydroSpotGobo as Gobo;
pub use lib::lights::fixture::HydroSpotPrism as Prism;
pub use lib::lights::fixture::OutcastBeamwashRingPattern as RingPattern;
use lib::prelude::*;

use super::Swatch;
use super::logic::State;
use crate::dmx::{OUTCASTS, travel};
use crate::home::Rest;

///////////////////////// LOOK /////////////////////////

/// Which families of fixture a look lights.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mask {
    All,
    Movers,
    Pars,
}

impl Mask {
    pub fn movers(self) -> f32 {
        matches!(self, Mask::All | Mask::Movers) as u8 as f32
    }
    pub fn pars(self) -> f32 {
        matches!(self, Mask::All | Mask::Pars) as u8 as f32
    }
}

/// A partial look: `None` = inherit from the layer below.
#[derive(Clone, Copy, Debug, Default)]
pub struct Look {
    pub energy: Option<Energy>,
    pub movement: Option<Movement>,
    /// How long one pass of the movement takes. Defaults to the energy's own.
    pub movement_pd: Option<Pd>,
    pub texture: Option<Texture>,
    pub color: Option<Swatch>,
    pub ring_pattern: Option<RingPattern>,
    pub mask: Option<Mask>,
}

impl Look {
    pub const NONE: Self = Self {
        energy: None,
        movement: None,
        movement_pd: None,
        texture: None,
        color: None,
        ring_pattern: None,
        mask: None,
    };

    pub const fn new(energy: Energy, movement: Movement, movement_pd: Pd) -> Self {
        Self {
            energy: Some(energy),
            movement: Some(movement),
            movement_pd: Some(movement_pd),
            texture: None,
            color: None,
            ring_pattern: None,
            mask: None,
        }
    }

    pub const fn masked(mut self, mask: Mask) -> Self {
        self.mask = Some(mask);
        self
    }

    pub const fn colored(mut self, color: Swatch) -> Self {
        self.color = Some(color);
        self
    }
}

/// A fully resolved look, ready to render.
pub struct Looked {
    pub energy: Energy,
    pub movement: Movement,
    pub movement_pd: Option<Pd>,
    pub texture: Texture,
    pub beam: Swatch,
    pub par: Rgbw,
    pub ring_pattern: RingPattern,
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
        if let Some(t) = o.texture {
            self.texture = t;
        }
        if let Some(c) = o.color {
            self.beam = c;
            self.par = c.rgbw;
        }
        if let Some(p) = o.ring_pattern {
            self.ring_pattern = p;
        }
        if let Some(m) = o.mask {
            self.mask = m;
        }
    }

    /// How long one pass of the movement takes.
    pub fn pd(&self) -> Pd {
        self.movement_pd.or_else(|| self.energy.pd()).unwrap_or(Pd(4, 1))
    }

    /// Where fixture `i` points. A dark rig has nothing to show, so the movers
    /// park at home rather than holding wherever the pattern left them.
    pub fn angles(&self, s: &State, pd: Pd, i: usize, fr: f32, world: &Transform, home: &Rest) -> (f32, f32) {
        match self.energy {
            Energy::Off => Movement::Home.angles(s, pd, i, fr, world, home),
            _ => self.movement.angles(s, pd, i, fr, world, home),
        }
    }

    /// The zoom to hold, which at rest is whatever the fixture rests at.
    pub fn zoom(&self, home: &Rest) -> f32 {
        match self.energy {
            Energy::Off => home.zoom,
            _ => self.texture.zoom,
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
            Energy::Chase { pd } => s.pd(pd.mul(4)).phase(1.0, fr).square(1.0, 0.1),
            Energy::Swell { pd } => s.pd(pd.mul(4)).phase(1.0, fr).tri(1.0),
            Energy::Alternate { pd } => {
                s.pd(*pd).phase(1.0, if fr < 0.5 { 0.0 } else { 0.5 }).square(1.0, 0.33)
            }
        }
    }
}

///////////////////////// TEXTURE /////////////////////////

/// Which Outcast cells a look drives.
#[derive(Clone, Copy, Debug)]
pub enum RingMode {
    Both,
    Ring,
    Center,
}

/// Slow optics: wheels, prism, frost, zoom, focus. Colour is not here; it
/// comes down with the palette, in [`Swatch`].
// TODO: measure the Hydro gobo/prism slot bytes on the rig.
#[derive(Clone, Copy, Debug)]
pub struct Texture {
    pub zoom: f32,
    pub focus: f32,
    pub gobo: Gobo,
    /// How hard the gobo shakes, `0..1` slow to fast.
    pub gobo_shake: f32,
    pub gobo_rot: u8,
    pub prism: Prism,
    /// Signed, and turned round on a mover whose pan axis is flipped.
    pub prism_rot: f32,
    pub frost: f32,
    pub ring: RingMode,
}

impl Texture {
    pub const OPEN: Self = Self {
        zoom: 0.0,
        focus: 0.5,
        gobo: Gobo::Open,
        gobo_shake: 0.0,
        gobo_rot: 0,
        prism: Prism::Off,
        prism_rot: 0.0,
        frost: 0.0,
        ring: RingMode::Both,
    };
}

///////////////////////// STYLES /////////////////////////

/// A curated identity: movement family + optics. Selected in the style
/// browser, rerolled within `movements` by seed.
pub struct Style {
    pub name: &'static str,
    pub color: Rgbw,
    pub movements: &'static [Movement],
    pub texture: Texture,
}

impl Style {
    pub fn movement(&self, seed: usize) -> Movement {
        self.movements[seed % self.movements.len()]
    }
}

#[rustfmt::skip]
pub const STYLES: &[Style] = &[
    Style { name: "wash",   color: Rgbw::CYAN,    movements: &[Movement::Out, Movement::WaveY],                                 texture: Texture { zoom: 1.0, frost: 1.0, ..Texture::OPEN } },
    Style { name: "beams",  color: Rgbw::WHITE,   movements: &[Movement::SpreadOut, Movement::CrissCross { pitch: 0.4 }],       texture: Texture::OPEN },
    Style { name: "gobo",   color: Rgbw::ORANGE,  movements: &[Movement::WaveY, Movement::Twisting],                            texture: Texture { gobo: Gobo::Lines, gobo_rot: 200, focus: 0.4, ..Texture::OPEN } },
    Style { name: "prism",  color: Rgbw::VIOLET,  movements: &[Movement::Whirl, Movement::Spinner],                             texture: Texture { prism: Prism::Linear, prism_rot: 0.1, ..Texture::OPEN } },
    Style { name: "sniper", color: Rgbw::RED,     movements: &[Movement::Out, Movement::SnapX],                                 texture: Texture { ring: RingMode::Center, ..Texture::OPEN } },
    Style { name: "pixel",  color: Rgbw::MAGENTA, movements: &[Movement::Square, Movement::SnapY],                              texture: Texture { ring: RingMode::Ring, ..Texture::OPEN } },
    Style { name: "riser",  color: Rgbw::MINT,    movements: &[Movement::RaisingBeams],                                         texture: Texture::OPEN },
    Style { name: "chaos",  color: Rgbw::PINK,    movements: &[Movement::Twisting, Movement::DarthMaul],                        texture: Texture { prism: Prism::Linear, ..Texture::OPEN } },
    Style { name: "keys",   color: Rgbw::PEA,     movements: &[Movement::Keyed(&FAN), Movement::Keyed(&BOX), Movement::Keyed(&PULSE)], texture: Texture::OPEN },
];

///////////////////////// MOVEMENT /////////////////////////

#[derive(Clone, Copy, Debug)]
#[allow(unused)]
pub enum Movement {
    /// Parked where the fixture rests.
    Home,
    /// Keyframed over the beat grid, relative to the rest aim.
    Keyed(&'static Track),
    Down,
    Out,
    Center,
    SpreadOut,
    SpreadIn,
    Cross {
        pitch: f32,
        angle: Option<f32>,
        fanning: Option<f32>,
    },
    CrissCross {
        pitch: f32,
    },
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
    /// Aim at a point in the room, using the fixture's place in the model.
    LookAt(Vec3),
    /// `LookAt`, with the target sliding back and forth over `delta`.
    LookAtSway {
        pd: Pd,
        target: Vec3,
        delta: Vec3,
    },
}

/// Total pan and tilt travel in degrees, which is what a normalized 0..1
/// pan/tilt spans. Both movers are 540/270.
pub const PAN_RANGE: f32 = 540.0;
pub const TILT_RANGE: f32 = 270.0;

/// A normalized 0..1 pan or tilt as degrees off the middle of its travel.
pub fn degrees(value: f32, travel: f32) -> f32 {
    (value - 0.5) * travel
}

/// Degrees off the middle of the travel back to the normalized 0..1 the
/// fixture is driven with.
pub fn normalized(degrees: f32, travel: f32) -> f32 {
    (0.5 + degrees / travel).clamp(0.0, 1.0)
}

///////////////////////// KEYFRAMES /////////////////////////

/// An aim relative to where the fixture rests, in degrees: pitch tips the beam
/// off the rest aim, yaw swings it round.
#[derive(Clone, Copy, Debug)]
pub struct Pose {
    pub pitch: f32,
    pub yaw: f32,
}

impl Pose {
    pub const HOME: Self = Self { pitch: 0.0, yaw: 0.0 };

    fn lerp(self, to: Self, fr: f32) -> Self {
        Self { pitch: fr.lerp(self.pitch..to.pitch), yaw: fr.lerp(self.yaw..to.yaw) }
    }

    /// The normalized pan and tilt that put mover `slot`, resting at `home`,
    /// here.
    ///
    /// A flipped axle is turned round whole, aim and all, so a pair either side
    /// of centre stage works as a mirror image. `mirrored` is what a track can
    /// drop to stay uniform: the heads still rest where their flips put them,
    /// but they all work the same way from there rather than against each other.
    fn aim(self, home: &Rest, slot: usize, mirrored: bool) -> (f32, f32) {
        let travel = travel(slot);
        let (rest_pitch, rest_yaw) = home.aim();
        let (pitch, yaw) = match mirrored {
            true => home.sign(),
            false => (1.0, 1.0),
        };
        (
            normalized(rest_pitch + pitch * self.pitch, travel[1]),
            normalized(rest_yaw + yaw * self.yaw, travel[0]),
        )
    }
}

/// Shapes the travel between two keys.
pub type Curve = fn(f32) -> f32;

/// Constant speed from one key to the next.
pub fn linear(fr: f32) -> f32 {
    fr
}
/// Eased at both ends, so the head sets off and arrives gently.
pub fn smooth(fr: f32) -> f32 {
    fr.inout_quad()
}
/// Sits still, then goes: a key hit rather than a sweep.
pub fn snap(fr: f32) -> f32 {
    fr.in_quartic()
}

/// A beam pattern written as poses on the beat grid. One pass through the keys
/// takes `pd` beats; each key says how far into that pass it lands, `0..1`, and
/// the beam travels from one to the next, wrapping from the last back to the
/// first.
#[derive(Clone, Copy, Debug)]
pub struct Track {
    pub pd: Pd,
    /// Keys in ascending order of when they land.
    pub keys: &'static [(f32, Pose)],
    /// How far along the track each successive fixture is pushed, which is what
    /// fans a pattern out across the rig. 0 moves them as one.
    pub stagger: f32,
    pub curve: Curve,
    /// Whether the fixtures' axle flips apply. Mirrored, a symmetric pair works
    /// away from each other; uniform, every head sweeps the same way whatever
    /// it is flipped to.
    pub mirrored: bool,
}

impl Track {
    /// A single pass every four beats, all fixtures together, eased, mirrored.
    pub const BASE: Self =
        Self { pd: Pd(4, 1), keys: &[], stagger: 0.0, curve: smooth, mirrored: true };

    /// Where fixture `i` is on the track right now.
    fn pose(&self, s: &State, i: usize) -> Pose {
        self.at((s.pd(self.pd) + i as f32 * self.stagger).fmod(1.0))
    }

    /// The pose `fr` of the way through one pass.
    fn at(&self, fr: f32) -> Pose {
        let keys = self.keys;
        match keys.len() {
            0 => return Pose::HOME,
            1 => return keys[0].1,
            _ => {}
        }
        // The key the pass is heading for, and the one it left behind.
        let next = keys.iter().position(|(at, _)| *at > fr).unwrap_or(keys.len());
        let (from_at, from) = keys[(next + keys.len() - 1) % keys.len()];
        let (to_at, to) = keys[next % keys.len()];
        // The last leg runs off the end of the pass and back to its start.
        let leg = (to_at - from_at).fmod(1.0);
        match leg > 0.0 {
            true => from.lerp(to, (self.curve)((fr - from_at).fmod(1.0) / leg)),
            false => from,
        }
    }
}

/// Beams leaning out to one side and back, a fixture at a time.
static FAN: Track = Track {
    pd: Pd(8, 1),
    keys: &[(0.0, Pose { pitch: 25.0, yaw: -45.0 }), (0.5, Pose { pitch: 55.0, yaw: 45.0 })],
    stagger: 0.25,
    ..Track::BASE
};

/// A box drawn on the beat, held at each corner.
static BOX: Track = Track {
    pd: Pd(4, 1),
    keys: &[
        (0.0, Pose { pitch: 30.0, yaw: -25.0 }),
        (0.25, Pose { pitch: 30.0, yaw: 25.0 }),
        (0.5, Pose { pitch: 65.0, yaw: 25.0 }),
        (0.75, Pose { pitch: 65.0, yaw: -25.0 }),
    ],
    curve: snap,
    ..Track::BASE
};

/// Up on the downbeat, back down over the bar. Uniform: it is a wave down the
/// rig, so the heads have to lean the same way whatever they are flipped to.
static PULSE: Track = Track {
    pd: Pd(2, 1),
    keys: &[(0.0, Pose::HOME), (0.15, Pose { pitch: 70.0, yaw: 0.0 })],
    stagger: 0.125,
    curve: linear,
    mirrored: false,
    ..Track::BASE
};

/// A quarter turn of pan, as a fraction of the hydro's travel.
const HYDRO_YAW: f32 = 90.0 / PAN_RANGE;

/// Put a raw mslive aim on this rig: the outcasts hang the other way up, and
/// the hydros sit a quarter turn round from what the patterns were written for.
fn rigged(slot: usize, (pitch, yaw): (f32, f32)) -> (f32, f32) {
    match OUTCASTS.contains(&slot) {
        true => (1.0 - pitch, yaw),
        false => (pitch, (yaw + HYDRO_YAW).fmod(1.0)),
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
    /// Calculate (pitch, yaw) for the given pattern. `world` is where the
    /// fixture sits in the model, which only the LookAt patterns use, and
    /// `home` where it rests, which the keyed patterns are written against.
    pub fn angles(
        self,
        s: &State,
        pd: Pd,
        i: usize,
        fr: f32,
        world: &Transform,
        home: &Rest,
    ) -> (f32, f32) {
        match self {
            Movement::Home => Pose::HOME.aim(home, i, false),
            Movement::Keyed(track) => track.pose(s, i).aim(home, i, track.mirrored),
            Movement::LookAt(target) => Self::look_at_angles(target - world.translation, world.rotation),
            Movement::LookAtSway { pd, target, delta } => {
                let target = target + delta * s.pd(pd).fsin(1.0);
                Movement::LookAt(target).angles(s, pd, i, fr, world, home)
            }
            _ => rigged(i, self.raw(s, pd, i, fr)),
        }
    }

    /// The mslive patterns, in the raw pan and tilt they were written in.
    fn raw(self, s: &State, pd: Pd, i: usize, fr: f32) -> (f32, f32) {
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
            _ => (0.5, 0.5),
        }
    }

    /// Normalized pan/tilt that puts the beam on `dir`, a world-space direction
    /// from a fixture whose frame is `rotation`.
    ///
    /// A mover's head at home points down the fixture's own -y, tilt swings it
    /// about local x and pan carries that round local y, so the beam ends up at
    /// `Ry(pan) * Rx(tilt) * -y`. Inverting that: tilt is what opens the beam
    /// away from home, pan is where it opens toward. Tilt comes out positive,
    /// so pan covers the other half of the sphere.
    fn look_at_angles(dir: Vec3, rotation: Quat) -> (f32, f32) {
        let u = (rotation.inverse() * dir).normalize_or_zero();
        let tilt = (-u.y).clamp(-1.0, 1.0).acos().to_degrees();
        let pan = f32::atan2(-u.x, -u.z).to_degrees();
        ((0.5 + tilt / TILT_RANGE).clamp(0.0, 1.0), (0.5 + pan / PAN_RANGE).clamp(0.0, 1.0))
    }

    /// Per-fixture brightness mask for patterns that hide the reset sweep.
    pub fn mask(self, s: &State, pd: Pd, _i: usize, fr: f32) -> f32 {
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
