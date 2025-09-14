use bevy_egui::egui::{Painter, Rect};

use crate::prelude::*;

pub fn rect(p: &Painter, c: Rgbw, x: f32, y: f32, w: f32, h: f32) {
    let rect =
        Rect::from_center_size(egui::Pos2::new(x as f32, y as f32), egui::Vec2::new(w as f32, h as f32));
    p.rect_filled(rect, egui::CornerRadius::ZERO, c);
}

pub fn rect_stroke(p: &Painter, stroke: f32, c: Rgbw, x: f32, y: f32, w: f32, h: f32) {
    let rect =
        Rect::from_center_size(egui::Pos2::new(x as f32, y as f32), egui::Vec2::new(w as f32, h as f32));
    p.rect_stroke(
        rect,
        egui::CornerRadius::ZERO,
        egui::Stroke::new(stroke as f32, c),
        egui::StrokeKind::Inside,
    );
}

pub fn text(p: &Painter, size: f32, c: Rgbw, x: f32, y: f32, text: impl ToString) {
    p.text(
        egui::Pos2::new(x as f32, y as f32),
        egui::Align2::CENTER_CENTER,
        text,
        egui::FontId::monospace(size as f32),
        c.into(),
    );
}
