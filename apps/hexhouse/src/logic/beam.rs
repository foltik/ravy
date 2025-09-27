use lib::lights::fixture::StealthBeam;
use lib::prelude::*;

use crate::logic::State;

#[derive(Clone, Copy, Debug)]
#[allow(unused)]
pub enum BeamPattern {
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
    UpDownWave,
}

impl BeamPattern {
    pub fn apply(self, s: &State, pd: Pd, beam: &mut StealthBeam, i: usize, fr: f32) {
        let (pitch, yaw) = self.angles(s, pd, i, fr);
        beam.pitch = pitch;
        beam.yaw = yaw;
    }

    pub fn values(self, s: &State, pd: Pd, i: usize, fr: f32) -> (f32, f32, f32) {
        let (pitch, yaw) = self.angles(s, pd, i, fr);
        (1.0, pitch, yaw)
    }

    /// Calculate (pitch, yaw) for the given pattern
    pub fn angles(self, s: &State, pd: Pd, i: usize, fr: f32) -> (f32, f32) {
        match self {
            BeamPattern::Down => (0.0, 0.0),
            BeamPattern::Out => (0.5, 0.0),
            BeamPattern::Center => (
                0.85,
                match i {
                    0 => 0.05,
                    1 => 0.7,
                    2 => 0.63,
                    _ => 0.6,
                },
            ),
            BeamPattern::SpreadOut => (
                0.0,
                match i {
                    0 => 0.5 - 0.05,
                    1 => 0.5 - 0.02,
                    2 => 0.5 + 0.02,
                    _ => 0.5 + 0.05,
                } - (0.25 / 1.5),
            ),
            BeamPattern::SpreadIn => (
                0.0,
                match i {
                    0 => 0.5 + 0.09,
                    1 => 0.5 + 0.07,
                    2 => 0.5 - 0.07,
                    _ => 0.5 - 0.09,
                } - (0.25 / 1.5),
            ),
            BeamPattern::Cross { pitch, angle, fanning } => {
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
            BeamPattern::CrissCross { pitch } => (
                pitch,
                match i {
                    0 => 0.5 + 0.08,
                    1 => 0.5 - 0.05,
                    2 => 0.5 + 0.05,
                    _ => 0.5 - 0.08,
                } - (0.25 / 1.5),
            ),
            BeamPattern::SnapY => {
                let t = s.pd(pd.mul(4)).square(1.0, 0.5);
                let pitch = 0.3
                    * match i % 2 == 0 {
                        true => t,
                        false => 1.0 - t,
                    };
                (pitch, 0.5)
            }
            BeamPattern::SnapX => {
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
            BeamPattern::WaveY => {
                let t = s.pd(pd.mul(4)).tri(1.0);
                let pitch = 0.4
                    * match i % 2 == 0 {
                        _ => t,
                        // true => t,
                        // false => 1.0 - t,
                    };
                (1.0 - pitch, 0.0)
            }
            BeamPattern::Square => {
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
            BeamPattern::Whirl => {
                use super::preset::WhirlState;
                let angle = (s.pd(pd) + fr * 1.5) % 1.0;
                match WhirlState::from_angle(angle) {
                    WhirlState::FullyResetting { pitch, yaw } => (pitch, yaw),
                    WhirlState::DoingSubrotation { pitch, yaw, .. } => (pitch, yaw),
                }
            }
            BeamPattern::RaisingBeams => {
                let angle = (s.pd(pd.mul(2)) + fr * 2.0) % 1.0;
                // let
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
            BeamPattern::Twisting => {
                // angle
                // rand::thread_rng().s
                // rand::Rng::Ch.from_seed(10);
                // (0.5)
                use rand::prelude::*;
                let seed = (s.pd(pd.mul(512)) * 255.) as u8;
                let mut seed_array = [seed; 32];
                seed_array[0] = i as u8;
                let mut rng = rand::prelude::StdRng::from_seed(seed_array);
                let yaw = rng.sample(rand::distributions::Uniform::new(0.0, 1.0));
                let pitch = rng.sample(rand::distributions::Uniform::new(0.2, 0.8));
                (pitch, yaw)
                // (0.0, s.tri(pd))
            }
            BeamPattern::DarthMaul => (
                0.2,
                match i {
                    _ if i % 2 == 0 => s.pd(pd.mul(8)).tri(1.0).lerp(0.2..0.8),
                    _ if i % 2 == 1 => s.pd(pd.mul(8)).tri(1.0).lerp(0.2..0.8) + 0.66,
                    _ => 0.0,
                },
            ),
            BeamPattern::UpDownWave => (0.2, s.pd(Pd(8, 1)).phase(1.0, fr * 0.1).square(1.0, 0.5)),
        }
    }
}
