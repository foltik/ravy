//! Chrome shared between the panels.

use std::ops::RangeInclusive;

use lib::prelude::*;

/// Fixed widths for the label and the readout, so stacked rows line up and
/// every slider starts and ends in the same place.
const LABEL: f32 = 62.0;
const VALUE: f32 = 52.0;

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
    logarithmic: bool,
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
        let slider = egui::Slider::new(value, range)
            .logarithmic(logarithmic)
            .smallest_positive(1e-5)
            .show_value(false);
        changed |= ui.add(slider).changed();
    });
    changed
}
