use lib::prelude::*;

use super::logic::State;

///////////////////////// LOOK /////////////////////////

/// A partial look: `None` = inherit from the layer below.
#[derive(Clone, Copy, Debug, Default)]
pub struct Look {
    pub energy: Option<Energy>,
    pub movement: Option<Movement>,
    pub texture: Option<Texture>,
    pub color: Option<Rgbw>,
}

impl Look {
    pub const NONE: Self = Self { energy: None, movement: None, texture: None, color: None };
}

/// A fully resolved look, ready to render.
pub struct Looked {
    pub energy: Energy,
    pub movement: Movement,
    pub texture: Texture,
    pub beam: Rgbw,
    pub par: Rgbw,
}

///////////////////////// ENERGY /////////////////////////

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Energy {
    Off,
    On,
    Beat { pd: Pd },
    Strobe { pd: Pd, duty: f32 },
    Chase { pd: Pd },
}

impl Energy {
    pub fn pd(&self) -> Option<Pd> {
        match self {
            Energy::Off | Energy::On => None,
            Energy::Beat { pd } | Energy::Strobe { pd, .. } | Energy::Chase { pd } => Some(*pd),
        }
    }

    /// Brightness envelope, with `fr` staggering chases across fixtures.
    pub fn env(&self, s: &State, fr: f32) -> f32 {
        match self {
            Energy::Off => 0.0,
            Energy::On => 1.0,
            Energy::Beat { pd } => s.pd(*pd).ramp(1.0).inv().lerp(0.2..1.0).in_quad(),
            Energy::Strobe { pd, duty } => {
                s.pd(pd.mul(2)).phase(1.0, fr).square(1.0, duty.in_exp().lerp(1.0..0.5))
            }
            Energy::Chase { pd } => s.pd(pd.mul(4)).phase(1.0, fr).square(1.0, 0.1),
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

/// Slow optics: wheels, prism, frost, zoom, focus.
// TODO: measure the Hydro wheel/gobo/prism slot bytes on the rig.
#[derive(Clone, Copy, Debug)]
pub struct Texture {
    pub zoom: f32,
    pub focus: f32,
    pub gobo: u8,
    pub gobo_rot: u8,
    pub prism: u8,
    pub prism_rot: u8,
    pub frost: f32,
    pub ring: RingMode,
}

impl Texture {
    pub const OPEN: Self = Self {
        zoom: 0.5,
        focus: 0.5,
        gobo: 0,
        gobo_rot: 0,
        prism: 0,
        prism_rot: 0,
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
    Style { name: "beams",  color: Rgbw::WHITE,   movements: &[Movement::SpreadOut, Movement::CrissCross { pitch: 0.4 }],       texture: Texture { zoom: 0.0, ..Texture::OPEN } },
    Style { name: "gobo",   color: Rgbw::ORANGE,  movements: &[Movement::WaveY, Movement::Twisting],                            texture: Texture { gobo: 40, gobo_rot: 200, focus: 0.4, ..Texture::OPEN } },
    Style { name: "prism",  color: Rgbw::VIOLET,  movements: &[Movement::Whirl, Movement::Spinner],                             texture: Texture { prism: 30, prism_rot: 200, ..Texture::OPEN } },
    Style { name: "sniper", color: Rgbw::RED,     movements: &[Movement::Out, Movement::SnapX],                                 texture: Texture { zoom: 0.0, ring: RingMode::Center, ..Texture::OPEN } },
    Style { name: "pixel",  color: Rgbw::MAGENTA, movements: &[Movement::Square, Movement::SnapY],                              texture: Texture { ring: RingMode::Ring, ..Texture::OPEN } },
    Style { name: "riser",  color: Rgbw::MINT,    movements: &[Movement::RaisingBeams],                                         texture: Texture::OPEN },
    Style { name: "chaos",  color: Rgbw::PINK,    movements: &[Movement::Twisting, Movement::DarthMaul],                        texture: Texture { prism: 30, ..Texture::OPEN } },
];

///////////////////////// MOVEMENT /////////////////////////

#[derive(Clone, Copy, Debug)]
#[allow(unused)]
pub enum Movement {
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
const PAN_RANGE: f32 = 540.0;
const TILT_RANGE: f32 = 270.0;

impl Movement {
    /// Calculate (pitch, yaw) for the given pattern. `world` is where the
    /// fixture sits in the model, which only the LookAt patterns use.
    pub fn angles(self, s: &State, pd: Pd, i: usize, fr: f32, world: &Transform) -> (f32, f32) {
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
            Movement::Cross { pitch, angle, fanning } => {
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
            Movement::CrissCross { pitch } => (
                pitch,
                match i {
                    0 => 0.5 + 0.08,
                    1 => 0.5 - 0.05,
                    2 => 0.5 + 0.05,
                    _ => 0.5 - 0.08,
                } - (0.25 / 1.5),
            ),
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
            Movement::LookAt(target) => Self::look_at_angles(target - world.translation, world.rotation),
            Movement::LookAtSway { pd, target, delta } => {
                let target = target + delta * s.pd(pd).fsin(1.0);
                Movement::LookAt(target).angles(s, pd, i, fr, world)
            }
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
