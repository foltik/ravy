#![allow(unused)]

use itertools::Itertools;
use lib::dmx::device::beam_rgbw_60w::Beam;
use lib::dmx::device::laser_scan_30w::{Laser, LaserColor, LaserPattern};
use lib::dmx::device::spider_rgbw_8x10w::Spider;
use lib::lights::personal::Personal as Lights;
use lib::midi::device::launch_control_xl::{self, LaunchControlXL};
use lib::midi::device::launchpad_x::{self, LaunchpadX};
use lib::prelude::*;
use rand::Rng;
use rand::rngs::ThreadRng;

mod ui;
mod utils;

/// Example app.
#[derive(argh::FromArgs)]
struct Args {
    /// enable debug logging
    #[argh(switch, short = 'v')]
    debug: bool,
    /// enable trace logging
    #[argh(switch, short = 'V')]
    trace: bool,
}

fn main() {
    let args: Args = argh::from_env();
    App::new()
        .add_plugins(RavyPlugin { module: module_path!(), debug: args.debug, trace: args.trace })
        .add_systems(Startup, setup)
        .add_systems(Update, (on_pad, on_ctrl, tick, render_lights, render_pad).chain())
        .add_systems(EguiPrimaryContextPass, ui::draw)
        .run();
}

fn setup(mut cmds: Commands) -> Result {
    let ctrl = Midi::new("Launch Control XL", LaunchControlXL::default());
    let mut pad = Midi::new("Launchpad X LPX MIDI", LaunchpadX::default());
    {
        use launchpad_x::types::*;
        use launchpad_x::*;
        pad.send(Output::Pressure(Pressure::Off, PressureCurve::Medium));
        pad.send(Output::Brightness(0.0));
    }
    cmds.insert_resource(pad);
    cmds.insert_resource(ctrl);

    cmds.insert_resource(E131::new("10.16.4.1")?);
    cmds.insert_resource(Lights::default());
    cmds.insert_resource(State::new());

    // Needed to draw the UI
    cmds.spawn(Camera2d);

    Ok(())
}

#[derive(Resource, Default)]
pub struct State {
    /// Time since the last `tick()` in seconds
    pub dt: f32,
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

    /// Color palette
    pub palette: Palette,
    /// Lighting mode
    pub mode: Mode,
    /// Manual beat
    pub beat: Option<ManualBeat>,

    /// Preset loop
    pub preset: bool,
    /// Whether we've swapped the preset yet
    pub preset_switched: bool,

    /// Global brightness modifier
    pub brightness: f32,

    /// Pad debug mode. Enable for colored button guide, disable for pretty pad effects.
    pub debug: bool,

    /// Most recently pressed x coord
    pub x: i8,
    /// Most recently pressed y coord
    pub y: i8,

    // Test paramters
    pub test0: f32,
    pub test1: f32,
    pub test2: f32,
    pub test3: f32,
    pub test4: f32,
}

impl State {
    pub fn new() -> Self {
        Self {
            debug: true,
            brightness: 0.25,
            palette: Palette::Rainbow,
            bpm: 120.0,
            phi_mul: 1.0,
            ..Default::default()
        }
    }

    fn phi(&self, n: usize, d: usize) -> f32 {
        self.pd(Pd(n, d))
    }
    fn pd(&self, pd: Pd) -> f32 {
        self.phi.fmod_div(pd.fr())
    }

    fn dt(&self, n: usize, d: usize) -> f32 {
        self.dt / ((self.bpm / 60.0) * Pd(n, d).fr())
    }
}

///////////////////////// LOCKOUT /////////////////////////

#[derive(Debug, Default)]
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

///////////////////////// COLOR PALETTE /////////////////////////

#[derive(Clone, Copy, Debug, Default)]
pub enum Palette {
    /// Gradually cycling rainbow
    #[default]
    Rainbow,
    RgbOsc,
    RainbowOsc,
    RedWhiteOsc,
    /// Solid color
    Solid(Rgbw),
    Split(Rgbw, Rgbw),
}

impl Palette {
    fn color0(self, s: &mut State, _fr: f32) -> Rgbw {
        match self {
            Palette::Rainbow => Rgb::hsv(s.phi(16, 1), 1.0, 1.0).into(),
            Palette::RgbOsc => match s.pd(Pd(1, 2)).ramp(1.0) {
                ..0.33 => Rgbw::RED,
                0.33..0.66 => Rgbw::LIME,
                _ => Rgbw::BLUE,
            },
            Palette::RainbowOsc => match (s.pd(Pd(1, 2)).ramp(1.0) * 8.0).floor() as u8 {
                0 => Rgbw::RED,
                1 => Rgbw::ORANGE,
                2 => Rgbw::YELLOW,
                3 => Rgbw::LIME,
                4 => Rgbw::MINT,
                5 => Rgbw::CYAN,
                6 => Rgbw::BLUE,
                _ => Rgbw::MAGENTA,
            },
            Palette::RedWhiteOsc => match s.pd(Pd(1, 2)).ramp(1.0) {
                ..0.5 => Rgbw::RED,
                _ => Rgbw::WHITE,
            },
            Palette::Solid(col) => col,
            Palette::Split(col0, _col1) => col0,
        }
    }

    fn color1(self, s: &mut State, fr: f32) -> Rgbw {
        match self {
            Palette::Split(_col0, col1) => col1,
            _ => self.color0(s, fr),
        }
    }
}

///////////////////////// WHIRL ////////////////////////

enum WhirlState {
    FullyResetting { pitch: f32, yaw: f32 },
    ReadyingSubrotation { pitch: f32, yaw: f32 },
    DoingSubrotation { pitch: f32, yaw: f32, percentage: f32 },
}

impl WhirlState {
    fn from_angle(angle: f32) -> Self {
        let pitch = 0.4;
        let full_reset_sector = 0.25;
        if angle > 1.0 - full_reset_sector {
            WhirlState::FullyResetting { pitch, yaw: 1.0 }
        } else {
            let rot_angle = angle / (1.0 - full_reset_sector);
            WhirlState::DoingSubrotation { pitch, yaw: 1.0 - rot_angle, percentage: rot_angle }
        }
    }

    fn to_env(&self) -> f32 {
        let warmup = 0.1;
        match &self {
            WhirlState::FullyResetting { .. } => 0.0,
            WhirlState::ReadyingSubrotation { .. } => 0.0,
            WhirlState::DoingSubrotation { percentage, .. } => {
                if *percentage < warmup {
                    0.0
                } else {
                    ((percentage - warmup) / (1.0 - warmup)).trapazoid(1.0, 1.0 / 16.0).powf(2.0)
                }
            }
        }
    }

