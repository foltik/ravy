use lib::prelude::*;

use crate::logic::{BeamPattern, PadPattern, State};

pub trait Preset: DynClone + Send + Sync + 'static {
    /// Color displayed on the launchpad button mapped to this preset.
    fn pad_color(&self, s: &State) -> Rgbw {
        self.palette_color0(s)
    }
    /// Brightness of the launchpad button mapped to this preset.
    fn pad_env(&self, s: &State) -> f32 {
        match self.visuals_pd() {
            Some(pd) => s.pd(pd).ramp(1.0).inv().in_quad(),
            None => 1.0,
        }
    }
    /// Visualizer pattern displayed on the launchpad when this preset is active.
    fn pad_pattern(&self) -> PadPattern {
        PadPattern::Random
    }

    /// Baseline palette0 color.
    fn palette_color0(&self, s: &State) -> Rgbw {
        s.palette.color0(s)
    }
    /// Baseline palette1 color.
    fn palette_color1(&self, s: &State) -> Rgbw {
        s.palette.color1(s)
    }
    /// Baseline gradient.
    fn palette_gradient(&self, s: &State) -> RgbwGradient {
        s.palette.gradient(s)
    }

    /// Beam shaper returning (brightness, pitch, yaw)
    fn light_beams(&self, _s: &State, _i: usize, _fr: f32) -> (f32, f32, f32) {
        (1.0, 0.0, 0.0)
    }
    /// Spotlight shaper returning brightness.
    fn light_spots(&self, _s: &State, _i: usize, _fr: f32) -> f32 {
        1.0
    }

    /// Scalar mask for external visuals.
    fn visuals_mask(&self, _s: &State) -> f32 {
        1.0
    }
    /// Beat period for external visuals.
    fn visuals_pd(&self) -> Option<Pd> {
        None
    }
}
clone_trait_object!(Preset);

///////////////////////// On/Off /////////////////////////

#[derive(Clone)]
pub struct Off;
impl Preset for Off {
    /// Solid black pad.
    fn pad_color(&self, _: &State) -> Rgbw {
        Rgbw::BLACK
    }
    /// Pad visual off.
    fn pad_pattern(&self) -> PadPattern {
        PadPattern::Off
    }
    fn visuals_mask(&self, _s: &State) -> f32 {
        0.0
    }
}

#[derive(Clone)]
pub struct On {
    pub beams: Option<BeamPattern>,
}
impl Preset for On {
    /// Solid palette0.
    fn pad_color(&self, s: &State) -> Rgbw {
        self.palette_color0(s)
    }
    /// Spiral visual.
    fn pad_pattern(&self) -> PadPattern {
        PadPattern::Spiral
    }
    /// Beam aim only (no brightness env).
    fn light_beams(&self, s: &State, i: usize, fr: f32) -> (f32, f32, f32) {
        if let Some(p) = self.beams {
            let (pitch, yaw) = p.angles(s, Pd(4, 1), i, fr);
            (1.0, pitch, yaw)
        } else {
            (1.0, 0.0, 0.0)
        }
    }
}

///////////////////////// Break /////////////////////////

#[derive(Clone)]
pub struct Break {
    pub beams: Option<BeamPattern>,
}
impl Preset for Break {
    /// Solid palette0.
    fn pad_color(&self, s: &State) -> Rgbw {
        self.palette_color0(s)
    }
    /// Default random visual.
    fn pad_pattern(&self) -> PadPattern {
        PadPattern::Random
    }
    /// Beam aim only.
    fn light_beams(&self, s: &State, i: usize, fr: f32) -> (f32, f32, f32) {
        if let Some(p) = self.beams {
            let (pitch, yaw) = p.angles(s, Pd(4, 1), i, fr);
            (1.0, pitch, yaw)
        } else {
            (1.0, 0.0, 0.0)
        }
    }
}

///////////////////////// AutoBeat /////////////////////////

