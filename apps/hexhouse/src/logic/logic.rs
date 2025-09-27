use itertools::Itertools;
// - [ ] port more shaders
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
use crate::{beat, bind, func, palette, preset};

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

    // Beats
    (0, 0) => beat!(0, Pd(4, 1)),
    (0, 1) => beat!(0, Pd(2, 1)),
    (0, 2) => beat!(0, Pd(1, 1)),
    (0, 3) => beat!(0, Pd(1, 2)),
    (0, 4) => beat!(0, Pd(1, 4)),
    (7, 0) => beat!(1, Pd(4, 1)),
    (7, 1) => beat!(1, Pd(2, 1)),
    (7, 2) => beat!(1, Pd(1, 1)),
    (7, 3) => beat!(1, Pd(1, 2)),
    (7, 4) => beat!(1, Pd(1, 4)),

    // Red, Red/White, Red/White cycle
    // Green, Pea, Mint, Green/White cycle
    // Blue, Blue/Green, Blue/White
    //
    // Rainbow, Rgb cycle
    //
    // White, Rgbw

    // --- Palettes ---
    // Whites
    (0, 6) => palette!(Solid(Rgbw::WHITE)),
    (0, 5) => palette!(Solid(Rgbw::RGBW)),
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

    // // --- y=0: Off / Break / Special ---
    (1, 0) => preset!(Off),
    (2, 0) => preset!(Break { beams: BeamPattern::Out }),
    (3, 0) => preset!(Break { beams: BeamPattern::WaveY }),
    (4, 0) => preset!(Whirl { pd: Pd(16, 1) }),
    (5, 0) => preset!(RaisingBeams { pd: Pd(8, 1) }),
    (6, 0) => preset!(Break { beams: BeamPattern::LookAtSway { pd: Pd(4, 1), target_pos: vec3(-2.2408, 3.6088, 2.6469), delta: vec3(0.1, 0.1, 0.1) }}), // Disco ball
    // (6, 0) => preset!(Break { beams: BeamPattern::LookAt(vec3(-2.2408, 3.6088, 2.6469)) }),

    // // --- y=0: Off / Break / Special ---
    // (1, 0) => preset!(Off),
    // (2, 0) => preset!(Break { beams: BeamPattern::LookAt(vec3(-2.2408, 3.6088, 2.6469)) }),
    // (3, 0) => preset!(Break { beams: BeamPattern::WaveY }),
    // (4, 0) => preset!(RaisingBeams { pd: Pd(8, 1) }),
    // (5, 0) => preset!(Whirl { pd: Pd(16, 1) }),
    // (6, 0) => preset!(Break { beams: BeamPattern::Spinner }),

    // --- y=2: Pd(4,1) family ---
    (1, 1) => preset!(ChaseSmooth { pd: Pd(1, 1), beam: BeamPattern::Whirl }),
    (2, 1) => preset!(AutoBeat { pd: Pd(4, 1), beam: BeamPattern::RaisingBeams }),
    (3, 1) => preset!(AutoBeat { pd: Pd(4, 1), beam: BeamPattern::WaveY }),
    (4, 1) => preset!(AutoBeat { pd: Pd(4, 1), beam: BeamPattern::Spinner }),
    (5, 1) => preset!(AutoBeat { pd: Pd(4, 1), beam: BeamPattern::Whirl }),
    (6, 1) => preset!(AutoBeat { pd: Pd(4, 1), beam: BeamPattern::Twisting }),

    // --- y=3: Pd(2,1) family ---
    (1, 2) => preset!(ChaseSmooth { pd: Pd(1, 2), beam: BeamPattern::Whirl }),
    (2, 2) => preset!(AutoBeat { pd: Pd(2, 1), beam: BeamPattern::RaisingBeams }),
    (3, 2) => preset!(AutoBeat { pd: Pd(2, 1), beam: BeamPattern::WaveY }),
    (4, 2) => preset!(AutoBeat { pd: Pd(2, 1), beam: BeamPattern::Spinner }),
    (5, 2) => preset!(AutoBeat { pd: Pd(2, 1), beam: BeamPattern::Whirl }),
    (6, 2) => preset!(AutoBeat { pd: Pd(2, 1), beam: BeamPattern::Twisting }),

    // --- y=4: Pd(1,1) family ---
    (1, 3) => preset!(AutoBeat { pd: Pd(1, 1), beam: BeamPattern::Square }),
    (2, 3) => preset!(AutoBeat { pd: Pd(1, 1), beam: BeamPattern::RaisingBeams }),
    (3, 3) => preset!(AutoBeat { pd: Pd(1, 1), beam: BeamPattern::WaveY }),
    (4, 3) => preset!(AutoBeat { pd: Pd(1, 1), beam: BeamPattern::Spinner }),
    (5, 3) => preset!(AutoBeat { pd: Pd(1, 1), beam: BeamPattern::Whirl }),
    (6, 3) => preset!(AutoBeat { pd: Pd(1, 1), beam: BeamPattern::Twisting }),

    // --- y=5: Strobes / Chase ---
    (3, 4) => preset!(StrobeBeams { pd: Pd(1, 4), duty: 1.0 }),
    (4, 4) => preset!(Strobe { pd: Pd(1, 4), duty: 1.0 }),
    (5, 4) => preset!(Strobe { pd: Pd(1, 8), duty: 1.0 }),
    (6, 4) => preset!(Chase { pd: Pd(1, 2), beam: BeamPattern::Whirl }),
    (7, 4) => preset!(Chase { pd: Pd(1, 4), beam: BeamPattern::Whirl }),
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
    /// Per-side monotonic counters incrementing on tap
    pub beat_c0: f32,
    pub beat_c1: f32,

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

    /// Visuals state.
    pub visuals: Visuals,
    pub sent_visuals: Option<Visuals>,
    /// Global visuals brightness modifier
    pub visuals_brightness: f32,
    /// Visuals playback speed
    pub visuals_speed: f32,

    /// Whether the 4 directional buttons (shift) are held.
    pub shift: [bool; 4],
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
            beat_c0: 0.0,
            beat_c1: 0.0,

            brightness: 1.0,
            visuals_brightness: 1.0,
            visuals_speed: 1.0,
            visuals: Visuals { dj: Dj::Laptou, style: Style::Text, i: 0 },
            sent_visuals: None,
            visualizer: false,
            lock: false,
            shift: [false; 4],
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
    pub fn beat_c0(&self) -> f32 {
        let mut v = self.beat_c0;
        if let Some(Beat { t0, pd0, .. }) = self.beat {
            let p = pd0.fr().max(1e-6); // Pd in beats (e.g., 4, 2, 1, 0.5, 0.25)
            let dphi = ((self.t - t0).max(0.0)) * (self.bpm / 60.0) * self.phi_mul; // beats since tap
            let u = (dphi / p).clamp(0.0, 1.0); // 0..1 progress across Pd
            let e = 1.0 - (1.0 - u) * (1.0 - u); // ease-out quad (inline)
            v = self.beat_c0 - (1.0 - e); // goes (N-1) -> N
        }
        v
    }
    pub fn beat_c1(&self) -> f32 {
        let mut v = self.beat_c1;
        if let Some(Beat { t1, pd1, .. }) = self.beat {
            let p = pd1.fr().max(1e-6);
            let dphi = ((self.t - t1).max(0.0)) * (self.bpm / 60.0) * self.phi_mul;
            let u = (dphi / p).clamp(0.0, 1.0);
            let e = 1.0 - (1.0 - u) * (1.0 - u);
            v = self.beat_c1 - (1.0 - e);
        }
        v
    }
}