    fn from_angle_top_only(angle: f32) -> Self {
        let full_reset_sector = 0.125;
        let start_of_rot1 = 1.0 - 0.25 / 1.5;

        if angle > 1.0 - full_reset_sector {
            WhirlState::FullyResetting { pitch: 0.0, yaw: start_of_rot1 }
        } else {
            let active_angle = angle / (1.0 - full_reset_sector);
            let n_rots_before_reset = 2.0;
            let subrot_idx = (active_angle * n_rots_before_reset).floor() as i32;
            let subrot_angle = active_angle.fmod_div(1.0 / n_rots_before_reset);

            // println!("Subrot idx: {subrot_idx} Active angle {active_angle} subrot sector  ");

            let subrot_start_yaw = start_of_rot1 - (subrot_idx as f32) * 0.5 / 1.5;
            let subrot_pitch = if subrot_idx % 2 == 0 { 0.25 } else { 0.75 };

            let subrot_start_sector = 0.25;

            if subrot_angle < subrot_start_sector {
                WhirlState::ReadyingSubrotation { pitch: subrot_pitch, yaw: subrot_start_yaw }
            } else {
                let movement_angle = (subrot_angle - subrot_start_sector) / (1.0 - subrot_start_sector);
                let subrot_end_yaw = subrot_start_yaw - 0.5 / 1.5;
                let yaw = subrot_start_yaw + subrot_angle * (subrot_end_yaw - subrot_start_yaw);
                WhirlState::DoingSubrotation { pitch: subrot_pitch, yaw, percentage: movement_angle }
            }
        }
    }
}

///////////////////////// BEAM PATTERNS /////////////////////////

