use lib::prelude::*;
use lib::ui::mirror::CtrlLabels;
pub use lib::ui::mirror::{Ctrl, Pad};

use crate::ui::{MARGIN, device_header, pane};

/// What each Launch Control XL slider drives, for the tooltips.
const SLIDERS: [&str; 8] = [
    "brightness",
    "beam trim",
    "bigbeam trim",
    "par trim",
    "spider trim",
    "bar trim",
    "strobe trim",
    "global trim",
];

pub fn draw(
    mut ctxs: EguiContexts,
    mut pad: ResMut<Pad>,
    mut ctrl: ResMut<Ctrl>,
    state: Res<crate::logic::State>,
    mut styled: Local<bool>,
) -> Result {
    let ctx = ctxs.ctx_mut()?;
    if !*styled {
        *styled = true;
        // Tooltips are the labels on an unlabelled grid, so they want to be
        // near-instant rather than egui's read-the-docs pause.
        ctx.global_style_mut(|s| s.interaction.tooltip_delay = 0.05);
    }

    // Both sit in the bottom left corner, the control surface beside the pad.
    let pad_pane =
        pane(ctx, "Launchpad X", egui::Align2::LEFT_BOTTOM, egui::Vec2::ZERO).default_width(0.0);
    let pad_shown = pad_pane.show(ctx, |ui| {
        device_header(ui, pad.name(), pad.connected());
        pad.draw(ui, |x, y| state.binding((x, y)).map(|op| op.name()));
    });

    // Whatever width the pad came out, so the two never overlap or leave a gap.
    let beside = pad_shown.map_or(0.0, |pad| pad.response.rect.width() + MARGIN);
    let ctrl_pane = pane(ctx, "Launch Control XL", egui::Align2::LEFT_BOTTOM, egui::vec2(beside, 0.0))
        .default_width(0.0);
    ctrl_pane.show(ctx, |ui| {
        device_header(ui, ctrl.name(), ctrl.connected());
        ctrl.draw(ui, &CtrlLabels {
            sliders: SLIDERS,
            side: ["", "", "", "booth light"],
            ..default()
        });
    });

    Ok(())
}