///////////////////////// VISUALS /////////////////////////

#[derive(Clone, Copy, PartialEq)]
struct Visuals {
    dj: Dj,
    style: Style,
    i: usize,
}

#[derive(Clone, Copy, PartialEq)]
enum Dj {
    Laptou,
    Chub,
    Suzi,
    MusicDJ102,
    Helix,
    Xuko,
}

// laptou:
// - lily script
// - commando
// - babak
// - technique brk 800
// - rubik wet paint

// chub:
// - pusab
// -
// - rubik wet paint

#[derive(Clone, Copy, PartialEq)]
enum Style {
    Text,
    Fractal,
    Landscape,
    Temple,
    Video,
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
    s.phi += (s.bpm / 60.0) * s.phi_mul * dt;

    {
        let g: RgbGradient = s.preset.visuals_gradient(s).into();
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
        syn.set_ravy_float("mask", s.visuals_brightness * s.preset.visuals_brightness(s));

        syn.set_control("meta", "playbackspeed", s.visuals_speed);

        if s.sent_visuals.is_none() || s.sent_visuals.is_some_and(|v| v != s.visuals) {
            match s.visuals.style {
                Style::Text => match s.visuals.dj {
                    Dj::Laptou => match s.visuals.i % 3 {
                        0 => {
                            syn.launch_scene("0textneontrail");
                            syn.launch_media("laptou-1");
                        }
                        1 => {
                            syn.launch_scene("0textneontrail");
                            syn.launch_media("laptou-2");
                        }
                        2 => {
                            syn.launch_scene("0textphasorbloom");
                            syn.launch_media("laptou-1");
                        }
                        _ => unreachable!(),
                    },
                    Dj::Chub => todo!(),
                    Dj::Suzi => todo!(),
                    Dj::MusicDJ102 => todo!(),
                    Dj::Helix => todo!(),
                    Dj::Xuko => todo!(),
                },
                Style::Fractal => todo!(),
                Style::Landscape => todo!(),
                Style::Temple => todo!(),
                Style::Video => match s.visuals.i % 3 {
                    0 => {
                        syn.launch_scene("0videorainbowshift");
                        syn.launch_media("tarzan");
                    }
                    1 => {
                        syn.launch_scene("0videorainbowshift");
                        syn.launch_media("tarzan");
                    }
                    2 => {
                        syn.launch_scene("0videorainbowshift");
                        syn.launch_media("tarzan");
                    }
                    _ => unreachable!(),
                },
            }

            s.sent_visuals = Some(s.visuals);
        }
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
        l.for_each_beam(|beam, _i, _fr| beam.color = s.preset.beam_color(s));
        l.for_each_spot(|spot, _i, _fr| spot.color = s.preset.spot_color(s));

        // Preset spot/beam shapers
        l.for_each_beam(|beam, i, fr| {
            beam.color *= s.preset.beam_brightness(s, i, fr);

            let pd = s.preset.visuals_pd().unwrap_or(Pd(4, 1));
            let (pitch, yaw) = s.preset.beam_pattern().angles(s, pd, i, fr, beam.transform);
            beam.pitch = pitch;
            beam.yaw = yaw;
        });
        l.for_each_spot(|spot, i, fr| {
            spot.color *= s.preset.spot_brightness(s, i, fr);
        });

        // Global brightness
        l.map_colors(|c| c * s.brightness);
        // Global beat mask
        l.for_each_beam(|beam, _, _| beam.color = beam.color * s.beat_fr0().unwrap_or(1.0));
        l.for_each_spot(|par, _, _| par.color = par.color * s.beat_fr1().unwrap_or(1.0));
    }
    l.send(&mut *e131);
}

