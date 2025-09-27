use itertools::Itertools;
use lib::lights::fixture::{SaberSpot, StealthBeam};
use lib::midi::device::launch_control_xl::{self, LaunchControlXL};
use lib::midi::device::launchpad_x::{self, LaunchpadX};
use lib::prelude::*;

// use rand::Rng;
// use rand::rngs::ThreadRng;
use super::palette::Palette;
use super::preset::Preset;
use crate::lights::Lights;
use crate::logic::{BeamPattern, PadBinding, PadOp};
use crate::{DiscoBall, FixtureChannel, FixtureIndex};

///////////////////////// BINDINGS /////////////////////////

const _: () = {};
use super::palette::*;
use super::preset::*;
use crate::{bind, func, palette, preset};

bind! {
    // Lock
    (6, 8) => func!(Rgbw::BLACK, |s, _| {
        s.lock = !s.lock;
    }),
    // Toggle pad visualizer
    (7, 8) => func!(Rgbw::BLACK, |s, pad| {
        s.visualizer = !s.visualizer;
        pad.send(lib::midi::device::launchpad_x::Output::Clear);
    }),

    // Tap to record BPM
    (0, 7) => func!(Rgbw::WHITE, |s, _| {
        s.bpm_taps.push(s.t);
    }),
    // Tap to calculate BPM and reset phase
    (7, 7) => func!(Rgbw::WHITE, |s, _| {
        match s.bpm_taps.len() {
            0 => { s.phi = 0.0; }
            1 => { s.bpm_taps.clear(); }
            n => {
                let dts = s.bpm_taps.drain(..).tuple_windows().map(|(t0, t1)| t1 - t0);
                let dt = dts.sum::<f32>() / (n as f32 - 1.0);
                let bpm = 60.0 / dt;
                s.phi = 0.0;
                s.bpm = bpm;
                info!("Calculated bpm={bpm:.2} from {n} samples");
            }
        }
    }),

    // --- Palettes ---
    // Whites
    (0, 6) => palette!(Solid(Rgbw::WHITE)),
    (7, 6) => palette!(Solid(Rgbw::RGBW)),
    // Reds
    (1, 6) => palette!(Solid(Rgbw::RED)),
    (2, 6) => palette!(Split(Rgbw::RED, Rgbw::BLACK)),
    (3, 6) => palette!(Split(Rgbw::BLACK, Rgbw::RED)),
    (1, 7) => palette!(Split(Rgbw::RED, Rgbw::WHITE)),
    (2, 7) => palette!(Split(Rgbw::WHITE, Rgbw::RED)),
    (3, 7) => palette!(Cycle([Rgbw::RED, Rgbw::WHITE])),
    // Green/Blues
    (4, 6) => palette!(Solid(Rgbw::LIME)),
    (4, 7) => palette!(Split(Rgbw::LIME, Rgbw::WHITE)),
    (5, 6) => palette!(Solid(Rgbw::BLUE)),
    (5, 7) => palette!(Split(Rgbw::LIME, Rgbw::BLUE)),
    (6, 6) => palette!(Split(Rgbw::BLUE, Rgbw::WHITE)),
    (6, 7) => palette!(Split(Rgbw::WHITE, Rgbw::BLUE)),

    // --- y=0: Off / Break / Special ---
    (1, 0) => preset!(Off),
    (2, 0) => preset!(Break { beams: Some(BeamPattern::Out) }),
    (3, 0) => preset!(Break { beams: Some(BeamPattern::WaveY) }),
    (4, 0) => preset!(RaisingBeams { pd: Pd(8, 1) }),
    (5, 0) => preset!(Whirl { pd: Pd(16, 1) }),
    (6, 0) => preset!(Break { beams: Some(BeamPattern::UpDownWave) }),

    // --- y=1: Solid patterns (On + beam) ---
    (1, 1) => preset!(On { beams: Some(BeamPattern::Out) }),
    (2, 1) => preset!(On { beams: Some(BeamPattern::Out) }),
    (3, 1) => preset!(On { beams: Some(BeamPattern::WaveY) }),
    (4, 1) => preset!(On { beams: Some(BeamPattern::SnapX) }),
    (5, 1) => preset!(On { beams: Some(BeamPattern::Whirl) }),
    (6, 1) => preset!(On { beams: Some(BeamPattern::Twisting) }),

    // --- y=2: Pd(4,1) family ---
    (1, 2) => preset!(ChaseSmooth { pd: Pd(1, 1), beam: BeamPattern::WaveY }),
    (2, 2) => preset!(AutoBeat { pd: Pd(4, 1), r: (0.2..1.0).into(), beam: BeamPattern::RaisingBeams }),
    (3, 2) => preset!(AutoBeat { pd: Pd(4, 1), r: (0.2..1.0).into(), beam: BeamPattern::WaveY }),
    (4, 2) => preset!(AutoBeat { pd: Pd(4, 1), r: (0.2..1.0).into(), beam: BeamPattern::UpDownWave }),
    (5, 2) => preset!(AutoBeat { pd: Pd(4, 1), r: (0.2..1.0).into(), beam: BeamPattern::Whirl }),
    (6, 2) => preset!(AutoBeat { pd: Pd(4, 1), r: (0.2..1.0).into(), beam: BeamPattern::Twisting }),

    // --- y=3: Pd(2,1) family ---
    (1, 3) => preset!(ChaseSmooth { pd: Pd(1, 2), beam: BeamPattern::Square }),
    (2, 3) => preset!(AutoBeat { pd: Pd(2, 1), r: (0.2..1.0).into(), beam: BeamPattern::RaisingBeams }),
    (3, 3) => preset!(AutoBeat { pd: Pd(2, 1), r: (0.2..1.0).into(), beam: BeamPattern::WaveY }),
    (4, 3) => preset!(AutoBeat { pd: Pd(2, 1), r: (0.2..1.0).into(), beam: BeamPattern::UpDownWave }),
    (5, 3) => preset!(AutoBeat { pd: Pd(2, 1), r: (0.2..1.0).into(), beam: BeamPattern::Whirl }),
    (6, 3) => preset!(AutoBeat { pd: Pd(2, 1), r: (0.2..1.0).into(), beam: BeamPattern::Twisting }),

    // --- y=4: Pd(1,1) family ---
    (1, 4) => preset!(AutoBeat { pd: Pd(1, 1), r: (0.2..1.0).into(), beam: BeamPattern::Square }),
    (2, 4) => preset!(AutoBeat { pd: Pd(1, 1), r: (0.2..1.0).into(), beam: BeamPattern::RaisingBeams }),
    (3, 4) => preset!(AutoBeat { pd: Pd(1, 1), r: (0.2..1.0).into(), beam: BeamPattern::WaveY }),
    (4, 4) => preset!(AutoBeat { pd: Pd(1, 1), r: (0.2..1.0).into(), beam: BeamPattern::UpDownWave }),
    (5, 4) => preset!(AutoBeat { pd: Pd(1, 1), r: (0.2..1.0).into(), beam: BeamPattern::Whirl }),
    (6, 4) => preset!(AutoBeat { pd: Pd(1, 1), r: (0.2..1.0).into(), beam: BeamPattern::Twisting }),

    // --- y=5: Strobes / Chase ---
    (0, 5) => preset!(Strobe0 { pd: Pd(1, 8), duty: 1.0 }),
    (1, 5) => preset!(ChaseNotColorful { pd: Pd(1, 4) }),
    (2, 5) => preset!(Strobe { pd: Pd(1, 4), duty: 1.0 }),
    (3, 5) => preset!(Strobe { pd: Pd(1, 8), duty: 1.0 }),
    (4, 5) => preset!(Strobe { pd: Pd(1, 8), duty: 1.0 }),
    (5, 5) => preset!(Chase { pd: Pd(1, 1), beam: BeamPattern::Twisting }),
    (6, 5) => preset!(Chase { pd: Pd(1, 2), beam: BeamPattern::Twisting }),
    (7, 5) => preset!(Chase { pd: Pd(1, 4), beam: BeamPattern::Twisting }),
}