#[derive(Clone)]
pub struct AutoBeat {
    pub pd: Pd,
    pub r: Range,
    pub beam: BeamPattern,
}
impl Preset for AutoBeat {
    /// Driven by beat env.
    fn pad_env(&self, s: &State) -> f32 {
        s.pd(self.pd).ramp(1.0).inv().lerp(self.r).in_quad()
    }
    /// Default random visual (your Pulse logic lives inside PadPattern).
    fn pad_pattern(&self) -> PadPattern {
        PadPattern::Random
    }
    /// Beam aim + beat brightness.
    fn light_beams(&self, s: &State, i: usize, fr: f32) -> (f32, f32, f32) {
        let env = self.pad_env(s);
        let (pitch, yaw) = self.beam.angles(s, self.pd, i, fr);
        // Whirl keeps its original gating curve.
        if let BeamPattern::Whirl = self.beam {
            let angle = (s.pd(self.pd) + fr * 1.5) % 1.0;
            let warmup = 0.1;
            let whirl_env = match super::preset::WhirlState::from_angle(angle) {
                super::preset::WhirlState::FullyResetting { .. } => 0.0,
                super::preset::WhirlState::DoingSubrotation { percentage, .. } => {
                    if percentage < warmup {
                        0.0
                    } else {
                        ((percentage - warmup) / (1.0 - warmup)).trapazoid(1.0, 1.0 / 16.0).powf(2.0)
                    }
                }
            };
            (whirl_env, pitch, yaw)
        } else {
            (env, pitch, yaw)
        }
    }
    /// Expose beat period to visuals.
    fn visuals_pd(&self) -> Option<Pd> {
        Some(self.pd)
    }
}

///////////////////////// Strobes /////////////////////////

#[derive(Clone)]
pub struct Strobe {
    pub pd: Pd,
    pub duty: f32,
}
impl Preset for Strobe {
    /// Pad flashes by strobe env.
    fn pad_env(&self, s: &State) -> f32 {
        s.pd(self.pd.mul(2)).square(1.0, self.duty.in_exp().lerp(1.0..0.5))
    }
    /// Strobe visual.
    fn pad_pattern(&self) -> PadPattern {
        PadPattern::Strobe
    }
    fn visuals_mask(&self, s: &State) -> f32 {
        s.pd(self.pd.mul(2)).square(1.0, self.duty.in_exp().lerp(1.0..0.5))
    }
    /// Expose beat period.
    fn visuals_pd(&self) -> Option<Pd> {
        Some(self.pd)
    }
    /// Spots flash hard; beams square + Square aim.
    fn light_spots(&self, s: &State, _i: usize, fr: f32) -> f32 {
        s.pd(self.pd.mul(2))
            .phase(1.0, fr)
            .square(1.0, self.duty.in_exp().lerp(1.0..0.5))
    }
    fn light_beams(&self, s: &State, i: usize, fr: f32) -> (f32, f32, f32) {
        let env = self.light_spots(s, i, fr);
        let (pitch, yaw) = BeamPattern::Square.angles(s, Pd(2, 1), i, fr);
        (env, pitch, yaw)
    }
}

#[derive(Clone)]
pub struct Strobe0 {
    pub pd: Pd,
    pub duty: f32,
}
impl Preset for Strobe0 {
    /// Pad flashes by strobe env.
    fn pad_env(&self, s: &State) -> f32 {
        s.pd(self.pd.mul(2)).square(1.0, self.duty.in_exp().lerp(1.0..0.5))
    }
    /// Strobe visual.
    fn pad_pattern(&self) -> PadPattern {
        PadPattern::Strobe
    }
    /// Left bank color0 / right black.
    fn palette_color1(&self, _s: &State) -> Rgbw {
        Rgbw::BLACK
    }
    /// Strobe hint.
    fn visuals_mask(&self, s: &State) -> f32 {
        s.pd(self.pd.mul(2)).square(1.0, self.duty.in_exp().lerp(1.0..0.5))
    }
    /// Expose beat period.
    fn visuals_pd(&self) -> Option<Pd> {
        Some(self.pd)
    }
    /// Spots/Beams same as Strobe.
    fn light_spots(&self, s: &State, _i: usize, fr: f32) -> f32 {
        s.pd(self.pd.mul(2))
            .phase(1.0, fr)
            .square(1.0, self.duty.in_exp().lerp(1.0..0.5))
    }
    fn light_beams(&self, s: &State, i: usize, fr: f32) -> (f32, f32, f32) {
        let env = self.light_spots(s, i, fr);
        let (pitch, yaw) = BeamPattern::Square.angles(s, Pd(2, 1), i, fr);
        (env, pitch, yaw)
    }
}