///////////////////////// PAD INPUT /////////////////////////

pub fn on_pad(mut s: ResMut<State>, mut pad: ResMut<Midi<LaunchpadX>>) {
    let s: &mut State = &mut *s;
    let pad: &mut Midi<LaunchpadX> = &mut *pad;

    use launchpad_x::*;

    for input in pad.recv() {
        // Handle shift keys
        match input {
            Input::Up(b) => s.shift[0] = b,
            Input::Down(b) => s.shift[1] = b,
            Input::Left(b) => s.shift[2] = b,
            Input::Right(b) => s.shift[3] = b,
            _ => {}
        }

        // Handle brightness/visuals keys
        if let Some((x, y)) = input.xy() {
            match (x, y) {
                // No shift: change LIGHTS brightness
                (8, y) if s.shift.iter().all(|b| !b) => {
                    let fr = y as f32 / 7.0;
                    s.brightness = fr;
                }

                // Shift 0: change PROJECTOR brightness
                (8, y) if s.shift[0] => {
                    let fr = y as f32 / 7.0;
                    s.visuals_brightness = fr;
                }

                // Shift 1: change visuals SPEED
                (8, y) if s.shift[1] => {
                    let fr = y as f32 / 7.0;
                    s.visuals_speed = 2.0 * fr;
                }

                // Shift 2: change VISUAL
                (8, y) if s.shift[2] => {
                    s.visuals.i += 1;
                    match y {
                        0 => s.visuals.style = Style::Text,
                        1 => s.visuals.style = Style::Fractal,
                        2 => s.visuals.style = Style::Landscape,
                        3 => s.visuals.style = Style::Temple,
                        4 => s.visuals.style = Style::Video,
                        _ => {}
                    };
                }

                // Shift 3: change DJ
                (8, y) if s.shift[3] => match y {
                    0 => s.visuals.dj = Dj::Laptou,
                    1 => s.visuals.dj = Dj::Chub,
                    2 => s.visuals.dj = Dj::Suzi,
                    3 => s.visuals.dj = Dj::MusicDJ102,
                    4 => s.visuals.dj = Dj::Helix,
                    5 => s.visuals.dj = Dj::Xuko,
                    _ => {}
                },
                _ => {}
            }
        }

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
                            s.beat_c0 += 1.0;
                        }
                        1 => {
                            beat.t1 = s.t;
                            beat.pd1 = *pd;
                            s.beat_c1 += 1.0;
                        }
                        _ => unreachable!(),
                    },
                    None => match side {
                        0 => {
                            s.beat = Some(Beat { t0: s.t, t1: 0.0, pd0: *pd, pd1: *pd });
                            s.beat_c0 += 1.0;
                        }
                        1 => {
                            s.beat = Some(Beat { t0: 0.0, t1: s.t, pd0: *pd, pd1: *pd });
                            s.beat_c1 += 1.0;
                        }
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
                PadOp::Preset(preset) => set(*x, *y, preset.pad_color(s) * preset.pad_brightness(s)),
                PadOp::Palette(palette) => set(*x, *y, palette.beam_color(s)),
                PadOp::Func { color, .. } => set(*x, *y, *color),
                _ => {}
            }
        }

        // Shift buttons
        for x in 0..4 {
            set(x, 8, if s.shift[x as usize] { Rgbw::WHITE } else { Rgbw::BLACK });
        }

        if s.shift[0] {
            // Shift 0: change PROJECTOR brightness
            for y in 0..8 {
                let fr = y as f32 / 7.0;
                let col = Rgb::hsv(s.phi(16, 1), 1.0, 1.0);
                set(8, y, Rgbw::from(col) * fr);
            }
        } else if s.shift[1] {
            // Shift 1: change visuals SPEED
            for y in 0..8 {
                let fr = y as f32 / 7.0;
                let fr = s.pd(Pd(1, 4).mul(2)).square(1.0, 1.0 - fr.lerp(0.1..0.9));
                set(8, y, Rgbw::WHITE * fr);
            }
        } else if s.shift[2] {
            // Shift 2: change VISUAL
            set(8, 0, Rgbw::RED);
            set(8, 1, Rgbw::ORANGE);
            set(8, 2, Rgbw::YELLOW);
            set(8, 3, Rgbw::LIME);
            set(8, 4, Rgbw::PEA);
            for i in 4..8 {
                set(8, i, Rgbw::BLACK);
            }
        } else if s.shift[3] {
            // Shift 3: change DJ
            set(8, 0, Rgbw::MINT);
            set(8, 1, Rgbw::CYAN);
            set(8, 2, Rgbw::BLUE);
            set(8, 3, Rgbw::VIOLET);
            set(8, 4, Rgbw::MAGENTA);
            set(8, 5, Rgbw::PINK);
            for i in 6..8 {
                set(8, i, Rgbw::BLACK);
            }
        } else {
            // No shift: change LIGHTS brightness
            for y in 0..8 {
                let fr = y as f32 / 7.0;
                set(8, y, Rgbw::WHITE * fr);
            }
        }

        // Beat buttons
        for i in 0..=4 {
            // Upwards propagating wave at BPM
            let col = Rgbw::WHITE * (s.phi - i as f32 * 0.2).fsin(2.0);
            set(0, i, col);
            set(7, i, col);
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

pub fn on_ctrl(mut s: ResMut<State>, mut ctrl: ResMut<Midi<LaunchControlXL>>) {
    let s: &mut State = &mut *s;

    for input in ctrl.recv() {
        use launch_control_xl::*;
        debug!("ctrl: {input:?}");

        // - [ ] put back E131 error logs
        //
        // - [ ] make text visuals for all djs
        // - [ ] port more shaders
        // - [ ] MOSH/JUMP/GO-BANANAS visual
        // - [~] map visuals to launchpad buttons
        //
        // - [~] point beams at dj, center of floor
        //
        // - [ ] constant lower brightness for spots? * 0.2 globally?
        // - [ ] take ChaseSmooth pattern for spots (add SpotPattern?)
        // - [ ] auto preset switching
        //
        // - [ ] add more colors
        // - [ ] make beam presets better

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
