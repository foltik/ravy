//! Chrome shared between the panels.

use std::ops::RangeInclusive;

use lib::prelude::*;

/// Fixed widths for the label and the readout, so stacked rows line up and
/// every slider starts and ends in the same place.
const LABEL: f32 = 62.0;
const VALUE: f32 = 52.0;

/// What every panel keeps between itself and the edge of the view, and between
/// itself and the panel beside it.
pub const MARGIN: f32 = 16.0;

/// A panel held in a corner of the view, `off` further in from it, until it is
/// dragged; after that it stays where it was put.
pub fn pane(
    ctx: &egui::Context,
    name: &str,
    corner: egui::Align2,
    off: egui::Vec2,
) -> egui::Window<'static> {
    let id = egui::Id::new(name);
    let held = egui::Id::new((name, "held"));

    // Areas drag under their own id; nothing else here can tell a drag from
    // egui pulling an oversized panel back onto the screen.
    let dragged = ctx.is_being_dragged(id.with("move"));
    let free = dragged || ctx.data(|d| d.get_temp::<bool>(held)).unwrap_or(false);
    ctx.data_mut(|d| d.insert_temp(held, free));

    let window = egui::Window::new(name.to_string()).id(id).pivot(corner);
    if free {
        return window;
    }
    let inward = egui::vec2(inward(corner.x()), inward(corner.y()));
    let at = corner.pos_in_rect(&ctx.content_rect()) + inward * (egui::Vec2::splat(MARGIN) + off);
    window.current_pos(at)
}

/// Which way is into the view from an edge.
fn inward(align: egui::Align) -> f32 {
    match align {
        egui::Align::Max => -1.0,
        _ => 1.0,
    }
}

pub const DARK: egui::Color32 = egui::Color32::from_gray(52);
pub const GREEN: egui::Color32 = egui::Color32::from_rgb(80, 230, 120);
pub const RED: egui::Color32 = egui::Color32::from_rgb(230, 80, 80);
/// Held by hand rather than by the show, so lit solid instead of blinking.
pub const AMBER: egui::Color32 = egui::Color32::from_rgb(240, 180, 60);
pub const BLUE: egui::Color32 = egui::Color32::from_rgb(90, 160, 255);

/// Fixed-width indicator dot.
pub fn lamp(ui: &mut egui::Ui, color: egui::Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 4.0, color);
}

/// `[lamp] name` over a panel that talks to one piece of hardware.
pub fn device_header(ui: &mut egui::Ui, name: &str, connected: bool) {
    ui.horizontal(|ui| {
        lamp(ui, match connected {
            true => GREEN,
            false => RED,
        });
        ui.label(name);
    });
    ui.separator();
}

/// One `[label] [value] [slider]` row, the slider taking the rest of the width.
pub fn knob<T: egui::emath::Numeric>(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut T,
    range: RangeInclusive<T>,
) -> bool {
    row(ui, label, value, range, false)
}

/// How many choices a [`pick`] puts on a row before wrapping onto the next.
const PER_ROW: usize = 3;

/// One `[label] [choice]...` row, of which exactly one choice is set. A long
/// list wraps rather than widening the panel to fit it.
pub fn pick<T: PartialEq + Copy>(ui: &mut egui::Ui, label: &str, value: &mut T, choices: &[(T, &str)]) {
    let height = ui.spacing().interact_size.y;
    for (i, row) in choices.chunks(PER_ROW).enumerate() {
        ui.horizontal(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(LABEL, height),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    if i == 0 {
                        ui.label(label);
                    }
                },
            );
            for (choice, name) in row {
                if ui.selectable_label(*value == *choice, *name).clicked() {
                    *value = *choice;
                }
            }
        });
    }
}

/// A [`knob`] whose slider is logarithmic, for a value spanning decades. The
/// bottom of the travel is still exactly the range's start.
pub fn knob_log<T: egui::emath::Numeric>(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut T,
    range: RangeInclusive<T>,
) -> bool {
    row(ui, label, value, range, true)
}

fn row<T: egui::emath::Numeric>(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut T,
    range: RangeInclusive<T>,
    log: bool,
) -> bool {
    let height = ui.spacing().interact_size.y;
    let span = range.end().to_f64() - range.start().to_f64();
    let mut changed = false;

    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(LABEL, height),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| ui.label(label),
        );

        let drag = egui::DragValue::new(value).speed(span / 200.0).range(range.clone()).max_decimals(5);
        changed |= ui.add_sized([VALUE, height], drag).changed();

        ui.spacing_mut().slider_width = (ui.available_width() - ui.spacing().item_spacing.x).max(40.0);
        let slider =
            egui::Slider::new(value, range).logarithmic(log).smallest_positive(1e-5).show_value(false);
        changed |= ui.add(slider).changed();
    });
    changed
}
