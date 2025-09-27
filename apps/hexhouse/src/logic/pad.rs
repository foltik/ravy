use std::sync::Arc;

use lib::midi::device::launchpad_x::LaunchpadX;
use lib::prelude::*;

use crate::logic::{Palette, Preset, State};

///////////////////////// VISUALS /////////////////////////

#[derive(Clone, Copy, Debug)]
pub enum PadPattern {
    Off,
    Solid,
    Strobe,
    WaveX,
    WaveY,
    WaveDiagXY,
    WaveDiagYX,
    Spiral,
    Random,
}

impl PadPattern {
    pub fn env(&self, s: &State, x: i8, y: i8) -> f32 {
        let dir = if (s.preset_i & 1) == 0 { 1.0 } else { -1.0 };
        let xf = x as f32;
        let yf = y as f32;

        match *self {
            PadPattern::Off => 0.0,
            PadPattern::Solid => 1.0,
            PadPattern::Strobe => s.phi(1, 1).square(1.0, 0.6),
            PadPattern::WaveX => s.phi(1, 1).phase(1.0, (xf / 8.0) * dir).tri(1.0),
            PadPattern::WaveY => 1.0 - s.phi(4, 1).ramp(1.0).phase(1.0, (yf / 8.0) * 0.5 * dir).out_exp(),
            PadPattern::WaveDiagXY => (s.phi - xf * 0.125 + yf * 0.125).fsin(2.0).in_quad(),
            PadPattern::WaveDiagYX => (s.phi - yf * 0.125 + xf * 0.125).fsin(2.0).in_quad(),
            PadPattern::Spiral => {
                let speed = if dir > 0.0 { 12.0 } else { -8.0 };
                let (x, y) = ((xf / 7.0) * 2.0 - 1.0, (yf / 7.0) * 2.0 - 1.0);
                let (u, v) = ((x * x + y * y).sqrt(), y.atan2(x));

                let swirl = 0.5; // s.test0;
                let spokes = 2.0; // (4.0 * s.test1).floor();

                let _step = |thres, t: f32| if t < thres { 0.0 } else { 1.0 };
                let smoothstep = |thres: f32, epsilon: f32, t: f32| {
                    let (start, end) = (thres - epsilon, thres + epsilon);

                    if t < start {
                        0.0
                    } else if t >= start && t <= end {
                        t.map(start..end, 0.0..1.0).inout_quad()
                    } else {
                        1.0
                    }
                };

                smoothstep(0.0, 1.0, ((4.0 * swirl / u) + (spokes * v) + (speed * s.t)).sin())
            }
            // Deterministic variety based on preset_i.
            PadPattern::Random => {
                const CHOICES: &[PadPattern] = &[
                    PadPattern::Spiral,
                    PadPattern::WaveX,
                    PadPattern::WaveY,
                    PadPattern::WaveDiagXY,
                    PadPattern::WaveDiagYX,
                ];
                let pick = CHOICES[(s.preset_i as usize) % CHOICES.len()];
                return pick.env(s, x, y);
            }
        }
    }

    /// Full 8×8 batch via `render(x,y)`.
    pub fn render(&self, s: &State) -> Vec<(i8, i8, Rgbw)> {
        let col = s.preset.pad_color(s) * s.preset.visuals_brightness(s);

        let mut batch = Vec::with_capacity(64);
        for x in 0..8_i8 {
            for y in 0..8_i8 {
                batch.push((x, y, col * self.env(s, x, y)));
            }
        }
        batch
    }
}

///////////////////////// MACROS /////////////////////////

pub struct PadBinding {
    pub xy: (i8, i8),
    pub op: PadOp,
}

pub enum PadOp {
    Preset(Box<dyn Preset>),
    Palette(Box<dyn Palette>),
    Beat {
        side: usize,
        pd: Pd,
    },
    Func {
        color: Rgbw,
        func: Arc<dyn Fn(&mut State, &mut Midi<LaunchpadX>) + Send + Sync + 'static>,
    },
}

#[macro_export]
macro_rules! bind {
    ( $( ($x:expr, $y:expr) => $op:expr),* $(,)? ) => {
        fn bindings() -> Vec<$crate::logic::PadBinding> {
            vec![
                $(
                    $crate::logic::PadBinding {
                        xy: ($x, $y),
                        op: $op,
                    }
                ),*
            ]
        }
    };
}

#[macro_export]
macro_rules! preset {
    ($v:expr) => {
        $crate::logic::PadOp::Preset(Box::new($v))
    };
}

#[macro_export]
macro_rules! palette {
    ($v:expr) => {
        $crate::logic::PadOp::Palette(Box::new($v))
    };
}

#[macro_export]
macro_rules! beat {
    ($side:expr, $pd:expr) => {
        $crate::logic::PadOp::Beat { side: $side, pd: $pd }
    };
}

#[macro_export]
macro_rules! func {
    ($color:expr, $func:expr) => {
        $crate::logic::PadOp::Func { color: $color, func: std::sync::Arc::new($func) }
    };
}
