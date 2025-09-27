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
    Spinner,

    LookAt(Vec3),
    LookAtSway {
        pd: Pd,
        target_pos: Vec3,
        delta: Vec3,
    },
}

impl BeamPattern {
    pub fn apply(self, s: &State, pd: Pd, beam: &mut StealthBeam, i: usize, fr: f32, transform: &Transform) {
        let (pitch, yaw) = self.angles(s, pd, i, fr, transform);
        beam.pitch = pitch;
        beam.yaw = yaw;
    }

    pub fn values(self, s: &State, pd: Pd, i: usize, fr: f32, transform: &Transform) -> (f32, f32, f32) {
        let (pitch, yaw) = self.angles(s, pd, i, fr, transform);
        (1.0, pitch, yaw)
    }

    /// Calculate (pitch, yaw) for the given pattern
    pub fn angles(self, s: &State, pd: Pd, i: usize, fr: f32, transform: &Transform) -> (f32, f32) {
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
                let pitch = 0.15 + 0.40 * t;
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
            BeamPattern::Spinner => (0.3, s.pd(Pd(8, 1)).phase(1.0, fr * 0.1).square(1.0, 0.5)),
            BeamPattern::LookAt(target_pos) => {
                let fixture_pos = transform.translation;
                let fixture_rot = transform.rotation;

                // Direction to target in world space
                let world_dir = (target_pos - fixture_pos).normalize();
                // Transform to local space (where +X is forward)
                let local_dir = fixture_rot.inverse() * world_dir;

                // For beam pointing at +X locally:
                // After Ry(yaw) * Rz(pitch), +X becomes:
                // x = cos(yaw) * cos(pitch)
                // y = sin(pitch)
                // z = sin(yaw) * cos(pitch)

                // Solve for angles:
                let mut pitch_deg = local_dir.y.asin().clamp(-PI / 2.0, PI / 2.0).to_degrees();
                let mut yaw_deg = (-local_dir.z).atan2(local_dir.x).to_degrees();

                // Map pitch from [-180, 180] to [0, 180]
                if pitch_deg < 0.0 {
                    pitch_deg = -pitch_deg; // Negate pitch
                    yaw_deg += 180.0; // Flip yaw to compensate
                }
                pitch_deg = pitch_deg.clamp(0.0, 180.0);

                // Map yaw from [-180, 180] to [-540, 0]
                // First normalize to [-180, 180]
                yaw_deg = ((yaw_deg + 180.0).rem_euclid(360.0)) - 180.0;
                // Then map to [-540, 0]
                if yaw_deg > 0.0 {
                    yaw_deg -= 360.0;
                }
                yaw_deg = yaw_deg.clamp(-540.0, 0.0);

                // Normalize to [0, 1]
                let yaw_n = (-yaw_deg / 540.0).clamp(0.0, 1.0); // 0° → 1.0, -540° → 0.0
                let pitch_n = (pitch_deg / 180.0).clamp(0.0, 1.0); // 0° → 0.0, 180° → 1.0

                (pitch_n, yaw_n)
            }
            BeamPattern::LookAtSway { target_pos, delta, pd } => {
                let target_pos = target_pos + delta * s.pd(pd).fsin(1.0);
                BeamPattern::LookAt(target_pos).angles(s, pd, i, fr, transform)
            }
        }
    }
}
