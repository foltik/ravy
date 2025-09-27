use std::sync::Arc;

use lib::midi::device::launchpad_x::LaunchpadX;
use lib::prelude::*;

use crate::logic::{Palette, Preset, State};

///////////////////////// VISUALS /////////////////////////

#[derive(Clone, Copy, Debug)]
pub enum PadPattern {
    Off,
    Spiral,
    Pulse,
    Strobe,
    WaveX,
    WaveY,
    Random,
}

impl PadPattern {
    pub fn render(&self, s: &State) -> Vec<(i8, i8, Rgbw)> {
        let mut batch = Vec::with_capacity(64);
        let dir = if (s.preset_i & 1) == 0 { 1.0 } else { -1.0 };

        match self {
            PadPattern::Off => {
                for x in 0..8_i8 {
                    for y in 0..8_i8 {
                        batch.push((x, y, Rgbw::BLACK));
                    }
                }
            }
            PadPattern::Spiral => {
                let col = s.palette.color0(s);
                for x in 0..8_i8 {
                    for y in 0..8_i8 {
                        batch.push((x, y, col * spiral(s.t, x, y, 12.0)));
                    }
                }
            }

            // AutoBeat family, selected deterministically by preset_i.
            PadPattern::Pulse => {
                let col0 = s.palette.color0(s);
                let mode = (s.preset_i % 6) as i32;
                match mode {
                    // 1: Upwards propagating wave at BPM
                    1 => {
                        for i in 0..8_i8 {
                            let env = (s.phi - (i as f32) * 0.125).fsin(2.0).inout_exp();
                            let c = col0 * env;
                            for j in 0..8_i8 {
                                batch.push((j, i, c));
                            }
                        }
                    }
                    // 2: Sideways propagating wave at BPM
                    2 => {
                        for i in 0..8_i8 {
                            let env = (s.phi - (i as f32) * 0.125).fsin(2.0).inout_exp();
                            let c = col0 * env;
                            for j in 0..8_i8 {
                                batch.push((i, j, c));
                            }
                        }
                    }
                    // 3: Sideways staggered propagating wave
                    3 => {
                        for x in 0..8_i8 {
                            for y in 0..8_i8 {
                                let env =
                                    (s.phi - (x as f32) * 0.125 + (y as f32) * 0.125).fsin(2.0).in_quad();
                                batch.push((x, y, col0 * env));
                            }
                        }
                    }
                    // 4: Sideways staggered propagating wave (swapped axes)
                    4 => {
                        for x in 0..8_i8 {
                            for y in 0..8_i8 {
                                let env =
                                    (s.phi - (x as f32) * 0.125 + (y as f32) * 0.125).fsin(2.0).in_quad();
                                batch.push((y, x, col0 * env));
                            }
                        }
                    }
                    // 5: Whirl
                    5 => {
                        for x in 0..8_i8 {
                            for y in 0..8_i8 {
                                batch.push((x, y, col0 * spiral(s.t, x, y, -8.0)));
                            }
                        }
                    }
                    // 0 or other: Downwards propagating wave at BPM
                    _ => {
                        for i in 0..8_i8 {
                            let env = (s.phi + (i as f32) * 0.125).fsin(2.0).inout_exp();
                            let c = col0 * env;
                            for j in 0..8_i8 {
                                batch.push((j, i, c));
                            }
                        }
                    }
                }
            }

            PadPattern::Strobe => {
                let env = s.phi(1, 1).square(1.0, 0.6);
                let col = s.palette.color0(s) * env;
                for x in 0..8_i8 {
                    for y in 0..8_i8 {
                        batch.push((x, y, col));
                    }
                }
            }

            // Horizontal sweep, alternates LR each preset.
            PadPattern::WaveX => {
                for x in 0..8_i8 {
                    let fx = x as f32 / 8.0;
                    let env = s.phi(1, 1).phase(1.0, fx * dir).tri(1.0);
                    let col = s.palette.color0(s) * env;
                    for y in 0..8_i8 {
                        batch.push((x, y, col));
                    }
                }
            }

            // Vertical sweep, alternates UD each preset.
            PadPattern::WaveY => {
                for y in 0..8_i8 {
                    let fy = y as f32 / 8.0;
                    let env = s.phi(4, 1).ramp(1.0).phase(1.0, fy * 0.5 * dir).out_exp();
                    let col = s.palette.color0(s) * (1.0 - env);
                    for x in 0..8_i8 {
                        batch.push((x, y, col));
                    }
                }
            }

            PadPattern::Random => {
                let choices = [
                    PadPattern::Spiral,
                    PadPattern::Pulse,
                    PadPattern::Strobe,
                    PadPattern::WaveX,
                    PadPattern::WaveY,
                ];
                let pick = choices[(s.preset_i as usize) % choices.len()];
                return pick.render(s);
            }
        }

        batch
    }
}

fn spiral(time: f32, x: i8, y: i8, speed: f32) -> f32 {
    let (x, y) = ((x as f32 / 7.0) * 2.0 - 1.0, (y as f32 / 7.0) * 2.0 - 1.0);
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

    smoothstep(0.0, 1.0, ((4.0 * swirl / u) + (spokes * v) + (speed * time)).sin())
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