///////////////////////// STATE /////////////////////////

#[derive(Resource)]
pub struct State {
    /// List of bindings defined just above.
    pub bindings: Vec<PadBinding>,
    /// Current color palette.
    pub palette: Box<dyn Palette>,
    /// Counter incremented when the palette is changed.
    pub palette_i: usize,
    /// Current lighting preset.
    pub preset: Box<dyn Preset>,
    /// Counter incremented when the preset is changed.
    pub preset_i: usize,

    /// Total time elapsed since startup in seconds
    pub t: f32,
    /// Current approximately matched BPM
    pub bpm: f32,
    /// Timestamps when the beatmatch button was tapped
    pub bpm_taps: Vec<f32>,
    /// Current fractional beat number in a 16 beat measure at the current `bpm`. Ranges from `0..16` and wraps around
    pub phi: f32,
    /// Bpm multiplier, e.g. 0.5 for half-time, 2.0 for double-time.
    pub phi_mul: f32,

    /// Manual beat
    pub beat: Option<Beat>,

    // /// Preset loop
    // pub preset: bool,
    // /// Whether we've swapped the preset yet
    // pub preset_switched: bool,
    /// Global brightness modifier
    pub brightness: f32,

    /// Whether to show pretty effects instead of indicators Enable for colored button guide, disable for pretty pad effects.
    pub visualizer: bool,
    /// Whether to lock out inputs.
    pub lock: bool,
}

