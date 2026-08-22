//! On-screen mirrors of the MIDI control surfaces: each renders the device
//! state, injects clicks as inputs, and works with no hardware connected.
//! Apps place the panes and supply the labels; the widgets live here.

use crate::midi::device::launch_control_xl::{self as ctl, LaunchControlXL};
use crate::midi::device::launchpad_x::{self as lpx, LaunchpadX};
use crate::prelude::*;

///////////////////////// PAD /////////////////////////

/// Launchpad X plus an on-screen mirror.
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

    /// The 9x9 grid; `binding` names the pad at a coord for the tooltips.
    pub fn draw(&mut self, ui: &mut egui::Ui, binding: impl Fn(i8, i8) -> Option<String>) {
        use lpx::types::*;

        ui.spacing_mut().item_spacing = egui::vec2(3.0, 3.0);
        for y in (0..9usize).rev() {
            ui.horizontal(|ui| {
                for x in 0..9usize {
                    let Rgb(r, g, b) = self.leds[x][y];
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

                    if let Some(name) = binding(x as i8, y as i8) {
                        resp.clone().on_hover_text(name);
                    }

                    let down = resp.is_pointer_button_down_on();
                    if down != self.held[x][y] {
                        self.held[x][y] = down;
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
                        self.sim.push(input);
                    }
                }
            });
        }
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
    /// Knob rows top to bottom: send A, send B, pan. -1..1 around a centre.
    pub knobs: [[f32; 8]; 3],
    pub sliders: [f32; 8],
    /// LED under each knob, mirrored from the outgoing LED batches.
    knob_leds: [[Rgb; 8]; 3],
    /// Focus row then control row, likewise.
    leds: [[Rgb; 8]; 2],
    /// Send select up/down, then track select left/right.
    select_leds: [[Rgb; 2]; 2],
    /// Device, mute, solo, record.
    side_leds: [Rgb; 4],
    held: [[bool; 8]; 2],
    select_held: [[bool; 2]; 2],
    side_held: [bool; 4],
    sim: Vec<ctl::Input>,
}

impl Ctrl {
    pub fn new(midi: Midi<LaunchControlXL>) -> Self {
        Self {
            midi,
            knobs: [[0.0; 8]; 3],
            sliders: [0.0; 8],
            knob_leds: [[Rgb::BLACK; 8]; 3],
            leds: [[Rgb::BLACK; 8]; 2],
            select_leds: [[Rgb::BLACK; 2]; 2],
            side_leds: [Rgb::BLACK; 4],
            held: [[false; 8]; 2],
            select_held: [[false; 2]; 2],
            side_held: [false; 4],
            sim: vec![],
        }
    }

    pub fn connected(&self) -> bool {
        self.midi.connected()
    }

    pub fn name(&self) -> &str {
        self.midi.name()
    }

    pub fn recv(&mut self) -> Vec<ctl::Input> {
        let mut inputs = self.midi.recv();
        // Keep the mirror in sync with hardware moves
        for input in &inputs {
            match *input {
                ctl::Input::SendA(i, fr) => self.knobs[0][i as usize] = fr,
                ctl::Input::SendB(i, fr) => self.knobs[1][i as usize] = fr,
                ctl::Input::Pan(i, fr) => self.knobs[2][i as usize] = fr,
                ctl::Input::Slider(i, fr) => self.sliders[i as usize] = fr,
                _ => {}
            }
        }
        inputs.append(&mut self.sim);
        inputs
    }

    pub fn send(&mut self, output: ctl::Output) {
        match &output {
            ctl::Output::Batch(batch) => {
                for &(led, color, brightness) in batch {
                    self.led(led, color, brightness);
                }
            }
            single => {
                let (led, color, brightness) = single.single();
                self.led(led, color, brightness);
            }
        }
        self.midi.send(output);
    }

    fn led(&mut self, led: ctl::Led, color: ctl::types::Color, brightness: ctl::types::Brightness) {
        use ctl::types::Color;

        // These four only light amber, whatever is sent; see process_output.
        let color = match led {
            ctl::Led::Device | ctl::Led::Mute | ctl::Led::Solo | ctl::Led::Record => Color::Amber,
            _ => color,
        };
        let slot = match led {
            ctl::Led::SendA(i) => &mut self.knob_leds[0][i as usize],
            ctl::Led::SendB(i) => &mut self.knob_leds[1][i as usize],
            ctl::Led::Pan(i) => &mut self.knob_leds[2][i as usize],
            ctl::Led::Focus(i) => &mut self.leds[0][i as usize],
            ctl::Led::Control(i) => &mut self.leds[1][i as usize],
            ctl::Led::SendSelect(up) => &mut self.select_leds[0][if up { 0 } else { 1 }],
            ctl::Led::TrackSelect(left) => &mut self.select_leds[1][if left { 0 } else { 1 }],
            ctl::Led::Device => &mut self.side_leds[0],
            ctl::Led::Mute => &mut self.side_leds[1],
            ctl::Led::Solo => &mut self.side_leds[2],
            ctl::Led::Record => &mut self.side_leds[3],
        };
        let level = brightness.byte() as f32 / 3.0;
        *slot = match color {
            Color::Red => Rgb(level, 0.0, 0.0),
            Color::Amber => Rgb(level, 0.75 * level, 0.0),
            Color::Green => Rgb(0.0, level, 0.0),
        };
    }

