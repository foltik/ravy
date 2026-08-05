use lib::lights::fixture::{ColoradoSolo, HydroSpot, OutcastBeamwash};
use lib::prelude::*;

/// Test app for the ENTTEC DMX USB PRO rig: 2x Rogue Outcast BeamWash (37CH),
/// 2x ADJ Hydro Spot (22CH), 6x COLORado 1 Solo (STDY).
#[derive(argh::FromArgs)]
struct Args {
    /// serial port path (default: auto-detect the first FTDI port)
    #[argh(option)]
    port: Option<String>,
    /// enable debug logging
    #[argh(switch, short = 'v')]
    debug: bool,
    /// enable trace logging
    #[argh(switch, short = 'V')]
    trace: bool,
}

/// DMX start addresses, as set via `rdm.py address`.
const OUTCAST_ADDRS: [usize; 2] = [1, 38];
const HYDRO_ADDRS: [usize; 2] = [75, 97];
const COLORADO_ADDRS: [usize; 6] = [119, 136, 153, 170, 187, 204];
/// Highest occupied DMX channel (last COLORado, 204 + 17 - 1).
const LAST_CH: usize = 220;

fn main() -> Result {
    let args: Args = argh::from_env();

    let enttec = match &args.port {
        Some(path) => Enttec::open(path)?,
        None => Enttec::new()?,
    };

    App::new()
        .add_plugins(RavyPlugin { module: module_path!(), debug: args.debug, trace: args.trace })
        .add_systems(Startup, setup)
        .add_systems(Update, (tick, render).chain())
        .add_systems(EguiPrimaryContextPass, draw_ui)
        .insert_resource(enttec)
        .insert_resource(State::default())
        .run();
    Ok(())
}

fn setup(mut commands: Commands) {
    // Needed to draw the UI
    commands.spawn(Camera2d);
}

///////////////////////// STATE /////////////////////////

#[derive(Resource)]
pub struct State {
    /// Total time elapsed since startup in seconds
    pub t: f32,

    /// Master brightness applied to every dimmer.
    pub brightness: f32,
    /// Kill all output while held.
    pub blackout: bool,
    /// Per-group enables.
    pub outcasts_on: bool,
    pub hydros_on: bool,
    pub colorados_on: bool,

    pub color_test: ColorTest,
    /// Solid color RGBW sliders.
    pub solid: [f32; 4],

    pub dim_test: DimTest,
    /// Manual dimmer level.
    pub dim: f32,
    /// Ramp period in seconds.
    pub dim_pd: f32,

    pub move_test: MoveTest,
    /// Manual pan/tilt.
    pub pan: f32,
    pub tilt: f32,
    /// Circle radius in normalized pan/tilt units.
    pub circle_r: f32,
    /// Circle period in seconds.
    pub circle_pd: f32,

    /// Zoom level, or triangle sweep when enabled.
    pub zoom: f32,
    pub zoom_sweep: bool,

    /// Raw strobe chart byte for both Outcast cells.
    pub outcast_strobe: u8,
    /// Raw shutter byte for the Hydros.
    pub hydro_shutter: u8,

    /// Hydro wheel/beam-shaping state. Movement, dimmer, shutter, and zoom
    /// are overridden per-frame from the tests above.
    pub hydro: HydroSpot,
}

impl Default for State {
    fn default() -> Self {
        Self {
            t: 0.0,
            brightness: 0.3,
            blackout: false,
            outcasts_on: true,
            hydros_on: true,
            colorados_on: true,
            color_test: ColorTest::default(),
            solid: [1.0, 0.0, 0.0, 0.0],
            dim_test: DimTest::default(),
            dim: 1.0,
            dim_pd: 2.0,
            move_test: MoveTest::default(),
            pan: 0.5,
            tilt: 0.5,
            circle_r: 0.1,
            circle_pd: 8.0,
            zoom: 0.5,
            zoom_sweep: false,
            outcast_strobe: OutcastBeamwash::SHUTTER_OPEN,
            hydro_shutter: HydroSpot::SHUTTER_OPEN,
            hydro: HydroSpot::default(),
        }
    }
}

#[derive(Default, PartialEq)]
pub enum ColorTest {
    /// Solid color from the RGBW sliders.
    #[default]
    Solid,
    /// R, G, B, RG, GB, RB, W, RGBW at 1s each with a 0->1->0 dimmer ramp,
    /// then a 360° HSV hue sweep over 3s.
    Cycle,
    /// Continuous HSV hue sweep.
    Hsv,
}

#[derive(Default, PartialEq)]
pub enum DimTest {
    /// Manual dimmer slider.
    #[default]
    Manual,
    /// 0 -> 1 -> 0 triangle, for eyeballing 16-bit smoothness at low levels.
    Ramp,
}