#[derive(Clone)]
pub struct Strobe1 {
    pub pd: Pd,
    pub duty: f32,
}
impl Preset for Strobe1 {
    /// Pad flashes by strobe env.
    fn pad_env(&self, s: &State) -> f32 {
        s.pd(self.pd.mul(2)).square(1.0, self.duty.in_exp().lerp(1.0..0.5))
    }
    /// Strobe visual.
    fn pad_pattern(&self) -> PadPattern {
        PadPattern::Strobe
    }
    /// Left bank black / right color0.
    fn palette_color0(&self, _s: &State) -> Rgbw {
        Rgbw::BLACK
    }
    fn visuals_mask(&self, s: &State) -> f32 {
        s.pd(self.pd.mul(2)).square(1.0, self.duty.in_exp().lerp(1.0..0.5))
    }
    /// Expose beat period.
    fn visuals_pd(&self) -> Option<Pd> {
        Some(self.pd)
    }
    /// Spots/Beams same as Strobe.
    fn light_spots(&self, s: &State, _i: usize, fr: f32) -> f32 {
        s.pd(self.pd.mul(2))
            .phase(1.0, fr)
            .square(1.0, self.duty.in_exp().lerp(1.0..0.5))
    }
    fn light_beams(&self, s: &State, i: usize, fr: f32) -> (f32, f32, f32) {
        let env = self.light_spots(s, i, fr);
        let (pitch, yaw) = BeamPattern::Square.angles(s, Pd(2, 1), i, fr);
        (env, pitch, yaw)
    }
}

///////////////////////// Whirl /////////////////////////

