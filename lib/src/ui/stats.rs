//! Frame rate readout, pinned to the bottom right corner.

use std::collections::VecDeque;

use crate::prelude::*;

/// How long a reading is held before the next one replaces it. Anything shorter
/// changes faster than it can be read.
const WINDOW: f32 = 1.0;

/// Frames the histogram holds, and how wide each of their bars is.
const HISTORY: usize = 120;
const BAR: f32 = 2.0;
const HEIGHT: f32 = 32.0;

/// Digits the reading is given room for, so it stops moving the histogram when
/// the frame rate crosses a power of ten.
const DIGITS: usize = 3;

/// Frame time the histogram tops out at, in ms: 30fps, where a bar reaching the
/// top means the frame was at least that late.
const CEILING: f32 = 1000.0 / 30.0;

#[derive(Resource, Default)]
pub struct Frames {
    elapsed: f32,
    counted: u32,
    shown: f32,
    /// Recent frame times in ms, oldest first.
    history: VecDeque<f32>,
}

pub fn draw(mut ctxs: EguiContexts, time: Res<Time>, mut frames: ResMut<Frames>) -> Result {
    let ctx = ctxs.ctx_mut()?;

    let dt = time.delta_secs();
    frames.elapsed += dt;
    frames.counted += 1;
    if frames.elapsed >= WINDOW {
        frames.shown = frames.counted as f32 / frames.elapsed;
        (frames.elapsed, frames.counted) = (0.0, 0);
    }

    frames.history.push_back(dt * 1000.0);
    while frames.history.len() > HISTORY {
        frames.history.pop_front();
    }

    egui::Area::new(egui::Id::new("stats"))
        .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-8.0, -8.0))
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    histogram(ui, &frames.history);
                    reading(ui, frames.shown);
                });
            });
        });
    Ok(())
}

/// Frame times as bars, newest on the right, over a line at 60fps. What the
/// average hides: one long frame is a visible hitch.
fn histogram(ui: &mut egui::Ui, history: &VecDeque<f32>) {
    let width = HISTORY as f32 * BAR;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, HEIGHT), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 2.0, egui::Color32::from_black_alpha(160));

    let mark = rect.bottom() - HEIGHT * (1000.0 / 60.0) / CEILING;
    painter.hline(rect.x_range(), mark, egui::Stroke::new(1.0_f32, egui::Color32::from_gray(70)));

    for (i, &ms) in history.iter().enumerate() {
        // Right-aligned, so a partly filled history grows from the newest end.
        let x = rect.right() - (history.len() - i) as f32 * BAR;
        let height = (ms / CEILING).clamp(0.0, 1.0) * HEIGHT;
        let bar = egui::Rect::from_min_max(
            egui::pos2(x, rect.bottom() - height),
            egui::pos2(x + BAR - 0.5, rect.bottom()),
        );
        painter.rect_filled(bar, 0.0, rate(1000.0 / ms.max(1e-3)));
    }
}

/// The averaged rate, filling the height the histogram set.
fn reading(ui: &mut egui::Ui, fps: f32) {
    let font = egui::FontId::monospace(HEIGHT * 0.75);
    let widest = "0".repeat(DIGITS);
    let width = ui.painter().layout_no_wrap(widest, font.clone(), egui::Color32::WHITE).rect.width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, HEIGHT), egui::Sense::hover());
    let text = format!("{fps:.0}");
    ui.painter().text(rect.right_center(), egui::Align2::RIGHT_CENTER, text, font, rate(fps));
}

/// Green down through to red across the 60fps mark.
fn rate(fps: f32) -> egui::Color32 {
    let good = (fps / 60.0).clamp(0.0, 1.0);
    match good >= 0.5 {
        true => egui::Color32::from_rgb((255.0 - 165.0 * (good - 0.5) * 2.0) as u8, 230, 110),
        false => egui::Color32::from_rgb(255, (80.0 + 300.0 * good) as u8, 90),
    }
}