#[derive(Default, PartialEq)]
pub enum MoveTest {
    /// Manual pan/tilt sliders.
    #[default]
    Manual,
    /// Analytic circle around the manual pan/tilt position.
    Circle,
    /// Snap between 4 corners every 2s, for eyeballing slew behavior.
    Corners,
}

///////////////////////// TICK /////////////////////////

pub fn tick(mut s: ResMut<State>, t: Res<Time>) {
    s.t = t.elapsed_secs();
}

///////////////////////// LIGHTS /////////////////////////

/// (color, dimmer) for the color cycle test at time `t`.
fn color_cycle(t: f32) -> (Rgbw, f32) {
    const STEPS: [Rgbw; 8] = [
        Rgbw(1.0, 0.0, 0.0, 0.0),
        Rgbw(0.0, 1.0, 0.0, 0.0),
        Rgbw(0.0, 0.0, 1.0, 0.0),
        Rgbw(1.0, 1.0, 0.0, 0.0),
        Rgbw(0.0, 1.0, 1.0, 0.0),
        Rgbw(1.0, 0.0, 1.0, 0.0),
        Rgbw(0.0, 0.0, 0.0, 1.0),
        Rgbw(1.0, 1.0, 1.0, 1.0),
    ];
    const STEP_TIME: f32 = 1.0;
    const HSV_TIME: f32 = 3.0;

    let t = t % (STEPS.len() as f32 * STEP_TIME + HSV_TIME);
    let steps_end = STEPS.len() as f32 * STEP_TIME;
    if t < steps_end {
        let color = STEPS[(t / STEP_TIME) as usize];
        let fr = (t % STEP_TIME) / STEP_TIME;
        (color, fr.tri(1.0))
    } else {
        (Rgb::hsv((t - steps_end) / HSV_TIME, 1.0, 1.0).into(), 1.0)
    }
}

pub fn render(s: Res<State>, mut enttec: ResMut<Enttec>) {
    let (color, cycle_dim) = match s.color_test {
        ColorTest::Solid => (Rgbw(s.solid[0], s.solid[1], s.solid[2], s.solid[3]), 1.0),
        ColorTest::Cycle => color_cycle(s.t),
        ColorTest::Hsv => (Rgb::hsv((s.t / 3.0).fract(), 1.0, 1.0).into(), 1.0),
    };

    let dim = match s.dim_test {
        DimTest::Manual => s.dim,
        DimTest::Ramp => s.t.tri(s.dim_pd),
    };
    let dim = dim * cycle_dim * s.brightness * if s.blackout { 0.0 } else { 1.0 };

    let (pan, tilt) = match s.move_test {
        MoveTest::Manual => (s.pan, s.tilt),
        MoveTest::Circle => {
            let phi = s.t * std::f32::consts::TAU / s.circle_pd;
            (s.pan + s.circle_r * phi.cos(), s.tilt + s.circle_r * phi.sin())
        }
        MoveTest::Corners => {
            let r = 0.2;
            match (s.t / 2.0) as usize % 4 {
                0 => (s.pan - r, s.tilt - r),
                1 => (s.pan + r, s.tilt - r),
                2 => (s.pan + r, s.tilt + r),
                _ => (s.pan - r, s.tilt + r),
            }
        }
    };
    let (pan, tilt) = (pan.clamp(0.0, 1.0), tilt.clamp(0.0, 1.0));

    let zoom = if s.zoom_sweep { s.t.tri(3.0) } else { s.zoom };

    let mut dmx = vec![0u8; LAST_CH];

    if s.outcasts_on {
        for addr in OUTCAST_ADDRS {
            OutcastBeamwash {
                pitch: tilt,
                yaw: pan,
                ring: color,
                ring_alpha: dim,
                ring_strobe: s.outcast_strobe,
                center: color,
                center_alpha: dim,
                center_strobe: s.outcast_strobe,
                zoom,
                ..default()
            }
            .encode(&mut dmx[addr - 1..]);
        }
    }

    if s.hydros_on {
        // A color wheel fixture can't render the RGBW tests: fold the test
        // color's intensity into the dimmer so the Hydros track the show.
        let Rgbw(r, g, b, w) = color;
        let alpha = r.max(g).max(b).max(w) * dim;
        for addr in HYDRO_ADDRS {
            HydroSpot { pitch: tilt, yaw: pan, shutter: s.hydro_shutter, alpha, zoom, ..s.hydro }
                .encode(&mut dmx[addr - 1..]);
        }
    }

    if s.colorados_on {
        for addr in COLORADO_ADDRS {
            ColoradoSolo { color, alpha: dim, zoom }.encode(&mut dmx[addr - 1..]);
        }
    }

    enttec.send(&dmx);
}

///////////////////////// UI /////////////////////////