#[derive(Clone)]
pub struct Whirl {
    pub pd: Pd,
}
impl Preset for Whirl {
    /// Solid palette0.
    fn pad_color(&self, s: &State) -> Rgbw {
        self.palette_color0(s)
    }
    /// Spiral visual.
    fn pad_pattern(&self) -> PadPattern {
        PadPattern::Spiral
    }
    /// Whirl gating env per beam.
    fn light_beams(&self, s: &State, i: usize, fr: f32) -> (f32, f32, f32) {
        let angle = (s.pd(self.pd) + fr * 1.5) % 1.0;
        let warmup = 0.1;
        let env = match WhirlState::from_angle(angle) {
            WhirlState::FullyResetting { .. } => 0.0,
            WhirlState::DoingSubrotation { percentage, .. } => {
                if percentage < warmup {
                    0.0
                } else {
                    ((percentage - warmup) / (1.0 - warmup)).trapazoid(1.0, 1.0 / 16.0).powf(2.0)
                }
            }
        };
        let (pitch, yaw) = BeamPattern::Whirl.angles(s, self.pd, i, fr);
        (env, pitch, yaw)
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

///////////////////////// Chases /////////////////////////

#[derive(Clone)]
pub struct Chase {
    pub pd: Pd,
    pub beam: BeamPattern,
}
impl Preset for Chase {
    /// White pad, modulated by chase gate for visual feedback.
    fn pad_color(&self, _: &State) -> Rgbw {
        Rgbw::WHITE
    }
    fn pad_env(&self, s: &State) -> f32 {
        s.pd(self.pd).square(1.0, 0.6)
    }
    /// Horizontal wave visual.
    fn pad_pattern(&self) -> PadPattern {
        PadPattern::WaveX
    }
    /// Chases are white.
    fn palette_color0(&self, _: &State) -> Rgbw {
        Rgbw::WHITE
    }
    fn palette_color1(&self, _: &State) -> Rgbw {
        Rgbw::WHITE
    }
    /// Spot/beam gates.
    fn light_spots(&self, s: &State, _i: usize, fr: f32) -> f32 {
        s.pd(self.pd.mul(4)).phase(1.0, fr).square(1.0, 0.1)
    }
    fn light_beams(&self, s: &State, i: usize, fr: f32) -> (f32, f32, f32) {
        let env = self.light_spots(s, i, fr);
        let (pitch, yaw) = self.beam.angles(s, Pd(1, 2), i, fr);
        (env, pitch, yaw)
    }
}

#[derive(Clone)]
pub struct ChaseSmooth {
    pub pd: Pd,
    pub beam: BeamPattern,
}
impl Preset for ChaseSmooth {
    /// Palette-based color with smooth gate.
    fn pad_env(&self, s: &State) -> f32 {
        s.pd(self.pd.mul(4)).tri(1.0)
    }
    /// Horizontal wave visual.
    fn pad_pattern(&self) -> PadPattern {
        PadPattern::WaveX
    }
    /// Chases are single-color.
    fn palette_color0(&self, s: &State) -> Rgbw {
        s.palette.color0(s)
    }
    fn palette_color1(&self, s: &State) -> Rgbw {
        s.palette.color0(s)
    }
    /// Spot/beam env.
    fn light_spots(&self, s: &State, _i: usize, fr: f32) -> f32 {
        s.pd(self.pd.mul(4)).phase(1.0, fr).tri(1.0)
    }
    fn light_beams(&self, s: &State, i: usize, fr: f32) -> (f32, f32, f32) {
        let env = self.light_spots(s, i, fr);
        let (pitch, yaw) = self.beam.angles(s, Pd(4, 1), i, fr);
        (env, pitch, yaw)
    }
}

#[derive(Clone)]
pub struct ChaseNotColorful {
    pub pd: Pd,
}
impl Preset for ChaseNotColorful {
    /// Palette color0 with square gate.
    fn pad_env(&self, s: &State) -> f32 {
        s.pd(self.pd).square(1.0, 0.33)
    }
    /// Horizontal wave visual.
    fn pad_pattern(&self) -> PadPattern {
        PadPattern::WaveX
    }
    /// Single palette color.
    fn palette_color1(&self, s: &State) -> Rgbw {
        s.palette.color0(s)
    }
    /// Beam env with animated cross aim.
    fn light_beams(&self, s: &State, i: usize, fr: f32) -> (f32, f32, f32) {
        let offset = if i < 2 { 0.0 } else { 0.5 };
        let env = s.pd(self.pd).phase(1.0, offset).square(1.0, 0.33);
        let (pitch, yaw) = BeamPattern::Cross {
            pitch: (1. - s.pd(self.pd.mul(8)).fsin(1.)) * 0.3 + 0.1,
            angle: Some(s.pd(self.pd.mul(8)).fsin(1.) * 0.2 - 0.1),
            fanning: Some(1.5),
        }
        .angles(s, self.pd, i, fr);
        (env, pitch, yaw)
    }
}

///////////////////////// RaisingBeams /////////////////////////

#[derive(Clone)]
pub struct RaisingBeams {
    pub pd: Pd,
}
impl Preset for RaisingBeams {
    /// Palette color0 with raising envelope.
    fn pad_env(&self, s: &State) -> f32 {
        1.0 - s.phi(4, 1).ramp(1.0).out_exp()
    }
    /// Vertical wave visual.
    fn pad_pattern(&self) -> PadPattern {
        PadPattern::WaveY
    }
    /// Beam env shaped by angle window.
    fn light_beams(&self, s: &State, i: usize, fr: f32) -> (f32, f32, f32) {
        let angle = (s.pd(self.pd) + fr * 2.0) % 1.0;
        let env = if angle < 0.45 { (angle - 0.1).trapazoid(0.5, 0.1) } else { 0.0 };
        let (pitch, yaw) = BeamPattern::RaisingBeams.angles(s, self.pd, i, fr);
        (env, pitch, yaw)
    }
}