impl State {
    pub fn new() -> Self {
        Self {
            bindings: bindings(),
            palette: Box::new(Rainbow),
            preset: Box::new(Off),
            palette_i: 0,
            preset_i: 0,

            t: 0.0,
            bpm: 120.0,
            bpm_taps: vec![],
            phi: 0.0,
            phi_mul: 1.0,
            beat: None,

            brightness: 1.0,
            visualizer: false,
            lock: false,
        }
    }

    pub fn phi(&self, n: usize, d: usize) -> f32 {
        self.pd(Pd(n, d))
    }
    pub fn pd(&self, pd: Pd) -> f32 {
        self.phi.fmod_div(pd.fr())
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

///////////////////////// LOCKOUT /////////////////////////

#[derive(Clone, Copy, Debug, Default)]
#[allow(unused)]
pub enum Mode {
    /// All off
    #[default]
    Off,
    /// All on, solid color
    On {
        beams: Option<BeamPattern>,
    },
    /// TODO: ???
    Hover,
    /// Flashing to the beat
    AutoBeat {
        /// How often to flash
        pd: Pd,
        /// Brightness range for each flash, from 0..1
        r: Range,
        beam: BeamPattern,
    },
    /// Strobe lights
    // HalfStrobe {
    Strobe0 {
        pd: Pd,
        duty: f32,
    },
    Strobe1 {
        pd: Pd,
        duty: f32,
    },
    Strobe {
        pd: Pd,
        duty: f32,
    },
    Chase {
        pd: Pd,
        beam: BeamPattern,
    },
    ChaseSmooth {
        pd: Pd,
        beam: BeamPattern,
    },
    ChaseNotColorful {
        pd: Pd,
    },
    // Twinkle {
    //     pd: Pd,
    // },
    Whirl {
        pd: Pd,
    },
    RaisingBeams {
        pd: Pd,
    },
    Break {
        beams: Option<BeamPattern>,
    },
    Twisting {
        pd: Pd,
    },
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

pub fn tick(mut s: ResMut<State>, mut syn: ResMut<Synesthesia>, time: Res<Time>) {
    let s: &mut State = &mut *s;
    let dt = time.delta_secs();

    s.t += dt;
    s.phi = (s.phi + (dt * (s.bpm / 60.0) * s.phi_mul)).fmod(16.0);

    // TODO: move these to the match on Preset
    {
        let g: RgbGradient = s.palette.gradient(s).into();
        syn.set_ravy_vec3("palette_dc", g.dc);
        syn.set_ravy_vec3("palette_amp", g.amp);
        syn.set_ravy_vec3("palette_freq", g.freq);
        syn.set_ravy_vec3("palette_phase", g.phase);

        // Send phase and beat
        syn.set_ravy_float("phi", s.phi);
        syn.set_ravy_float(
            "beat",
            if let Some(fr) = s.beat_fr0() {
                fr
            } else if let Some(pd) = s.preset.visuals_pd() {
                s.pd(pd).inv()
            } else {
                0.0
            },
        );
        // Send brightness mask
        syn.set_ravy_float("mask", s.preset.visuals_mask(s));
    }
}

///////////////////////// LIGHTS /////////////////////////

pub fn render_lights<'a>(
    beams: Query<(&'a mut StealthBeam, &'a Transform, &'a FixtureChannel, &'a FixtureIndex)>,
    spots: Query<(&'a mut SaberSpot, &'a Transform, &'a FixtureChannel, &'a FixtureIndex)>,
    disco: Query<&'a Transform, With<DiscoBall>>,

    mut s: ResMut<State>,
    mut e131: ResMut<E131>,
) {
    let s: &mut State = &mut *s;
    let Some(mut l) = Lights::new(beams, spots, disco) else {
        return;
    };

    l.reset();
    {
        // Preset baseline colors
        let c0 = s.preset.palette_color0(s);
        let c1 = s.preset.palette_color1(s);
        l.split(c0, c1);

        // Preset spot/beam shapers
        l.for_each_spot(|par, i, fr| {
            let env = s.preset.light_spots(s, i, fr);
            par.color = par.color * env;
        });
        l.for_each_beam(|beam, i, fr| {
            let (env, pitch, yaw) = s.preset.light_beams(s, i, fr);
            beam.pitch = pitch;
            beam.yaw = yaw;
            beam.color = beam.color * env;
        });

        // Global brightness
        l.map_colors(|c| c * s.brightness);
        // Global beat mask
        l.for_each_spot(|par, _, _| par.color = par.color * s.beat_fr0().unwrap_or(1.0));
        l.for_each_beam(|beam, _, _| beam.color = beam.color * s.beat_fr1().unwrap_or(1.0));
    }
    l.send(&mut *e131);
}

///////////////////////// PAD INPUT /////////////////////////

pub fn on_pad(mut s: ResMut<State>, mut pad: ResMut<Midi<LaunchpadX>>) {
    let s: &mut State = &mut *s;
    let pad: &mut Midi<LaunchpadX> = &mut *pad;

    for input in pad.recv() {
        if let Some((x, y)) = input.xy() {
            debug!("Pad({x}, {y})");

            let Some(PadBinding { op, .. }) = s.bindings.iter().find(|b| b.xy == (x, y)) else {
                continue;
            };

            match op {
                PadOp::Func { func, .. } => func.clone()(s, pad),
                PadOp::Preset(preset) if !s.lock => {
                    s.preset = preset.clone();
                    s.preset_i += 1;

                    s.beat = None;
                    s.phi_mul = 1.0;
                }
                PadOp::Palette(palette) if !s.lock => {
                    s.palette = palette.clone();
                    s.palette_i += 1;
                }
                PadOp::Beat { side, pd } if !s.lock => match &mut s.beat {
                    Some(beat) => match side {
                        0 => {
                            beat.t0 = s.t;
                            beat.pd0 = *pd;
                        }
                        1 => {
                            beat.t1 = s.t;
                            beat.pd1 = *pd;
                        }
                        _ => unreachable!(),
                    },
                    None => match side {
                        0 => s.beat = Some(Beat { t0: s.t, t1: 0.0, pd0: *pd, pd1: *pd }),
                        1 => s.beat = Some(Beat { t0: 0.0, t1: s.t, pd0: *pd, pd1: *pd }),
                        _ => unreachable!(),
                    },
                },
                _ => {}
            }
        }
    }
}

///////////////////////// PAD OUTPUT /////////////////////////

pub fn render_pad(mut s: ResMut<State>, mut pad: ResMut<Midi<LaunchpadX>>) {
    let s: &mut State = &mut *s;

    use launchpad_x::types::*;
    use launchpad_x::*;

    let mut batch: Vec<(Pos, Color)> = vec![];

    // Helper to set an x/y coord to a certain color in the batch
    let rgb = |Rgb(r, g, b): Rgb| Color::Rgb(r, g, b);
    let mut set = |x, y, color: Rgbw| batch.push((Coord(x, y).into(), rgb(color.into())));

    if s.visualizer {
        // Run visualizer
        for (x, y, color) in s.preset.pad_pattern().render(s) {
            set(x, y, color);
        }
    } else {
        // Display bindings
        for PadBinding { xy: (x, y), op } in &s.bindings {
            match op {
                PadOp::Preset(preset) => set(*x, *y, preset.pad_color(s) * preset.pad_env(s)),
                PadOp::Palette(palette) => set(*x, *y, palette.color0(s)),
                PadOp::Func { color, .. } => set(*x, *y, *color),
                _ => {}
            }
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
    }

    pad.send(Output::Batch(batch));
}

///////////////////////// CTRL INPUT /////////////////////////

pub fn on_ctrl(mut s: ResMut<State>, mut ctrl: ResMut<Midi<LaunchControlXL>>, mut syn: ResMut<Synesthesia>) {
    let s: &mut State = &mut *s;

    for input in ctrl.recv() {
        use launch_control_xl::*;
        debug!("ctrl: {input:?}");

        match input {
            Input::Slider(7, fr) => s.brightness = fr,

            // Input::Slider(i, fr) => syn.set_slider(1 + i as usize, fr),
            // Input::Pan(i, fr) => syn.set_knob(1 + i as usize, fr * 0.5 + 0.5),
            // Input::Focus(i, true) => syn.set_bang(1 + i as usize, 1.0),
            // Input::Control(i, true) => {
            //     let i = i as usize;
            //     s.buttons[i] = !s.buttons[i];
            //     syn.set_toggle(i + 1, if s.buttons[i] { 1.0 } else { 0.0 });
            // }
            _ => {}
        }

        // Global
        //
        // Slider(0, fr) => syn.set_control("media", "playbackspeed", fr),
        // Focus(0, b) => syn.set_control("media", "invertmedia", if b { 1.0 } else { 0.0 }),
        //
        // - Off: multiply final color
        // - White strobe/chase: switch palette to black/white

        // # Text
        //
        // # Fractal
        //
        // # Abstract
        //
        // # Video

        // Scene 0
        // - Shader: "Text - Neon Tunnel"
        // - Media: artist logos
        //   - Button: swap text fonts
        // - Strobe: multiply final color

        // Scene 1
        // - Shader: "Video - Rainbow Shift"
        // - Video: "lionking"
        // - Strobe: invert palette

        // Scene 1
        // - Shader: "Video - Rainbow Shift"
        // - Video: "golfcart"
        // - Strobe: invert palette

        // Scene 3
        // Scene 4
        // Scene 5
        // Scene 6
        // Scene 7

        // Artist Logos
        // -
    }
}

///////////////////////// CTRL OUTPUT /////////////////////////

// #[allow(unused)]
// pub fn render_ctrl(mut s: ResMut<State>, ctrl: ResMut<Midi<LaunchControlXL>>) {
//     use launch_control_xl::types::*;
//     use launch_control_xl::*;
// }

///////////////////////// TODO /////////////////////////

// TODO: fix the old automatic mode switching code
// if s.auto {
//     let phi = (s.phi(16, 1) * 4.0) as usize;
//     if phi % 4 == 0 {
//         if !s.auto {
//             info!("SWITCH");
//             s.auto = true;
//             match ThreadRng::default().gen_range(1..=6) {
//                 1 => s.preset = Preset::ChaseSmooth { pd: Pd(1, 1), beam: BeamPattern::WaveY },
//                 2 => {
//                     s.preset = Preset::AutoBeat {
//                         pd: Pd(4, 1),
//                         r: (0.2..1.0).into(),
//                         beam: BeamPattern::RaisingBeams,
//                     }
//                 }
//                 3 => {
//                     s.preset =
//                         Preset::AutoBeat { pd: Pd(4, 1), r: (0.2..1.0).into(), beam: BeamPattern::WaveY }
//                 }
//                 4 => {
//                     s.preset = Preset::AutoBeat {
//                         pd: Pd(4, 1),
//                         r: (0.2..1.0).into(),
//                         beam: BeamPattern::UpDownWave,
//                     }
//                 }
//                 5 => {
//                     s.preset =
//                         Preset::AutoBeat { pd: Pd(4, 1), r: (0.2..1.0).into(), beam: BeamPattern::Whirl }
//                 }
//                 6 | _ => {
//                     s.preset = Preset::AutoBeat {
//                         pd: Pd(4, 1),
//                         r: (0.2..1.0).into(),
//                         beam: BeamPattern::Twisting,
//                     }
//                 }
//             }
//         }
//     } else {
//         s.preset_switched = false;
//     }
// }