pub fn draw_ui(mut ctxs: EguiContexts, mut s: ResMut<State>) -> Result {
    let ctx = ctxs.ctx_mut()?;

    egui::CentralPanel::default().show(ctx, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.heading("Master");
            ui.checkbox(&mut s.blackout, "blackout");
            ui.add(egui::Slider::new(&mut s.brightness, 0.0..=1.0).text("brightness"));
            ui.horizontal(|ui| {
                ui.checkbox(&mut s.outcasts_on, "outcasts");
                ui.checkbox(&mut s.hydros_on, "hydros");
                ui.checkbox(&mut s.colorados_on, "colorados");
            });

            ui.separator();
            ui.heading("Color");
            ui.horizontal(|ui| {
                ui.radio_value(&mut s.color_test, ColorTest::Solid, "solid");
                ui.radio_value(&mut s.color_test, ColorTest::Cycle, "cycle");
                ui.radio_value(&mut s.color_test, ColorTest::Hsv, "hsv sweep");
            });
            if s.color_test == ColorTest::Solid {
                for (i, name) in ["r", "g", "b", "w"].into_iter().enumerate() {
                    ui.add(egui::Slider::new(&mut s.solid[i], 0.0..=1.0).text(name));
                }
            }

            ui.separator();
            ui.heading("Dimmer");
            ui.horizontal(|ui| {
                ui.radio_value(&mut s.dim_test, DimTest::Manual, "manual");
                ui.radio_value(&mut s.dim_test, DimTest::Ramp, "ramp");
            });
            match s.dim_test {
                DimTest::Manual => ui.add(egui::Slider::new(&mut s.dim, 0.0..=1.0).text("level")),
                DimTest::Ramp => ui.add(egui::Slider::new(&mut s.dim_pd, 0.25..=30.0).text("period (s)")),
            };

            ui.separator();
            ui.heading("Movement");
            ui.horizontal(|ui| {
                ui.radio_value(&mut s.move_test, MoveTest::Manual, "manual");
                ui.radio_value(&mut s.move_test, MoveTest::Circle, "circle");
                ui.radio_value(&mut s.move_test, MoveTest::Corners, "corners");
            });
            ui.add(egui::Slider::new(&mut s.pan, 0.0..=1.0).text("pan"));
            ui.add(egui::Slider::new(&mut s.tilt, 0.0..=1.0).text("tilt"));
            if s.move_test == MoveTest::Circle {
                ui.add(egui::Slider::new(&mut s.circle_r, 0.0..=0.5).text("radius"));
                ui.add(egui::Slider::new(&mut s.circle_pd, 1.0..=60.0).text("period (s)"));
            }

            ui.separator();
            ui.heading("Zoom");
            ui.checkbox(&mut s.zoom_sweep, "sweep");
            ui.add(egui::Slider::new(&mut s.zoom, 0.0..=1.0).text("zoom"));

            ui.separator();
            ui.heading("Strobe (hardware)");
            ui.add(egui::Slider::new(&mut s.outcast_strobe, 0..=255).text("outcast strobe byte"));
            ui.add(egui::Slider::new(&mut s.hydro_shutter, 0..=255).text("hydro shutter byte"));
            ui.horizontal(|ui| {
                if ui.button("open").clicked() {
                    s.outcast_strobe = OutcastBeamwash::SHUTTER_OPEN;
                    s.hydro_shutter = HydroSpot::SHUTTER_OPEN;
                }
                if ui.button("hydro strobe").clicked() {
                    s.hydro_shutter = HydroSpot::strobe(0.5);
                }
                if ui.button("hydro pulse").clicked() {
                    s.hydro_shutter = HydroSpot::pulse(0.5);
                }
                if ui.button("hydro random").clicked() {
                    s.hydro_shutter = HydroSpot::random_strobe(0.5);
                }
            });

            ui.separator();
            ui.heading("Hydro wheels");
            ui.add(egui::Slider::new(&mut s.hydro.color, 0..=255).text("color wheel 1"));
            ui.add(egui::Slider::new(&mut s.hydro.color2, 0..=255).text("color wheel 2"));
            ui.add(egui::Slider::new(&mut s.hydro.gobo, 0..=255).text("gobo wheel"));
            ui.add(egui::Slider::new(&mut s.hydro.gobo_rot, 0..=255).text("gobo rotation"));
            ui.add(egui::Slider::new(&mut s.hydro.prism, 0..=255).text("prism"));
            ui.add(egui::Slider::new(&mut s.hydro.prism_rot, 0..=255).text("prism rotation"));
            ui.add(egui::Slider::new(&mut s.hydro.focus, 0.0..=1.0).text("focus"));
            ui.add(egui::Slider::new(&mut s.hydro.frost, 0.0..=1.0).text("heavy frost"));
            ui.add(egui::Slider::new(&mut s.hydro.frost2, 0.0..=1.0).text("medium frost"));
        });
    });

    Ok(())
}