#[derive(Clone, Copy, Debug)]
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
    fn apply(self, s: &mut State, pd: Pd, beam: &mut Beam, i: usize, fr: f32) {
        let (pitch, yaw) = self.angles(s, pd, i, fr);
        beam.pitch = pitch;
        beam.yaw = yaw;
    }

    /// Calculate (pitch, yaw) for the given pattern
    fn angles(self, s: &mut State, pd: Pd, i: usize, fr: f32) -> (f32, f32) {
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
                let angle = (s.pd(pd) + fr * 1.5) % 1.0;
                match WhirlState::from_angle(angle) {
                    WhirlState::FullyResetting { pitch, yaw } => (pitch, yaw),
                    WhirlState::ReadyingSubrotation { pitch, yaw } => (pitch, yaw),
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

///////////////////////// MANUAL BEAT /////////////////////////

#[derive(Clone, Copy)]
pub struct ManualBeat {
    /// Time of left press
    t0: f32,
    /// Time of right press
    t1: f32,

    /// Duration of left beat.
    pd0: Pd,
    /// Duration of right beat.
    pd1: Pd,

    /// Brightness range of flash.
    r: Range,
}

///////////////////////// SPIDER PATTERNS /////////////////////////

#[derive(Clone, Copy, Debug)]
pub enum SpiderPattern {
    Up,
    Down,
    Wave { pd: Pd },
    Alternate { pd: Pd },
    Snap { pd: Pd },
}

impl SpiderPattern {
    fn apply(self, s: &mut State, spider: &mut Spider, i: usize, fr: f32) {
        let (pos0, pos1) = self.pos(s, i, fr);
        spider.pos0 = pos0;
        spider.pos1 = pos1;
    }

    /// Calculate (pos0, pos1) for the given pattern
    fn pos(self, s: &mut State, i: usize, _fr: f32) -> (f32, f32) {
        match self {
            SpiderPattern::Up => (0.0, 0.52),
            SpiderPattern::Down => (0.67, 0.52),
            SpiderPattern::Wave { pd } => {
                let fr = s.pd(pd.mul(2)).tri(1.0);
                (fr, 1.0 - fr)
            }
            SpiderPattern::Alternate { pd } => {
                let t = s.pd(pd.mul(2));
                let t = match i {
                    0 => t,
                    _ => t.phase(1.0, 0.5),
                };
                let fr = t.tri(1.0);
                (fr, fr)
            }
            SpiderPattern::Snap { pd } => {
                let t = s.pd(pd.mul(2));
                let t = match i {
                    0 => t,
                    _ => t.phase(1.0, 0.5),
                };
                let fr = t.square(1.0, 0.5);
                (fr, fr)
            }
        }
    }
}

///////////////////////// LASER PATTERNS /////////////////////////

#[derive(Clone, Copy, Debug)]
pub enum LaserPos {
    Rotate { pd: Pd },
    WaveY { pd: Pd },
}

impl LaserPos {
    fn apply(self, s: &mut State, l: &mut Laser) {
        match self {
            LaserPos::Rotate { pd } => {
                l.on = true;
                l.pattern = LaserPattern::LineX;
                l.size = 0.66;
                l.x = 0.5;
                l.y = 0.1;
                l.rotate = s.pd(pd.mul(4)).tri(1.0);
            }
            LaserPos::WaveY { pd } => l.y = s.pd(pd),
        }
    }
}

///////////////////////// LIGHTS /////////////////////////

// impl Mode {
//     fn env(self, s: &State) -> f32 {
//         match self {
//             Mode::Off => 0.0,
//             Mode::On => 1.0,
//             Mode::Hover => s.phi(8, 1).ssin(1.0).map(-1.0..1.0, 0.3..0.7),
//             Mode::AutoBeat { pd, r } => s.pd(pd).ramp(1.0).lerp(r).in_quad(),
//             Mode::Beat { t, pd, r } => {
//                 let dt = s.t - t;
//                 let len = (60.0 / s.bpm) * pd.fr();

//                 if dt >= len {
//                     r.hi
//                 } else {
//                     (dt / len).ramp(1.0).lerp(r).in_quad()
//                 }
//             }
//             Mode::Press { fr } => fr,
//             Mode::Strobe { pd, duty } => s.pd(pd).square(1.0, duty.in_exp().lerp(1.0..0.5)),
//         }
//     }
// }

pub fn render_lights(mut s: ResMut<State>, mut l: ResMut<Lights>, mut e131: ResMut<E131>) {
    let s: &mut State = &mut *s;
    let l: &mut Lights = &mut *l;

    l.reset();

    match s.mode {
        Mode::Off => {
            l.for_each_beam(|beam, i, fr| {
                BeamPattern::Out.apply(s, Pd(4, 1), beam, i, fr);
            });
        }
        Mode::On { beams } => {
            l.split(s.palette.color0(s, 0.0), s.palette.color1(s, 0.0));

            if let Some(beams) = beams {
                let col = s.palette.color1(s, 0.0);
                l.for_each_beam(|beam, i, fr| {
                    beams.apply(s, Pd(4, 1), beam, i, fr);
                    beam.color = col;
                });
            }
        }
        Mode::AutoBeat { pd, r, beam: beam_pattern } => {
            let p = s.palette;

            let env = s.pd(pd.mul(2)).ramp(1.0).inv().lerp(r).in_quad();

            l.split(s.palette.color0(s, 0.0) * env, s.palette.color1(s, 0.0) * env);

            l.for_each_beam(|beam, i, fr| {
                // let pd_min
                beam_pattern.apply(s, pd, beam, i, fr);
                let beam_env = match beam_pattern {
                    BeamPattern::Whirl => {
                        let angle = (s.pd(pd) + fr * 1.5) % 1.0;
                        WhirlState::from_angle(angle).to_env()
                    }
                    _ => env,
                };
                beam.color = p.color0(s, 0.0) * beam_env;
            });
            l.for_each_spider(|spider, i, fr| {
                SpiderPattern::Alternate { pd: pd.mul(2) }.apply(s, spider, i, fr)
            });
        }
        Mode::Strobe { pd, duty } => {
            // let p = s.palette;

            let env = s.pd(pd.mul(2)).square(1.0, duty.in_exp().lerp(1.0..0.5));

            l.split(s.palette.color0(s, 0.0) * env, s.palette.color1(s, 0.0) * env);

            // Pars and strobes get solid color0
            // l.for_each_par(|par, i, fr| par.color = p.color0(s, fr) * env);
            // l.strobe.color = p.color0(s, 0.0).into();

            // // Beams and spiders get flashing color1
            // l.for_each_beam(|beam, i, fr| beam.color = p.color1(s, fr) * env);
            // l.for_each_bar(|bar, i, fr| bar.color = Rgb::from(p.color1(s, fr)) * env);
            // l.for_each_spider(|spider, i, fr| {
            //     spider.color0 = p.color0(s, fr);
            //     spider.color1 = p.color1(s, fr) * env;
            // });

            // l.for_each_beam(|beam, i, fr| BeamPattern::Square { pd }.apply(s, beam, i, fr));
            // l.for_each_spider(|spider, i, fr| SpiderPattern::Alternate { pd }.apply(s, spider, i, fr));

            l.for_each_beam(|beam, i, fr| BeamPattern::Square.apply(s, Pd(2, 1), beam, i, fr));
            l.for_each_spider(|spider, i, fr| {
                SpiderPattern::Alternate { pd: Pd(2, 1) }.apply(s, spider, i, fr)
            });
            l.strobe.color = Rgb::from(s.palette.color0(s, 0.0) * env);
        }
        Mode::Strobe0 { pd, duty } => {
            let p = s.palette;
            let env = s.pd(pd.mul(2)).square(1.0, duty.in_exp().lerp(1.0..0.5));
            l.split(s.palette.color0(s, 0.0) * env, Rgbw::BLACK);

            l.for_each_beam(|beam, i, fr| BeamPattern::Square.apply(s, Pd(2, 1), beam, i, fr));
            l.for_each_spider(|spider, i, fr| {
                SpiderPattern::Alternate { pd: Pd(2, 1) }.apply(s, spider, i, fr)
            });
            l.strobe.color = Rgb::from(s.palette.color0(s, 0.0) * env);
        }
        Mode::Strobe1 { pd, duty } => {
            let p = s.palette;
            let env = s.pd(pd.mul(2)).square(1.0, duty.in_exp().lerp(1.0..0.5));
            l.split(Rgbw::BLACK, s.palette.color0(s, 0.0) * env);

            l.for_each_beam(|beam, i, fr| BeamPattern::Square.apply(s, Pd(2, 1), beam, i, fr));
            l.for_each_spider(|spider, i, fr| {
                SpiderPattern::Alternate { pd: Pd(2, 1) }.apply(s, spider, i, fr)
            });
            l.strobe.color = Rgb::from(s.palette.color0(s, 0.0) * env);
        }
        Mode::Whirl { pd } => {
            // let p = s.palette;
            let col = s.palette.color0(s, 0.0);
            // l.map_colors(|_| s.palette.color0(s, 0.0));
            l.for_each_beam(|beam, i, fr| BeamPattern::Whirl.apply(s, pd, beam, i, fr));
            l.for_each_beam(|beam, i, fr| {
                let angle = (s.pd(pd) + fr * 1.5) % 1.0;
                let warmup = 0.1;
                let env0 = match WhirlState::from_angle(angle) {
                    WhirlState::FullyResetting { .. } => 0.0,
                    WhirlState::ReadyingSubrotation { .. } => 0.0,
                    WhirlState::DoingSubrotation { percentage, .. } => {
                        if percentage < warmup {
                            0.0
                        } else {
                            ((percentage - warmup) / (1.0 - warmup)).trapazoid(1.0, 1.0 / 16.0).powf(2.0)
                        }
                    }
                };
                beam.color = col * env0;
            });
        }
        Mode::Chase { pd, beam: beam_pattern } => {
            l.for_each_par(|par, i, fr| {
                par.color = Rgbw::WHITE * s.pd(pd.mul(4)).phase(1.0, fr).square(1.0, 0.1)
            });
            l.for_each_beam(|beam, i, fr| {
                beam.color = Rgbw::WHITE * s.pd(pd.mul(4)).phase(1.0, fr).square(1.0, 0.1);
                beam_pattern.apply(s, Pd(1, 2), beam, i, fr);
            });
            l.strobe.color = Rgb::WHITE * s.pd(pd.mul(4)).phase(1.0, 0.0).square(1.0, 0.1);
        }
        Mode::ChaseSmooth { pd, beam: beam_pattern } => {
            let color = s.palette.color0(s, s.pd(pd));
            l.for_each_par(|par, i, fr| par.color = color * s.pd(pd.mul(4)).phase(1.0, fr).tri(1.0));
            l.for_each_beam(|beam, i, fr| {
                beam.color = color * s.pd(pd.mul(4)).phase(1.0, fr).tri(1.0);
                beam_pattern.apply(s, Pd(4, 1), beam, i, fr);
            });
        }
        Mode::ChaseNotColorful { pd } => {
            let col0 = s.palette.color0(s, 0.0);
            let col1 = s.palette.color1(s, 0.0);
            // l.for_each_par(|par, i, fr| {
            //     par.color = Rgbw::WHITE * s.phi.fmod_div(pd.mul(4).fr() + fr * 4.3).phase(1.0, fr).square(1.0, 0.3);
            // });
            l.for_each_beam(|beam, i, fr| {
                let offset = if i < 2 { 0.0 } else { 0.5 };
                beam.color = col0 * s.pd(pd).phase(1.0, offset).square(1.0, 0.33);
                // let base = if i % 2 == 0 { col0 } else { col1 };
                // beam.color = Rgbw::WHITE * s.pd(pd.mul(4)).phase(1.0, fr).square(1.0, 1.0 / (10. + fr * 20.));
                BeamPattern::Cross {
                    pitch: (1. - s.pd(pd.mul(8)).fsin(1.)) * 0.3 + 0.1,
                    angle: Some(s.pd(pd.mul(8)).fsin(1.) * 0.2 - 0.1),
                    fanning: Some(1.5),
                }
                .apply(s, pd, beam, i, fr);
            });
            //
        }
        Mode::RaisingBeams { pd } => {
            // let angle = (s.pd(pd) + fr * 2.0) % 1.0;
            let col = s.palette.color0(s, 0.0);
            l.for_each_beam(|beam, i, fr| {
                BeamPattern::RaisingBeams.apply(s, pd, beam, i, fr);
                let angle = (s.pd(pd) + fr * 2.0) % 1.0;
                // // let
                // let pitch = if angle < 0.5 {
                //     (0.5 - angle)
                // } else if angle < 0.75 {
                //     (0.6)
                // } else {
                //     (0.5 - (angle - 0.9))
                // };
                let env = if angle < 0.45 { (angle - 0.1).trapazoid(0.5, 0.1) } else { 0.0 };

                beam.color = col * env;
            });
        }
        Mode::Break { beams } => {
            if let Some(beams) = beams {
                let col = s.palette.color0(s, 0.0);
                l.for_each_beam(|beam, i, fr| {
                    beams.apply(s, Pd(4, 1), beam, i, fr);
                    beam.color = col;
                });
            }
        }
        _ => {}
    }

    // Global brightness
    l.map_colors(|c| c * s.brightness);

    if let Some(ManualBeat { t0, t1, pd0, pd1, r }) = s.beat {
        let fr0 = {
            let dt = s.t - t0;
            let len = (60.0 / s.bpm) * pd0.fr();

            if dt >= len { r.hi } else { (dt / len).ramp(1.0).lerp(r).in_quad() }
        };

        let fr1 = {
            let dt = s.t - t1;
            let len = (60.0 / s.bpm) * pd1.fr();

            if dt >= len { r.hi } else { (dt / len).ramp(1.0).lerp(r).in_quad() }
        };

        l.for_each_par(|par, i, fr| par.color = par.color * fr0);
        l.for_each_beam(|beam, i, fr| beam.color = beam.color * fr1);
        l.for_each_spider(|spider, i, fr| {
            spider.color0 = spider.color0 * fr1;
            spider.color1 = spider.color1 * fr1;
        });
        l.for_each_bar(|bar, i, fr| bar.color = bar.color * fr1);
        l.strobe.color = l.strobe.color * fr0;
    }

    l.laser.size = 0.75;
    l.laser.pattern = LaserPattern::LineX;
    l.laser.y = 0.375;
    l.laser.x = s.pd(Pd(4, 1)).tri(1.0) + 0.25 * 0.25;
    l.laser.color = LaserColor::from_rgb(s.palette.color0(s, 0.0).into());
    //l.laser.color = LaserColor::RGB;

    // for b in &mut l.beams {
    //     b.pitch = s.test4;
    //     // b.yaw = s.test1;
    // }
    // l.beams[0].pitch = s.test2;
    // l.beams[1].pitch = s.test3;
    // l.beams[2].pitch = s.test3;
    // l.beams[3].pitch = s.test2;
    // // if s.test0 < 0.55
    // s.test0 = s.test0.min(0.66);
    // s.test1 = s.test1.min(0.66);
    // l.beams[0].yaw = s.test0;
    // l.beams[1].yaw = s.test1;
    // if s.test1 < 0.66 {
    //     l.beams[2].yaw = (0.66 - s.test1);
    // }
    // if s.test0 < 0.66 {
    //     l.beams[3].yaw = (0.66 - s.test0);
    // }
    // l.beams[2].yaw = s.test2;
    // l.beams[3].yaw = s.test3;

    l.send(&mut *e131);
}

///////////////////////// PAD /////////////////////////

pub fn render_pad(mut s: ResMut<State>, mut pad: ResMut<Midi<LaunchpadX>>) {
    let s: &mut State = &mut *s;

    use launchpad_x::types::*;
    use launchpad_x::*;

    use self::Mode;

    let mut batch: Vec<(Pos, Color)> = vec![];

    // Helper to set an x/y coord to a certain color
    let rgb = |Rgb(r, g, b): Rgb| Color::Rgb(r, g, b);
    let mut set = |x, y, color: Rgb| batch.push((Coord(x, y).into(), rgb(color)));

    if s.debug {
        let color0: Rgb = s.palette.color0(s, 0.0).into();
        let color1: Rgb = s.palette.color1(s, 0.0).into();

        // mod colors
        // rgb(2, 6, Rgb::BLACK);
        // rgb(3, 6, Rgb::WHITE);
        // rgb(4, 6, Rgb::hsv(s.phi(16, 1), 1.0, 1.0));
        // rgb(5, 6, Rgb::WHITE);

        // y=0: Lights off, or a brief pause/break
        set(1, 0, Rgb::BLACK);
        set(2, 0, Rgb::BLACK);
        set(3, 0, Rgb::BLACK);
        set(4, 0, Rgb::BLACK);
        set(5, 0, Rgb::BLACK);
        set(6, 0, Rgb::BLACK);

        // y=1: Solid patterns
        set(1, 1, color0);
        set(2, 1, color0);
        set(3, 1, color0);
        set(4, 1, color1);
        set(5, 1, color1);
        set(6, 1, color1);

        let beat = |pd: Pd| s.pd(pd.mul(4)).ramp(1.0).inv().in_quad();
        let beat11 = beat(Pd(1, 1));
        let beat12 = beat(Pd(1, 2));
        let beat14 = beat(Pd(1, 4));
        let beat116 = beat(Pd(1, 16));
        let beat132 = beat(Pd(1, 32));

        // y=2: Pd(1, 1) patterns
        set(1, 2, color0 * beat11);
        set(2, 2, color0 * beat11);
        set(3, 2, color0 * beat11);
        set(4, 2, color1 * beat11);
        set(5, 2, color1 * beat11);
        set(6, 2, color1 * beat11);

        // y=3: Pd(1, 2) patterns
        set(1, 3, color0 * beat12);
        set(2, 3, color0 * beat12);
        set(3, 3, color0 * beat12);
        set(4, 3, color1 * beat12);
        set(5, 3, color1 * beat12);
        set(6, 3, color1 * beat12);

        // y=4: Pd(1, 4) patterns
        set(1, 4, color0 * beat14);
        set(2, 4, color0 * beat14);
        set(3, 4, color0 * beat14);
        set(4, 4, color1 * beat14);
        set(5, 4, color1 * beat14);
        set(6, 4, color1 * beat14);

        // y=5: Strobes
        set(0, 5, color0 * beat116);
        set(1, 5, color0 * beat116);
        set(2, 5, color0 * beat116);
        set(3, 5, color0 * beat116);
        set(4, 5, color1 * beat116);
        set(5, 5, Rgb::WHITE * beat116);
        set(6, 5, Rgb::WHITE * beat132);
        set(7, 5, Rgb::WHITE * beat132);

        // y=6, y=7: Colorz
        // Sidez
        set(0, 6, Rgb::WHITE);
        set(7, 6, Rgb::WHITE);
        // Redz
        set(1, 6, Rgb::RED);
        set(2, 6, Rgb::RED * 0.5);
        set(3, 6, Rgb::RED * 0.5);
        set(1, 7, Rgb::WHITE);
        set(2, 7, Rgb::WHITE);
        set(3, 7, Palette::RedWhiteOsc.color0(s, 0.0).into());
        // Greenz n Bluez
        set(4, 6, Rgb::LIME);
        set(4, 7, Rgb::WHITE);
        set(5, 6, Rgb::BLUE);
        set(5, 7, Rgb::LIME);
        set(6, 6, Rgb::BLUE);
        set(6, 7, Rgb::WHITE);

        // Left and right edges: manual beat buttons
        for i in 0..=4 {
            // Upwards propagating wave at BPM
            let col = Rgb::WHITE * (s.phi - i as f32 * 0.2).fsin(2.0);
            set(0, i, col);
            set(7, i, col);
        }

        // Top and right outer buttons: alpha selectors (TODO)
        for i in 0..=7 {
            set(i, 8, Rgb::WHITE);
            set(8, i, Rgb::VIOLET);
        }

        // Top left/right: beatmatch buttons
        set(0, 7, Rgb::VIOLET);
        set(7, 7, Rgb::VIOLET);
    } else {
        if let Some(ManualBeat { t0, t1, pd0, pd1, r }) = s.beat {
            for i in 0..8 {
                for j in 0..8 {
                    let fr0 = {
                        let dt = s.t - t0;
                        let len = (60.0 / s.bpm) * pd0.div(2).fr();

                        let ofs = i as f32 / 8.0;
                        let t = (dt / len) - ofs + 0.0;
                        if t < 0.0 {
                            0.0
                        } else if t > 1.0 {
                            0.0
                        } else {
                            t.ramp(1.0).lerp(r).in_quad()
                        }
                    };

                    let fr1 = {
                        let dt = s.t - t1;
                        let len = (60.0 / s.bpm) * pd1.div(2).fr();

                        let ofs = 1.0 - (i as f32 / 8.0);
                        let t = (dt / len) - ofs + 0.125;
                        if t < 0.0 {
                            0.0
                        } else if t > 1.0 {
                            0.0
                        } else {
                            t.ramp(1.0).lerp(r).in_quad()
                        }
                    };

                    let fr = fr0.max(fr1);

                    //let col0 = s.palette.color0(s, 0.0) * (s.phi - i as f32 * 0.125).fsin(2.0).inout_exp();
                    //let col1 = s.palette.color0(s, 0.0) * (s.phi - i as f32 * 0.125).fsin(2.0).inout_exp();

                    set(i, j, Rgb::from(s.palette.color0(s, 0.0)) * fr);
                }
            }
        } else {
            match s.mode {
                Mode::On { .. } => {
                    let color = s.palette.color0(s, 0.0);
                    for i in 0..8 {
                        for j in 0..8 {
                            set(i, j, color.into());
                        }
                    }
                }
                Mode::AutoBeat { .. } => {
                    match s.x {
                        1 => {
                            // Upwards propagating wave at BPM
                            for i in 0..8 {
                                let col = s.palette.color0(s, 0.0)
                                    * (s.phi - i as f32 * 0.125).fsin(2.0).inout_exp();
                                for j in 0..8 {
                                    set(j, i, col.into());
                                }
                            }
                        }
                        2 => {
                            // Sideways propagating wave at BPM
                            for i in 0..8 {
                                let col = s.palette.color0(s, 0.0)
                                    * (s.phi - i as f32 * 0.125).fsin(2.0).inout_exp();
                                for j in 0..8 {
                                    set(i, j, col.into());
                                }
                            }
                        }
                        3 => {
                            // Sideways staggered propagating wave at BPM
                            for i in 0..8 {
                                for j in 0..8 {
                                    let col = s.palette.color0(s, 0.0)
                                        * (s.phi - i as f32 * 0.125 + j as f32 * 0.125).fsin(2.0).in_quad();
                                    set(i, j, col.into());
                                }
                            }
                        }
                        4 => {
                            // Sideways staggered propagating wave at BPM
                            for i in 0..8 {
                                for j in 0..8 {
                                    let col = s.palette.color0(s, 0.0)
                                        * (s.phi - i as f32 * 0.125 + j as f32 * 0.125).fsin(2.0).in_quad();
                                    set(j, i, col.into());
                                }
                            }
                        }
                        5 => {
                            // Whirl
                            for x in 0..8 {
                                for y in 0..8 {
                                    set(x, y, Rgb::from(s.palette.color0(s, 0.0)) * spiral(s.t, x, y, -8.0));
                                }
                            }
                        }
                        _ => {
                            // Downards propagating wave at BPM
                            for i in 0..8 {
                                let col = s.palette.color0(s, 0.0)
                                    * (s.phi + i as f32 * 0.125).fsin(2.0).inout_exp();
                                for j in 0..8 {
                                    set(j, i, col.into());
                                }
                            }
                        }
                    }
                }
                Mode::Whirl { .. } => {
                    for x in 0..8 {
                        for y in 0..8 {
                            set(x, y, Rgb::from(s.palette.color0(s, 0.0)) * spiral(s.t, x, y, 8.0));
                        }
                    }
                }
                Mode::RaisingBeams { .. }
                | Mode::Break { beams: Some(BeamPattern::WaveY | BeamPattern::UpDownWave) } => {
                    // Up/down wave
                    // let y0 = (.floor() as i8;
                    for x in 0..8 {
                        for y in 0..8 {
                            let fr = y as f32 / 8.0;
                            let env = s.phi(4, 1).ramp(1.0).phase(1.0, fr * 0.5).out_exp();
                            set(x, y, Rgb::from(s.palette.color0(s, 0.0)) * (1.0 - env));
                            // if y == y0 {
                            //     set(x, y, s.palette.color0(s, 0.0).into());
                            // } else {
                            //     set(x, y, Rgb::BLACK);
                            // }
                        }
                    }
                }
                Mode::Strobe { pd, duty } | Mode::Strobe0 { pd, duty } | Mode::Strobe1 { pd, duty } => {
                    let env = s.pd(pd).square(1.0, duty.in_exp().lerp(1.0..0.5));
                    let col = Rgb::from(s.palette.color0(s, 0.0)) * env;

                    // Solid strobe
                    for i in 0..8 {
                        for j in 0..8 {
                            set(i, j, col);
                        }
                    }
                }
                Mode::Chase { pd, .. } => {
                    let env = s.pd(pd).square(1.0, 0.6);
                    for x in 0..8 {
                        for y in 0..8 {
                            set(x, y, Rgb::WHITE * spiral(s.t, x, y, 12.0) * env);
                        }
                    }
                }
                Mode::ChaseNotColorful { .. } => {
                    let col = Rgb::from(s.palette.color0(s, 0.0));
                    for x in 0..8 {
                        for y in 0..8 {
                            set(x, y, col * spiral(s.t, x, y, 12.0));
                        }
                    }
                }
                _ => {
                    for i in 0..8 {
                        for j in 0..8 {
                            set(i, j, Rgb::BLACK);
                        }
                    }
                }
            }
        }
    }

    // Beat indicator
    set(
        8,
        8,
        match s.pd(Pd(1, 1)).bsquare(1.0, 0.1) {
            true => match s.pd(Pd(4, 1)).bsquare(1.0, 0.2) {
                // Purple on the first beat of each bar
                true => Rgb::VIOLET,
                // White on every other beat
                false => Rgb::WHITE,
            },
            false => Rgb::BLACK,
        },
    );

    pad.send(Output::Batch(batch));
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

///////////////////////// CTRL /////////////////////////

// #[allow(unused)]
// pub fn render_ctrl(mut s: ResMut<State>, ctrl: ResMut<Midi<LaunchControlXL>>) {
//     use launch_control_xl::types::*;
//     use launch_control_xl::*;

//     use self::Mode;
// }

///////////////////////// TICK /////////////////////////

pub fn tick(mut s: ResMut<State>, mut l: ResMut<Lights>, time: Res<Time>) {
    let s: &mut State = &mut *s;
    let l: &mut Lights = &mut *l;
    let dt = time.delta_secs();

    s.dt = dt;
    s.t += dt;
    s.phi = (s.phi + (dt * (s.bpm / 60.0) * s.phi_mul)).fmod(16.0);

    if s.preset {
        let phi = (s.phi(16, 1) * 4.0) as usize;
        if phi % 4 == 0 {
            if !s.preset_switched {
                info!("SWITCH");
                s.preset_switched = true;
                match ThreadRng::default().gen_range(1..=6) {
                    1 => s.mode = Mode::ChaseSmooth { pd: Pd(1, 1), beam: BeamPattern::WaveY },
                    2 => {
                        s.mode = Mode::AutoBeat {
                            pd: Pd(4, 1),
                            r: (0.2..1.0).into(),
                            beam: BeamPattern::RaisingBeams,
                        }
                    }
                    3 => {
                        s.mode =
                            Mode::AutoBeat { pd: Pd(4, 1), r: (0.2..1.0).into(), beam: BeamPattern::WaveY }
                    }
                    4 => {
                        s.mode = Mode::AutoBeat {
                            pd: Pd(4, 1),
                            r: (0.2..1.0).into(),
                            beam: BeamPattern::UpDownWave,
                        }
                    }
                    5 => {
                        s.mode =
                            Mode::AutoBeat { pd: Pd(4, 1), r: (0.2..1.0).into(), beam: BeamPattern::Whirl }
                    }
                    6 | _ => {
                        s.mode =
                            Mode::AutoBeat { pd: Pd(4, 1), r: (0.2..1.0).into(), beam: BeamPattern::Twisting }
                    }
                }
            }
        } else {
            s.preset_switched = false;
        }
    }
}

///////////////////////// PAD INPUT /////////////////////////

pub fn on_pad(mut s: ResMut<State>, mut l: ResMut<Lights>, mut pad: ResMut<Midi<LaunchpadX>>) {
    let s: &mut State = &mut *s;
    let l: &mut Lights = &mut *l;

    use launchpad_x::types::*;
    use launchpad_x::*;

    for event in pad.recv() {
        use self::Mode;
        // debug!("pad: {event:?}");

        // Toggle preset mode, lockout other inputs
        if let Input::Custom(true) = event {
            s.preset = !s.preset;
            info!("preset={}", s.preset);
        }
        if s.preset {
            return;
        }

        match event {
            // Toggle debug mode
            Input::Capture(true) => {
                s.debug = !s.debug;
                pad.send(Output::Clear);
            }
            // Toggle laser
            Input::Custom(true) => l.laser.on = !l.laser.on,
            // Brightness
            Input::Record(true) => s.brightness = 0.07,
            Input::Solo(true) => s.brightness = 0.1,
            Input::Mute(true) => s.brightness = 0.125,
            Input::Stop(true) => s.brightness = 0.3,
            Input::B(true) => s.brightness = 0.4,
            Input::A(true) => s.brightness = 0.6,
            Input::Pan(true) => s.brightness = 0.8,
            Input::Volume(true) => s.brightness = 1.0,
            // half/double/normal time
            Input::Up(true) => s.phi_mul = 2.0,
            Input::Down(true) => s.phi_mul = 0.5,
            Input::Left(true) => s.phi_mul = 1.0,
            _ => {}
        }

        let beat0 = |pd: Pd, s: &mut State, r: Range| match &mut s.beat {
            Some(ManualBeat { t0, t1, pd0, pd1, r }) => {
                *t0 = s.t;
                *pd0 = pd;
            }
            None => s.beat = Some(ManualBeat { t0: s.t, t1: 0.0, pd0: pd, pd1: pd, r }),
        };
        let beat1 = |pd: Pd, s: &mut State, r: Range| match &mut s.beat {
            Some(ManualBeat { t1, pd1, .. }) => {
                *t1 = s.t;
                *pd1 = pd;
            }
            _ => s.beat = Some(ManualBeat { t0: 0.0, t1: s.t, pd0: pd, pd1: pd, r }),
        };

        // First match on x/y presses only.
        if let Some((x, y)) = match event {
            Input::Press(i, _) => Some((Coord::from(i).0, Coord::from(i).1)),
            _ => None,
        } {
            info!("Pad({x}, {y})");
            s.x = x;
            s.y = y;

            s.phi_mul = 1.0;

            let is_left_beat = x == 0 && y < 5;
            let is_right_beat = x == 7 && y < 5;
            let is_color = y == 6 || y == 7 && x > 0 && x < 7;
            let is_side_color = (x == 0 && y == 6) || (x == 7 && y == 6);
            if !is_left_beat && !is_right_beat && !is_color && !is_side_color {
                s.beat = None;
            }

            match (x, y) {
                // Beatmatch
                (0, 7) => s.bpm_taps.push(s.t),
                // Beatmatch apply
                (7, 7) => match s.bpm_taps.len() {
                    // If no beats, just reset phase
                    0 => s.phi = 0.0,
                    1 => s.bpm_taps.clear(),
                    n => {
                        // Calculate time difference between each consecutive tap
                        let dts = s.bpm_taps.drain(..).tuple_windows().map(|(t0, t1)| t1 - t0);
                        // Average out the difference
                        let dt = dts.sum::<f32>() / (n as f32 - 1.0);
                        // Calculate BPM
                        let bpm = 60.0 / dt;

                        s.phi = 0.0;
                        s.bpm = bpm;
                        info!("Calculated bpm={bpm:.2} from {n} samples");
                    }
                },

                // Manual beats
                (0, 0) => beat0(Pd(4, 1), s, (1.0..0.0).into()),
                (0, 1) => beat0(Pd(2, 1), s, (1.0..0.0).into()),
                (0, 2) => beat0(Pd(1, 1), s, (1.0..0.0).into()),
                (0, 3) => beat0(Pd(1, 2), s, (1.0..0.0).into()),
                (0, 4) => beat0(Pd(1, 4), s, (1.0..0.0).into()),
                (7, 0) => beat1(Pd(4, 1), s, (1.0..0.0).into()),
                (7, 1) => beat1(Pd(2, 1), s, (1.0..0.0).into()),
                (7, 2) => beat1(Pd(1, 1), s, (1.0..0.0).into()),
                (7, 3) => beat1(Pd(1, 2), s, (1.0..0.0).into()),
                (7, 4) => beat1(Pd(1, 4), s, (1.0..0.0).into()),

                // y=0: Lights off, or a brief pause/break
                (1, 0) => s.mode = Mode::Off,
                (2, 0) => s.mode = Mode::Break { beams: Some(BeamPattern::Out) },
                (3, 0) => s.mode = Mode::Break { beams: Some(BeamPattern::WaveY) },
                (4, 0) => s.mode = Mode::RaisingBeams { pd: Pd(8, 1) },
                (5, 0) => s.mode = Mode::Whirl { pd: Pd(16, 1) },
                (6, 0) => s.mode = Mode::Break { beams: Some(BeamPattern::UpDownWave) },

                // y=1: Solid patterns
                (1, 1) => s.mode = Mode::On { beams: Some(BeamPattern::Out) },
                (2, 1) => s.mode = Mode::On { beams: Some(BeamPattern::Out) },
                (3, 1) => s.mode = Mode::On { beams: Some(BeamPattern::WaveY) },
                (4, 1) => s.mode = Mode::On { beams: Some(BeamPattern::SnapX) },
                (5, 1) => s.mode = Mode::On { beams: Some(BeamPattern::Whirl) },
                (6, 1) => s.mode = Mode::On { beams: Some(BeamPattern::Twisting) },

                (1, 2) => s.mode = Mode::ChaseSmooth { pd: Pd(1, 1), beam: BeamPattern::WaveY },
                (2, 2) => {
                    s.mode =
                        Mode::AutoBeat { pd: Pd(4, 1), r: (0.2..1.0).into(), beam: BeamPattern::RaisingBeams }
                }
                (3, 2) => {
                    s.mode = Mode::AutoBeat { pd: Pd(4, 1), r: (0.2..1.0).into(), beam: BeamPattern::WaveY }
                }
                (4, 2) => {
                    s.mode =
                        Mode::AutoBeat { pd: Pd(4, 1), r: (0.2..1.0).into(), beam: BeamPattern::UpDownWave }
                }
                (5, 2) => {
                    s.mode = Mode::AutoBeat { pd: Pd(4, 1), r: (0.2..1.0).into(), beam: BeamPattern::Whirl }
                }
                (6, 2) => {
                    s.mode =
                        Mode::AutoBeat { pd: Pd(4, 1), r: (0.2..1.0).into(), beam: BeamPattern::Twisting }
                }

                // y=3: Pd(2, 1) patterns
                (1, 3) => s.mode = Mode::ChaseSmooth { pd: Pd(1, 2), beam: BeamPattern::Square },
                // (1, 3) => s.mode = Mode::AutoBeat { pd: Pd(2, 1), r: (0.2..1.0).into(), beam: BeamPattern::Square },
                (2, 3) => {
                    s.mode =
                        Mode::AutoBeat { pd: Pd(2, 1), r: (0.2..1.0).into(), beam: BeamPattern::RaisingBeams }
                }
                (3, 3) => {
                    s.mode = Mode::AutoBeat { pd: Pd(2, 1), r: (0.2..1.0).into(), beam: BeamPattern::WaveY }
                }
                (4, 3) => {
                    s.mode =
                        Mode::AutoBeat { pd: Pd(2, 1), r: (0.2..1.0).into(), beam: BeamPattern::UpDownWave }
                }
                (5, 3) => {
                    s.mode = Mode::AutoBeat { pd: Pd(2, 1), r: (0.2..1.0).into(), beam: BeamPattern::Whirl }
                }
                (6, 3) => {
                    s.mode =
                        Mode::AutoBeat { pd: Pd(2, 1), r: (0.2..1.0).into(), beam: BeamPattern::Twisting }
                }

                // y=4: Pd(1, 1) patterns
                (1, 4) => {
                    s.mode = Mode::AutoBeat { pd: Pd(1, 1), r: (0.2..1.0).into(), beam: BeamPattern::Square }
                }
                (2, 4) => {
                    s.mode =
                        Mode::AutoBeat { pd: Pd(1, 1), r: (0.2..1.0).into(), beam: BeamPattern::RaisingBeams }
                }
                (3, 4) => {
                    s.mode = Mode::AutoBeat { pd: Pd(1, 1), r: (0.2..1.0).into(), beam: BeamPattern::WaveY }
                }
                (4, 4) => {
                    s.mode =
                        Mode::AutoBeat { pd: Pd(1, 1), r: (0.2..1.0).into(), beam: BeamPattern::UpDownWave }
                }
                (5, 4) => {
                    s.mode = Mode::AutoBeat { pd: Pd(1, 1), r: (0.2..1.0).into(), beam: BeamPattern::Whirl }
                }
                (6, 4) => {
                    s.mode =
                        Mode::AutoBeat { pd: Pd(1, 1), r: (0.2..1.0).into(), beam: BeamPattern::Twisting }
                }

                // y=5: Strobes
                (0, 5) => s.mode = Mode::Strobe0 { pd: Pd(1, 8), duty: 1.0 },
                (1, 5) => s.mode = Mode::ChaseNotColorful { pd: Pd(1, 4) },
                (2, 5) => s.mode = Mode::Strobe { pd: Pd(1, 4), duty: 1.0 },
                (3, 5) => s.mode = Mode::Strobe { pd: Pd(1, 8), duty: 1.0 },
                (4, 5) => s.mode = Mode::Strobe { pd: Pd(1, 8), duty: 1.0 },
                (5, 5) => s.mode = Mode::Chase { pd: Pd(1, 1), beam: BeamPattern::Twisting },
                (6, 5) => s.mode = Mode::Chase { pd: Pd(1, 2), beam: BeamPattern::Twisting },
                (7, 5) => s.mode = Mode::Chase { pd: Pd(1, 4), beam: BeamPattern::Twisting },

                // y=?: Strobes

                //(2, 0) => s.mode = Mode::
                // (1, 1) => s.mode = Mode::On,
                // (1, 2) => s.mode = Mode::Hover,
                // (1, 3) => s.mode = Mode::AutoBeat { pd: Pd(2, 1), r: (0.2..1.0).into() },
                // (1, 4) => s.mode = Mode::AutoBeat { pd: Pd(1, 1), r: (0.2..1.0).into() },

                // (6, 0) => s.mode = Mode::Off,
                // (6, 1) => s.mode = Mode::On,
                // (6, 2) => s.mode = Mode::Hover,
                // (6, 3) => s.mode = Mode::AutoBeat { pd: Pd(2, 1), r: (0.2..1.0).into() },
                // (6, 4) => s.mode = Mode::AutoBeat { pd: Pd(1, 1), r: (0.2..1.0).into() },

                // (7, 5) => s.mode = Mode::AutoBeat { pd: Pd(1, 4), r: (0.0..1.0).into() },
                // (7, 6) => s.mode = Mode::Strobe { pd: Pd(1, 8), duty: 1.0 },
                // (0, 5) => s.mode = Mode::AutoBeat { pd: Pd(1, 4), r: (0.0..1.0).into() },
                // (0, 6) => s.mode = Mode::Strobe { pd: Pd(1, 8), duty: 1.0 },
                // (6, 7) => s.mode = Mode::Whirl { pd: Pd(16, 1) },

                // slow presets
                // (3, 0) => {
                //     s.env(|_| 0.1);
                //     l.beam_pos = BeamPos::Down;
                // },
                // (3, 1) => {},
                // (3, 2) => {
                //     s.env(|s| s.phi(1, 1).ramp(1.0).inv().lerp(0.2..0.3));
                //     l.beam_pos = BeamPos::WaveY { pd: Pd(8, 1) };
                // },

                // // fast presets
                // (4, 0) => {
                //     s.env(|_| 0.4);
                //     l.beam_pos = BeamPos::WaveY { pd: Pd(8, 1) };
                // },
                // (4, 1) => {},
                // (4, 2) => {
                //     s.env(|s| s.phi(1, 1).ramp(1.0).inv().lerp(0.2..0.5));
                //     l.beam_pos = BeamPos::Square { pd: Pd(8, 1) };
                // },

                // Colorz
                // Sidez
                (0, 6) => s.palette = Palette::Solid(Rgbw::WHITE),
                (7, 6) => s.palette = Palette::Solid(Rgbw::RGBW),
                // Redz
                (1, 6) => s.palette = Palette::Solid(Rgbw::RED),
                (2, 6) => s.palette = Palette::Split(Rgbw::RED, Rgbw::BLACK),
                (3, 6) => s.palette = Palette::Split(Rgbw::BLACK, Rgbw::RED),
                (1, 7) => s.palette = Palette::Split(Rgbw::RED, Rgbw::WHITE),
                (2, 7) => s.palette = Palette::Split(Rgbw::WHITE, Rgbw::RED),
                (3, 7) => s.palette = Palette::RedWhiteOsc,
                // Greenz n Bluez
                (4, 6) => s.palette = Palette::Solid(Rgbw::LIME),
                (4, 7) => s.palette = Palette::Split(Rgbw::LIME, Rgbw::WHITE),
                (5, 6) => s.palette = Palette::Solid(Rgbw::BLUE),
                (5, 7) => s.palette = Palette::Split(Rgbw::LIME, Rgbw::BLUE),
                (6, 6) => s.palette = Palette::Split(Rgbw::BLUE, Rgbw::WHITE),
                (6, 7) => s.palette = Palette::Split(Rgbw::WHITE, Rgbw::BLUE),

                // hold pressure env
                // (6, 2) => s.beat0 = Beat::Fr(fr.in_exp()),
                // (7, 2) => s.beat0 = Beat::Fr(fr.in_exp()),

                // hold mod colors
                // (5, 1) => {
                //     s.c_h.hold(x, y, b, Op::f(|s| Rgbw::hsv(s.pd(Pd(4, 1)), 1.0, 1.0)));
                //     s.env_h.hold(x, y, b, Op::v(1.0));
                // },
                // (5, 2) => s.c_h.hold(x, y, b, Op::v(Rgbw::BLACK)),
                // (5, 3) => {
                //     s.c_h.hold(x, y, b, Op::v(Rgbw::WHITE));
                //     s.env_h.hold(x, y, b, Op::v(1.0));
                // },

                // hold strobe w/ pressure
                // (6, 3) => s.env_h.hold(x, y, b, Op::f(move |s| s.pd(Pd(1, 4)).square(1.0, fr.in_exp().lerp(1.0..0.5)))),
                // (7, 3) => s.env_h.hold(x, y, b, Op::f(move |s| s.pd(Pd(1, 8)).square(1.0, fr.in_exp().lerp(1.0..0.5)))),

                // hold white strobe
                // (6, 4) => {
                //     s.env_h0.hold(x, y, b, Op::f(move |s| s.pd(Pd(1, 4)).square(1.0, fr.in_exp().lerp(1.0..0.5))));
                //     s.env_h1.hold(x, y, b, Op::v(0.0));
                //     s.c_h.hold(x, y, b, Op::v(Rgbw::WHITE));
                // },
                // (7, 4) => {
                //     s.env_h.hold(x, y, b, Op::f(move |s| s.pd(Pd(1, 8)).square(1.0, fr.in_exp().lerp(1.0..0.5))));
                //     s.c_h.hold(x, y, b, Op::v(Rgbw::WHITE));
                // },

                // // hold chase
                // (6, 5) => {
                //     s.par_src_h.hold(x, y, b, Source::Chase { pd: Pd(1, 1), duty: 0.1 });
                //     s.env_h1.hold(x, y, b, Op::v(0.0));
                //     s.c_h.hold(x, y, b, Op::v(Rgbw::WHITE));
                //     s.strobe_src_h.hold(x, y, b, Source::Strobe { pd: Pd(1, 4), duty: fr.in_exp().lerp(1.0..0.5) });
                // }
                _ => {}
            }
        }

        // Next match on x/y presses *and* releases, with a bool to indicate which one
        if let Some((x, y, _b)) = match event {
            Input::Press(i, _) => Some((Coord::from(i).0, Coord::from(i).1, true)),
            Input::Release(i) => Some((Coord::from(i).0, Coord::from(i).1, false)),
            _ => None,
        } {
            match (x, y) {
                _ => {}
            }
        }
    }
}

///////////////////////// CTRL INPUT /////////////////////////

pub fn on_ctrl(mut s: ResMut<State>, mut l: ResMut<Lights>, mut ctrl: ResMut<Midi<LaunchControlXL>>) {
    let s: &mut State = &mut *s;
    let l: &mut Lights = &mut *l;

    for input in ctrl.recv() {
        use launch_control_xl::types::*;
        use launch_control_xl::*;
        debug!("ctrl: {input:?}");

        match input {
            Input::Slider(0, fr) => s.brightness = fr,

            // Input::Slider(1, fr) => s.test0 = fr,
            // Input::Slider(2, fr) => s.test1 = fr,

            // pattern=LineX
            // y=0.1
            // size=0.66
            //

            // laser tweaks
            Input::Focus(0, true) => l.laser.on = !l.laser.on,

            // Input::Slider(1, fr) => s.test0 = fr,
            // Input::Slider(2, fr) => s.test1 = fr,
            // Input::Slider(3, fr) => s.test2 = fr,
            // Input::Slider(4, fr) => s.test3 = fr,
            // Input::Slider(5, fr) => s.test4 = fr,
            Input::Slider(1, fr) => {
                l.laser.pattern = LaserPattern::Raw(fr.byte());
                println!("{:?}", l.laser.pattern);
            }
            Input::Slider(2, fr) => l.laser.rotate = fr,
            // Input::Slider(3, fr) => l.laser.x = fr,
            Input::Slider(4, fr) => l.laser.y = fr,
            Input::Slider(5, fr) => l.laser.size = fr,
            Input::Slider(6, fr) => l.laser.color = LaserColor::Raw(fr.byte()),

            Input::Slider(3, fr) => l.laser.xflip = fr,
            Input::Slider(4, fr) => l.laser.yflip = fr,
            _ => {}
        }
    }
}
