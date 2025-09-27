use lib::prelude::*;

use crate::logic::{BeamPattern, PadPattern, State};

#[rustfmt::skip]
pub trait Preset: DynClone + Send + Sync + 'static {
    /// Beam color, defaults to the current palette.
    fn beam_color(&self, s: &State) -> Rgbw { s.palette.beam_color(s) }
    /// Beam brightness level.
    fn beam_brightness(&self, _s: &State, _i: usize, _fr: f32) -> f32 { 1.0 }
    /// Beam pattern to apply.
    fn beam_pattern(&self) -> BeamPattern { BeamPattern::Down }

    /// Spotlight color, defaults to the current palette.
    fn spot_color(&self, s: &State) -> Rgbw { s.palette.spot_color(s) }
    /// Spotlight brightness level.
    fn spot_brightness(&self, _s: &State, _i: usize, _fr: f32) -> f32 { 1.0 }

    /// Color displayed on the launchpad button mapped to this preset.
    fn pad_color(&self, s: &State) -> Rgbw { self.beam_color(s) }
    /// Brightness of the launchpad button mapped to this preset.
    fn pad_brightness(&self, s: &State) -> f32 { self.visuals_pd().map(|pd| s.pd(pd).ramp(1.0).inv().in_quad()).unwrap_or(1.0) }
    /// Visualizer pattern displayed on the launchpad when this preset is active.
    fn pad_pattern(&self) -> PadPattern { PadPattern::Solid }

    /// Brightness mask for external visuals.
    fn visuals_brightness(&self, _s: &State) -> f32 { 1.0 }
    /// Beat period for external visuals.
    fn visuals_pd(&self) -> Option<Pd> { None }
    /// Gradient for external visuals.
    fn visuals_gradient(&self, s: &State) -> RgbwGradient { s.palette.gradient(s) }
}
clone_trait_object!(Preset);

///////////////////////// On/Off /////////////////////////

/// All lights off.
#[derive(Clone)]
pub struct Off;
#[rustfmt::skip]
impl Preset for Off {
    fn beam_brightness(&self, _s: &State, _i: usize, _fr: f32) -> f32 { 0.0 }
    fn spot_brightness(&self, _s: &State, _i: usize, _fr: f32) -> f32 { 0.0 }
    fn pad_brightness(&self, _s: &State) -> f32 { 0.0 }
    fn visuals_brightness(&self, _s: &State) -> f32 { 0.0 }
}

/// All lights on.
#[derive(Clone)]
pub struct On {
    pub beams: BeamPattern,
}
#[rustfmt::skip]
impl Preset for On {
    fn beam_pattern(&self) -> BeamPattern { self.beams }
}

///////////////////////// Break /////////////////////////

/// Only beams on, visuals off.
#[derive(Clone)]
pub struct Break {
    pub beams: BeamPattern,
}
#[rustfmt::skip]
impl Preset for Break {
    fn spot_brightness(&self, _s: &State, _i: usize, _fr: f32) -> f32 { 0.0 }
    fn pad_brightness(&self, _s: &State) -> f32 { 0.0 }

    fn beam_pattern(&self) -> BeamPattern { self.beams }
}

///////////////////////// AutoBeat /////////////////////////

#[derive(Clone)]
pub struct AutoBeat {
    pub pd: Pd,
    pub beam: BeamPattern,
}
#[rustfmt::skip]
impl Preset for AutoBeat {
    fn beam_brightness(&self, s: &State, _i: usize, _fr: f32) -> f32 {
        self.pad_brightness(s)
    }
    fn pad_brightness(&self, s: &State) -> f32 {
        s.pd(self.pd).ramp(1.0).inv().lerp(0.2..1.0).in_quad()
    }

    fn beam_pattern(&self) -> BeamPattern { self.beam }
    fn pad_pattern(&self) -> PadPattern { PadPattern::Random }

    fn visuals_pd(&self) -> Option<Pd> { Some(self.pd) }
}

///////////////////////// Whirl /////////////////////////