    /// The whole surface, laid out like the hardware: the knob/slider/button
    /// grid with the template, select, and track buttons down the right.
    pub fn draw(&mut self, ui: &mut egui::Ui, labels: &CtrlLabels) {
        // Column pitch follows the sliders, whose thickness egui derives from
        // exactly this expression.
        let w = ui.text_style_height(&egui::TextStyle::Body).max(ui.spacing().interact_size.y);

        ui.horizontal_top(|ui| {
            ui.vertical(|ui| self.grid(ui, w, labels));
            ui.vertical(|ui| self.side(ui, w, labels));
        });
    }

    /// Three knob rows over the sliders over the two button rows.
    fn grid(&mut self, ui: &mut egui::Ui, w: f32, labels: &CtrlLabels) {
        for row in 0..3 {
            ui.horizontal(|ui| {
                for i in 0..8 {
                    let mut fr = self.knobs[row][i];
                    tooltip(knob(ui, w, &mut fr, self.knob_leds[row][i]), labels.knobs[row][i]);
                    if fr != self.knobs[row][i] {
                        self.knobs[row][i] = fr;
                        self.sim.push(match row {
                            0 => ctl::Input::SendA(i as u8, fr),
                            1 => ctl::Input::SendB(i as u8, fr),
                            _ => ctl::Input::Pan(i as u8, fr),
                        });
                    }
                }
            });
        }

        ui.horizontal(|ui| {
            for i in 0..8 {
                let mut fr = self.sliders[i];
                let slider = egui::Slider::new(&mut fr, 0.0..=1.0).vertical().show_value(false);
                let resp = tooltip(ui.add(slider), labels.sliders[i]);
                if resp.changed() {
                    self.sliders[i] = fr;
                    self.sim.push(ctl::Input::Slider(i as u8, fr));
                }
            }
        });

        for row in 0..2 {
            ui.horizontal(|ui| {
                for i in 0..8 {
                    let (rect, resp) =
                        ui.allocate_exact_size(egui::vec2(w, w * 0.6), egui::Sense::click_and_drag());
                    cell(ui, rect, self.leds[row][i]);
                    tooltip(resp.clone(), [labels.focus, labels.control][row][i]);

                    let down = resp.is_pointer_button_down_on();
                    if down != self.held[row][i] {
                        self.held[row][i] = down;
                        self.sim.push(match row {
                            0 => ctl::Input::Focus(i as u8, down),
                            _ => ctl::Input::Control(i as u8, down),
                        });
                    }
                }
            });
        }
    }

    /// The right-hand column, following the hardware: a button pair level with
    /// each knob row, then device, mute, solo, record squared up beside the
    /// sliders and evenly spread over their travel.
    fn side(&mut self, ui: &mut egui::Ui, w: f32, labels: &CtrlLabels) {
        ui.spacing_mut().item_spacing = egui::vec2(3.0, 3.0);
        let size = egui::vec2(w, w);
        // A pair row is exactly as tall as a knob cell, so the rows stay level.
        let row = egui::vec2(2.0 * w + 3.0, w + 6.0);
        let centered = egui::Layout::left_to_right(egui::Align::Center);

        // The driver keeps the device on the user template; shown, not clickable.
        ui.allocate_ui_with_layout(row, centered, |ui| {
            for (name, lit) in [("user template", true), ("factory template", false)] {
                let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::hover());
                cell(ui, rect, if lit { Rgb::RED } else { Rgb::BLACK });
                tooltip(resp, name);
            }
        });

        let selects = [
            ([egui::vec2(0.0, -1.0), egui::vec2(0.0, 1.0)], "send select"),
            ([egui::vec2(-1.0, 0.0), egui::vec2(1.0, 0.0)], "track select"),
        ];
        for (pair, (dirs, hw)) in selects.into_iter().enumerate() {
            ui.allocate_ui_with_layout(row, centered, |ui| {
                for i in 0..2 {
                    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
                    cell(ui, rect, self.select_leds[pair][i]);
                    arrow(ui, rect, dirs[i]);
                    tooltip(resp.clone(), or(labels.selects[pair][i], hw));

                    let down = resp.is_pointer_button_down_on();
                    if down != self.select_held[pair][i] {
                        self.select_held[pair][i] = down;
                        self.sim.push(match pair {
                            0 => ctl::Input::SendSelect(i == 0, down),
                            _ => ctl::Input::TrackSelect(i == 0, down),
                        });
                    }
                }
            });
        }

