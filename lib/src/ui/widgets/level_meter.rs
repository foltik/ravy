use bevy_egui::egui::{self, Stroke, StrokeKind};

use crate::prelude::*;

pub struct LevelMeter {
    pub min: f32,          // bottom of scale
    pub max: f32,          // top of scale
    pub yellow_start: f32, // start of yellow band
    pub red_start: f32,    // start of red band
    pub size_px: egui::Vec2,
    pub pad_px: f32,
}

impl LevelMeter {
    #[inline]
    fn norm(&self, x: f32) -> f32 {
        let span = (self.max - self.min).max(1e-6);
        ((x - self.min) / span).clamp(0.0, 1.0)
    }

    /// Draws a vertical segmented LevelMeter (dim bands always visible),
    /// lights the portion up to `value`, and optionally draws a hold tick.
    pub fn draw(&self, ui: &mut egui::Ui, value: f32, hold_tick: Option<f32>) {
        let (rect, _) = ui.allocate_exact_size(self.size_px, egui::Sense::hover());
        let painter = ui.painter_at(rect);

        let bg = ui.visuals().extreme_bg_color;
        let fg_stroke = ui.visuals().widgets.noninteractive.fg_stroke.color;
        let grid_col = ui.visuals().widgets.inactive.bg_stroke.color;

        // Frame
        painter.rect_filled(rect, 5.0, bg);
        painter.rect_stroke(rect, 5.0, Stroke::new(1.0, fg_stroke), StrokeKind::Inside);

        // Inner
        let inner = egui::Rect::from_min_max(
            egui::pos2(rect.left() + self.pad_px, rect.top() + self.pad_px),
            egui::pos2(rect.right() - self.pad_px, rect.bottom() - self.pad_px),
        );
        let h = inner.height();
        let y_of = |u: f32| inner.bottom() - self.norm(u) * h;

        // Colors (dim + lit)
        let dim_g = egui::Color32::from_rgb(88, 230, 144).linear_multiply(0.20);
        let dim_y = egui::Color32::from_rgb(255, 210, 60).linear_multiply(0.20);
        let dim_r = egui::Color32::from_rgb(255, 80, 80).linear_multiply(0.20);

        let lit_g = egui::Color32::from_rgb(88, 230, 144);
        let lit_y = egui::Color32::from_rgb(255, 210, 60);
        let lit_r = egui::Color32::from_rgb(255, 80, 80);

        // --- Background bands (always visible, dim) ---
        // Green band
        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(inner.left(), y_of(self.yellow_start)),
                egui::pos2(inner.right(), y_of(self.min)),
            ),
            0.0,
            dim_g,
        );
        // Yellow band
        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(inner.left(), y_of(self.red_start)),
                egui::pos2(inner.right(), y_of(self.yellow_start)),
            ),
            0.0,
            dim_y,
        );
        // Red band
        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(inner.left(), y_of(self.max)),
                egui::pos2(inner.right(), y_of(self.red_start)),
            ),
            0.0,
            dim_r,
        );

        // --- Lit portion up to current value ---
        let v = value.clamp(self.min, self.max);

        // Green lit
        if v > self.min {
            let v_g = v.min(self.yellow_start);
            if v_g > self.min {
                painter.rect_filled(
                    egui::Rect::from_min_max(
                        egui::pos2(inner.left(), y_of(v_g)),
                        egui::pos2(inner.right(), y_of(self.min)),
                    ),
                    0.0,
                    lit_g,
                );
            }
        }
        // Yellow lit
        if v > self.yellow_start {
            let v_y = v.min(self.red_start);
            if v_y > self.yellow_start {
                painter.rect_filled(
                    egui::Rect::from_min_max(
                        egui::pos2(inner.left(), y_of(v_y)),
                        egui::pos2(inner.right(), y_of(self.yellow_start)),
                    ),
                    0.0,
                    lit_y,
                );
            }
        }
        // Red lit
        if v > self.red_start {
            painter.rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(inner.left(), y_of(v)),
                    egui::pos2(inner.right(), y_of(self.red_start)),
                ),
                0.0,
                lit_r,
            );
        }

        // --- Optional hold tick
        if let Some(hv) = hold_tick {
            let hy = y_of(hv.clamp(self.min, self.max));
            painter.line_segment(
                [
                    egui::pos2(inner.left() + 1.0, hy),
                    egui::pos2(inner.right() - 1.0, hy),
                ],
                Stroke::new(2.0, egui::Color32::WHITE),
            );
        }

        // --- Legend from thresholds
        for (i, t) in [self.min, self.yellow_start, self.red_start, self.max].into_iter().enumerate() {
            let y = y_of(t);
            painter.line_segment(
                [egui::pos2(inner.left(), y), egui::pos2(inner.right(), y)],
                Stroke::new(
                    if i == 3 { 1.2 } else { 0.9 },
                    grid_col.linear_multiply(if i == 3 { 1.0 } else { 0.7 }),
                ),
            );
            painter.text(
                egui::pos2(rect.right() + 6.0, y - 6.0),
                egui::Align2::LEFT_TOP,
                format!("{:+.0}", t),
                egui::TextStyle::Small.resolve(ui.style()),
                grid_col,
            );
        }
    }
}
