use bevy_egui::EguiContexts;
use bevy_egui::egui::{self, Stroke, StrokeKind};

use crate::prelude::*;

const PEAK_HOLD_SEC: f32 = 0.5;
const PEAK_DECAY_HZ: f32 = 0.6;

#[derive(Resource, Default)]
pub struct AudioMeter {
    peak: f32,
    peak_age: f32,
}

pub fn audio_ui(
    mut ctxs: EguiContexts,
    mut audio: ResMut<Audio>,
    mut meter: ResMut<AudioMeter>,
    time: Res<Time>,
) -> Result {
    let ctx = ctxs.ctx_mut()?;

    let dt = time.delta_secs();
    let rms = audio.rms();
    let peak = audio.peak();

    if peak > meter.peak {
        meter.peak = peak;
        meter.peak_age = 0.0;
    } else {
        meter.peak_age += dt;
        if meter.peak_age > PEAK_HOLD_SEC {
            meter.peak = (meter.peak - PEAK_DECAY_HZ * dt).max(meter.peak);
        }
    }

    egui::Window::new("Audio")
        .resizable(false)
        .default_width(320.0)
        .show(ctx, |ui| {
            ui.scope(|ui| {
                egui::Grid::new("audio")
                    .num_columns(2)
                    .min_col_width(0.0)
                    .spacing(egui::vec2(12.0, 6.0))
                    .show(ui, |ui| {
                        // Left: dBFS meter
                        ui.vertical(|ui| {
                            draw_meter(ui, rms, peak, meter.peak);
                        });

                        // Right: device device selection + readouts
                        ui.vertical(|ui| {
                            // Input
                            ui.horizontal(|ui| {
                                let mut input = audio.input.clone().unwrap_or_else(|| "None".to_string());

                                ui.label("Input");
                                egui::ComboBox::from_id_salt("input_device_combo")
                                    .width(220.0)
                                    .selected_text(input.clone())
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(&mut input, "None".to_string(), "None");
                                        for name in audio.available_inputs() {
                                            ui.selectable_value(&mut input, name.clone(), name.clone());
                                        }
                                    });

                                audio.set_input(if input == "None" { None } else { Some(input) });
                            });

                            // Output
                            ui.horizontal(|ui| {
                                let mut output = audio.output.clone().unwrap_or_else(|| "None".to_string());

                                ui.label("Output");
                                egui::ComboBox::from_id_salt("output_device_combo")
                                    .width(220.0)
                                    .selected_text(output.clone())
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(&mut output, "None".to_string(), "None");
                                        for name in audio.available_outputs() {
                                            ui.selectable_value(&mut output, name.clone(), name.clone());
                                        }
                                    });

                                audio.set_output(if output == "None" { None } else { Some(output) });
                            });

                            // Readouts
                            ui.add_space(4.0);
                            ui.monospace(format!("Peak: {:>6.1} dBFS", linear_to_dbfs(meter.peak)));
                            ui.monospace(format!("RMS : {:>6.1} dBFS", linear_to_dbfs(rms)));
                        });

                        ui.end_row();
                    });
            });
        });

    Ok(())
}

fn draw_meter(ui: &mut egui::Ui, rms: f32, peak: f32, peak_hold: f32) {
    let (w, h) = (18.0, 140.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
    let painter = ui.painter_at(rect);

    let bg = ui.visuals().extreme_bg_color;
    let fg_stroke = ui.visuals().widgets.noninteractive.fg_stroke.color;
    let bg_stroke = ui.visuals().widgets.inactive.bg_stroke.color;

    // Border
    painter.rect_filled(rect, 5.0, bg);
    painter.rect_stroke(rect, 5.0, Stroke::new(1.0, fg_stroke), StrokeKind::Inside);

    let pad = 3.0;
    let inner = egui::Rect::from_min_max(
        egui::pos2(rect.left() + pad, rect.top() + pad),
        egui::pos2(rect.right() - pad, rect.bottom() - pad),
    );

    // dBFS ticks
    let ticks = [0.0, -6.0, -12.0, -24.0];
    for (i, db) in ticks.iter().enumerate() {
        let y = inner.bottom() - dbfs_to_linear(*db).clamp(0.0, 1.0) * inner.height();
        painter.line_segment(
            [egui::pos2(inner.left(), y), egui::pos2(inner.right(), y)],
            Stroke::new(
                if *db == 0.0 { 1.2 } else { 0.8 },
                bg_stroke.linear_multiply(if i == 0 { 1.0 } else { 0.7 }),
            ),
        );
    }

    // Peak meter from green to red
    let green = 0.60;
    let yellow = 0.85;
    if peak > 0.0 {
        let h = peak.min(green);
        if h > 0.0 {
            painter.rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(inner.left(), inner.bottom() - h * inner.height()),
                    egui::pos2(inner.right(), inner.bottom()),
                ),
                0.0,
                egui::Color32::from_rgb(88, 230, 144),
            );
        }
    }
    if peak >= green {
        let h = peak.min(yellow);
        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(inner.left(), inner.bottom() - h * inner.height()),
                egui::pos2(inner.right(), inner.bottom() - green * inner.height()),
            ),
            0.0,
            egui::Color32::from_rgb(255, 210, 60),
        );
    }
    if peak >= yellow {
        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(inner.left(), inner.bottom() - peak * inner.height()),
                egui::pos2(inner.right(), inner.bottom() - yellow * inner.height()),
            ),
            0.0,
            egui::Color32::from_rgb(255, 80, 80),
        );
    }

    // Overlay transparent RMS meter
    if rms > 0.0 {
        let inset = (inner.width() * 0.40).max(1.0);
        let inner = egui::Rect::from_min_max(
            egui::pos2(inner.left() + inset, inner.top()),
            egui::pos2(inner.right() - inset, inner.bottom()),
        );
        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(inner.left(), inner.bottom() - rms * inner.height()),
                egui::pos2(inner.right(), inner.bottom()),
            ),
            2.0,
            egui::Color32::WHITE,
        );
    }

    // Peak hold tick
    let hold_y = inner.bottom() - peak_hold.clamp(0.0, 1.0) * inner.height();
    painter.line_segment(
        [
            egui::pos2(inner.left() + 1.0, hold_y),
            egui::pos2(inner.right() - 1.0, hold_y),
        ],
        Stroke::new(2.0, egui::Color32::WHITE),
    );
}

fn linear_to_dbfs(x: f32) -> f32 {
    if x <= 1e-9 { -120.0 } else { 20.0 * x.max(1e-9).log10() }
}
fn dbfs_to_linear(db: f32) -> f32 {
    (10.0_f32).powf(db / 20.0)
}