#[derive(Clone)]
pub struct Whirl {
    pub pd: Pd,
}
#[rustfmt::skip]
impl Preset for Whirl {
    fn beam_brightness(&self, s: &State, _i: usize, fr: f32) -> f32 {
        let angle = (s.pd(self.pd) + fr * 1.5) % 1.0;
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
    fn spot_brightness(&self, _s: &State, _i: usize, _fr: f32) -> f32 { 0.0 }

    fn beam_pattern(&self) -> BeamPattern { BeamPattern::Whirl }
    fn pad_pattern(&self) -> PadPattern { PadPattern::Spiral }
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

///////////////////////// RaisingBeams /////////////////////////

#[derive(Clone)]
pub struct RaisingBeams {
    pub pd: Pd,
}
#[rustfmt::skip]
impl Preset for RaisingBeams {
    fn pad_brightness(&self, s: &State) -> f32 {
        1.0 - s.phi(4, 1).ramp(1.0).out_exp()
    }

    fn beam_brightness(&self, s: &State, _i: usize, fr: f32) -> f32 {
        let angle = (s.pd(self.pd) + fr * 2.0) % 1.0;
        if angle < 0.45 { (angle - 0.1).trapazoid(0.5, 0.1) } else { 0.0 }
    }
    fn spot_brightness(&self, s: &State, _i: usize, fr: f32) -> f32 {
        s.pd(self.pd.mul(4)).phase(1.0, fr).square(1.0, 0.1)
    }

    fn pad_pattern(&self) -> PadPattern { PadPattern::WaveY }
    fn beam_pattern(&self) -> BeamPattern { BeamPattern::RaisingBeams }
}

///////////////////////// Strobes /////////////////////////

#[derive(Clone)]
pub struct Strobe {
    pub pd: Pd,
    pub duty: f32,
}
#[rustfmt::skip]
impl Preset for Strobe {
    fn beam_brightness(&self, s: &State, _i: usize, fr: f32) -> f32 {
        s.pd(self.pd.mul(2))
            .phase(1.0, fr)
            .square(1.0, self.duty.in_exp().lerp(1.0..0.5))
    }
    fn spot_brightness(&self, s: &State, i: usize, fr: f32) -> f32 {
        self.beam_brightness(s, i, fr)
    }

    fn pad_brightness(&self, s: &State) -> f32 {
        s.pd(self.pd.mul(2)).square(1.0, self.duty.in_exp().lerp(1.0..0.5))
    }
    fn visuals_brightness(&self, s: &State) -> f32 {
        self.pad_brightness(s)
    }

    fn beam_pattern(&self) -> BeamPattern { BeamPattern::Square }
    fn pad_pattern(&self) -> PadPattern { PadPattern::Strobe }

    fn visuals_pd(&self) -> Option<Pd> { Some(self.pd) }
}

#[derive(Clone)]
pub struct StrobeBeams {
    pub pd: Pd,
    pub duty: f32,
}
#[rustfmt::skip]
impl Preset for StrobeBeams {
    fn beam_brightness(&self, s: &State, _i: usize, fr: f32) -> f32 {
        s.pd(self.pd.mul(2))
            .phase(1.0, fr)
            .square(1.0, self.duty.in_exp().lerp(1.0..0.5))
    }
    fn spot_brightness(&self, _s: &State, _i: usize, _fr: f32) -> f32 {
        0.0
    }

    fn pad_brightness(&self, s: &State) -> f32 {
        s.pd(self.pd.mul(2)).square(1.0, self.duty.in_exp().lerp(1.0..0.5))
    }
    fn visuals_brightness(&self, s: &State) -> f32 {
        self.pad_brightness(s)
    }

    fn beam_pattern(&self) -> BeamPattern { BeamPattern::Square }
    fn pad_pattern(&self) -> PadPattern { PadPattern::Strobe }

    fn visuals_pd(&self) -> Option<Pd> { Some(self.pd) }
}

#[derive(Clone)]
pub struct StrobeSpots {
    pub pd: Pd,
    pub duty: f32,
}
#[rustfmt::skip]
impl Preset for StrobeSpots {
    fn beam_brightness(&self, _s: &State, _i: usize, _fr: f32) -> f32 {
        0.0
    }
    fn spot_brightness(&self, s: &State, _i: usize, fr: f32) -> f32 {
        s.pd(self.pd.mul(2))
            .phase(1.0, fr)
            .square(1.0, self.duty.in_exp().lerp(1.0..0.5))
    }

    fn pad_brightness(&self, s: &State) -> f32 {
        s.pd(self.pd.mul(2)).square(1.0, self.duty.in_exp().lerp(1.0..0.5))
    }
    fn visuals_brightness(&self, s: &State) -> f32 {
        self.pad_brightness(s)
    }

    fn beam_pattern(&self) -> BeamPattern { BeamPattern::Square }
    fn pad_pattern(&self) -> PadPattern { PadPattern::Strobe }

    fn visuals_pd(&self) -> Option<Pd> { Some(self.pd) }
}

///////////////////////// Chases /////////////////////////

// White sequential chase
#[derive(Clone)]
pub struct Chase {
    pub pd: Pd,
    pub beam: BeamPattern,
}
#[rustfmt::skip]
impl Preset for Chase {
    fn beam_color(&self, _: &State) -> Rgbw { Rgbw::WHITE }
    fn spot_color(&self, _: &State) -> Rgbw { Rgbw::WHITE }
    fn visuals_gradient(&self, s: &State) -> RgbwGradient { RgbwGradient::solid(Rgbw::WHITE) }

    fn beam_brightness(&self, s: &State, _i: usize, fr: f32) -> f32 {
        s.pd(self.pd.mul(4)).phase(1.0, fr).square(1.0, 0.1)
    }
    fn spot_brightness(&self, s: &State, i: usize, fr: f32) -> f32 {
        self.beam_brightness(s, i, fr)
    }

    fn pad_brightness(&self, s: &State) -> f32 {
        s.pd(self.pd).square(1.0, 0.5)
    }
    fn visuals_brightness(&self, s: &State) -> f32 {
        self.pad_brightness(s)
    }

    fn beam_pattern(&self) -> BeamPattern { self.beam }
    fn pad_pattern(&self) -> PadPattern { PadPattern::Spiral }
}

#[derive(Clone)]
pub struct ChaseSmooth {
    pub pd: Pd,
    pub beam: BeamPattern,
}
#[rustfmt::skip]
impl Preset for ChaseSmooth {
    fn beam_brightness(&self, s: &State, _i: usize, fr: f32) -> f32 {
        s.pd(self.pd.mul(4)).phase(1.0, fr).tri(1.0)
    }
    fn spot_brightness(&self, s: &State, i: usize, fr: f32) -> f32 {
        self.beam_brightness(s, i, fr)
    }

    fn pad_brightness(&self, s: &State) -> f32 {
        s.pd(self.pd.mul(4)).tri(1.0)
    }

    fn beam_pattern(&self) -> BeamPattern { self.beam }
    fn pad_pattern(&self) -> PadPattern { PadPattern::WaveX }
}