        let len = ui.spacing().slider_width;
        let (span, _) = ui.allocate_exact_size(egui::vec2(row.x, len), egui::Sense::hover());
        let hw = ["device", "mute", "solo", "record arm"];
        for i in 0..4 {
            let top = span.top() + (len - w) * i as f32 / 3.0;
            let rect = egui::Rect::from_min_size(
                egui::pos2(span.center().x - w / 2.0, top),
                egui::vec2(w, w),
            );
            let resp = ui.interact(rect, ui.id().with(("side", i)), egui::Sense::click_and_drag());
            cell(ui, rect, self.side_leds[i]);
            tooltip(resp.clone(), or(labels.side[i], hw[i]));

            let down = resp.is_pointer_button_down_on();
            if down != self.side_held[i] {
                self.side_held[i] = down;
                self.sim.push(match i {
                    0 => ctl::Input::Device(down),
                    1 => ctl::Input::Mute(down),
                    2 => ctl::Input::Solo(down),
                    _ => ctl::Input::Record(down),
                });
            }
        }
    }
}

/// What the controls drive, for the tooltips; empty means none. The side
/// buttons fall back to their hardware names.
pub struct CtrlLabels {
    /// Rows top to bottom: send A, send B, pan.
    pub knobs: [[&'static str; 8]; 3],
    pub sliders: [&'static str; 8],
    pub focus: [&'static str; 8],
    pub control: [&'static str; 8],
    /// Send select up/down, then track select left/right.
    pub selects: [[&'static str; 2]; 2],
    /// Device, mute, solo, record.
    pub side: [&'static str; 4],
}

impl Default for CtrlLabels {
    fn default() -> Self {
        Self {
            knobs: [[""; 8]; 3],
            sliders: [""; 8],
            focus: [""; 8],
            control: [""; 8],
            selects: [[""; 2]; 2],
            side: [""; 4],
        }
    }
}

fn or(label: &'static str, fallback: &'static str) -> &'static str {
    match label {
        "" => fallback,
        label => label,
    }
}

/// A button painted in its LED colour.
fn cell(ui: &mut egui::Ui, rect: egui::Rect, led: Rgb) {
    ui.painter().rect_filled(rect, 3.0, color32(led));
    ui.painter().rect_stroke(
        rect,
        3.0,
        egui::Stroke::new(1.0_f32, egui::Color32::from_gray(60)),
        egui::StrokeKind::Inside,
    );
}

fn arrow(ui: &mut egui::Ui, rect: egui::Rect, dir: egui::Vec2) {
    let center = rect.center();
    let perp = egui::vec2(-dir.y, dir.x);
    let points = vec![
        center + dir * 3.0,
        center - dir * 3.0 + perp * 3.0,
        center - dir * 3.0 - perp * 3.0,
    ];
    ui.painter().add(egui::Shape::convex_polygon(
        points,
        egui::Color32::from_gray(160),
        egui::Stroke::NONE,
    ));
}

fn tooltip(resp: egui::Response, label: &str) -> egui::Response {
    match label {
        "" => resp,
        label => resp.on_hover_text(label),
    }
}

fn color32(Rgb(r, g, b): Rgb) -> egui::Color32 {
    egui::Color32::from_rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
}

/// Pointer travel either side of straight up.
const SWEEP: f32 = 3.0 * std::f32::consts::PI / 4.0;

/// A rotary knob for a -1..1 value with its LED underneath: drag vertically to
/// turn, double-click to centre.
fn knob(ui: &mut egui::Ui, size: f32, value: &mut f32, led: Rgb) -> egui::Response {
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(size, size + 6.0), egui::Sense::click_and_drag());
    if resp.dragged() {
        *value = (*value - resp.drag_delta().y / 60.0).clamp(-1.0, 1.0);
    }
    if resp.double_clicked() {
        *value = 0.0;
    }

    let center = rect.center_top() + egui::vec2(0.0, size / 2.0);
    let radius = size / 2.0 - 1.0;
    let outline = egui::Stroke::new(1.0_f32, egui::Color32::from_gray(60));
    let visuals = ui.style().interact(&resp);
    ui.painter().circle(center, radius, visuals.bg_fill, outline);
    let angle = *value * SWEEP;
    let dir = egui::vec2(angle.sin(), -angle.cos());
    ui.painter().line_segment(
        [center + dir * (radius * 0.3), center + dir * (radius * 0.9)],
        egui::Stroke::new(1.5_f32, visuals.fg_stroke.color),
    );

    let below = egui::pos2(rect.center().x, rect.bottom() - 2.5);
    ui.painter().circle(below, 2.5, color32(led), outline);
    resp
}
