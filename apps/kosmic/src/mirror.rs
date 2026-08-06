use lib::midi::device::launch_control_xl::{self as ctl, LaunchControlXL};
use lib::midi::device::launchpad_x::{self as lpx, LaunchpadX};
use lib::prelude::*;

use crate::ui::device_header;

///////////////////////// PAD /////////////////////////

/// Launchpad X plus an on-screen mirror: renders the last frame, injects
/// clicks as inputs, and works with no hardware connected.
#[derive(Resource)]
pub struct Pad {
    midi: Midi<LaunchpadX>,
    was_connected: bool,
    leds: [[Rgb; 9]; 9],
    held: [[bool; 9]; 9],
    sim: Vec<lpx::Input>,
}

impl Pad {
    pub fn new(midi: Midi<LaunchpadX>) -> Self {
        Self {
            midi,
            was_connected: false,
            leds: [[Rgb::BLACK; 9]; 9],
            held: [[false; 9]; 9],
            sim: vec![],
        }
    }

    pub fn connected(&self) -> bool {
        self.midi.connected()
    }

    pub fn name(&self) -> &str {
        self.midi.name()
    }

    pub fn recv(&mut self) -> Vec<lpx::Input> {
        use lpx::types::*;

        // App config on top of the device init, resent on every (re)connect
        let connected = self.midi.connected();
        if connected && !self.was_connected {
            self.midi.send(lpx::Output::Pressure(Pressure::Off, PressureCurve::Medium));
            self.midi.send(lpx::Output::Brightness(0.0));
        }
        self.was_connected = connected;

        let mut inputs = self.midi.recv();
        inputs.append(&mut self.sim);
        inputs
    }

    pub fn send(&mut self, output: lpx::Output) {
        use lpx::types::*;

        match &output {
            lpx::Output::Batch(batch) => {
                for (pos, color) in batch {
                    let Coord(x, y) = Coord::from_byte(pos.byte());
                    self.leds[x as usize][y as usize] = match *color {
                        Color::Rgb(r, g, b) => Rgb(r, g, b),
                        Color::Palette(p) => p.into(),
                    };
                }
            }
            lpx::Output::Clear => self.leds = [[Rgb::BLACK; 9]; 9],
            _ => {}
        }
        self.midi.send(output);
    }
}

/// Edge buttons come in as named inputs, not coords.
#[rustfmt::skip]
fn edge(x: usize, y: usize, b: bool) -> Option<lpx::Input> {
    Some(match (x, y) {
        (0, 8) => lpx::Input::Up(b),
        (1, 8) => lpx::Input::Down(b),
        (2, 8) => lpx::Input::Left(b),
        (3, 8) => lpx::Input::Right(b),
        (4, 8) => lpx::Input::Session(b),
        (5, 8) => lpx::Input::Note(b),
        (6, 8) => lpx::Input::Custom(b),
        (7, 8) => lpx::Input::Capture(b),
        (8, 7) => lpx::Input::Volume(b),
        (8, 6) => lpx::Input::Pan(b),
        (8, 5) => lpx::Input::A(b),
        (8, 4) => lpx::Input::B(b),
        (8, 3) => lpx::Input::Stop(b),
        (8, 2) => lpx::Input::Mute(b),
        (8, 1) => lpx::Input::Solo(b),
        (8, 0) => lpx::Input::Record(b),
        _ => return None,
    })
}

///////////////////////// CTRL /////////////////////////

/// LaunchControl XL plus an on-screen mirror.
#[derive(Resource)]
pub struct Ctrl {
    midi: Midi<LaunchControlXL>,
    pub sliders: [f32; 8],
    sim: Vec<ctl::Input>,
}

impl Ctrl {
    pub fn new(midi: Midi<LaunchControlXL>) -> Self {
        Self { midi, sliders: [0.0; 8], sim: vec![] }
    }

    pub fn connected(&self) -> bool {
        self.midi.connected()
    }

    pub fn recv(&mut self) -> Vec<ctl::Input> {
        let mut inputs = self.midi.recv();
        // Keep the mirror in sync with hardware moves
        for input in &inputs {
            if let ctl::Input::Slider(i, fr) = input {
                self.sliders[*i as usize] = *fr;
            }
        }
        inputs.append(&mut self.sim);
        inputs
    }

    pub fn name(&self) -> &str {
        self.midi.name()
    }

    #[allow(unused)]
    pub fn send(&mut self, output: ctl::Output) {
        self.midi.send(output);
    }
}

///////////////////////// DRAW /////////////////////////

/// What each Launch Control XL slider drives, for the tooltips.
const SLIDERS: [&str; 8] = ["", "", "", "", "", "", "", "brightness"];

pub fn draw(
    mut ctxs: EguiContexts,
    mut pad: ResMut<Pad>,
    mut ctrl: ResMut<Ctrl>,
    state: Res<crate::logic::State>,
    mut styled: Local<bool>,
) -> Result {
    use lpx::types::*;

    let ctx = ctxs.ctx_mut()?;
    if !*styled {
        *styled = true;
        // Tooltips are the labels on an unlabelled grid, so they want to be
        // near-instant rather than egui's read-the-docs pause.
        ctx.global_style_mut(|s| s.interaction.tooltip_delay = 0.05);
    }

    egui::Window::new("Launchpad X").id(egui::Id::new("pad")).show(ctx, |ui| {
        device_header(ui, pad.name(), pad.connected());
        ui.spacing_mut().item_spacing = egui::vec2(3.0, 3.0);
        for y in (0..9usize).rev() {
            ui.horizontal(|ui| {
                for x in 0..9usize {
                    let Rgb(r, g, b) = pad.leds[x][y];
                    let fill =
                        egui::Color32::from_rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8);
                    let (rect, resp) =
                        ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::click_and_drag());
                    let rounding = if x < 8 && y < 8 { 3.0 } else { 12.0 };
                    ui.painter().rect_filled(rect, rounding, fill);
                    ui.painter().rect_stroke(
                        rect,
                        rounding,
                        egui::Stroke::new(1.0_f32, egui::Color32::from_gray(60)),
                        egui::StrokeKind::Inside,
                    );

                    let bound = state.perform.iter().find(|b| b.xy == (x as i8, y as i8));
                    if let Some(bound) = bound {
                        resp.clone().on_hover_text(bound.op.name());
                    }

                    let down = resp.is_pointer_button_down_on();
                    if down != pad.held[x][y] {
                        pad.held[x][y] = down;
                        let input = if x < 8 && y < 8 {
                            let i = Index::from(Pos::from(Coord(x as i8, y as i8)));
                            match down {
                                true => lpx::Input::Press(i, 1.0),
                                false => lpx::Input::Release(i),
                            }
                        } else {
                            match edge(x, y, down) {
                                Some(input) => input,
                                None => continue,
                            }
                        };
                        pad.sim.push(input);
                    }
                }
            });
        }
    });

    egui::Window::new("Launch Control XL").id(egui::Id::new("ctrl")).show(ctx, |ui| {
        device_header(ui, ctrl.name(), ctrl.connected());
        ui.horizontal(|ui| {
            for i in 0..8 {
                let mut fr = ctrl.sliders[i];
                let slider = egui::Slider::new(&mut fr, 0.0..=1.0).vertical().show_value(false);
                let resp = ui.add(slider).on_hover_text(match SLIDERS[i] {
                    "" => "unbound",
                    name => name,
                });
                if resp.changed() {
                    ctrl.sliders[i] = fr;
                    ctrl.sim.push(ctl::Input::Slider(i as u8, fr));
                }
            }
        });
    });

    Ok(())
}
