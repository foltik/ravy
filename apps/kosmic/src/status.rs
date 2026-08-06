//! Debug view: what the composed look currently resolves to, and the two knobs
//! the sim needs that the show doesn't.

use lib::prelude::*;

use crate::logic::{STYLES, State};
use crate::sim::Room;
use crate::ui::{knob, knob_log};

const SWATCH: f32 = 26.0;

/// Scattering coefficients to hang the haze slider off, per metre. See [`Haze`].
const AIR: [(&str, f32); 5] =
    [("clean", 1e-5), ("desert", 5e-4), ("dusty", 5e-3), ("hazer", 2e-2), ("fog", 2e-1)];

pub fn draw(
    mut ctxs: EguiContexts,
    s: Res<State>,
    mut room: ResMut<Room>,
    mut haze: ResMut<Haze>,
) -> Result {
    let ctx = ctxs.ctx_mut()?;
    let look = s.look();

    egui::Window::new("Kosmic").default_width(320.0).default_pos([420.0, 24.0]).show(ctx, |ui| {
        ui.spacing_mut().item_spacing = egui::vec2(8.0, 6.0);

        ui.horizontal(|ui| {
            chip(ui, &format!("{:?}", s.mode), egui::Color32::from_rgb(90, 80, 140));
            chip(ui, &format!("{:.1} bpm", s.bpm), egui::Color32::from_gray(55));
            if s.phi_mul != 1.0 {
                chip(ui, &format!("x{:.2}", s.phi_mul), egui::Color32::from_gray(55));
            }
        });
        beats(ui, s.phi);

        ui.separator();
        ui.horizontal(|ui| {
            swatch(ui, look.beam, "beam");
            swatch(ui, look.par, "par");
            ui.vertical(|ui| {
                ui.label(format!("palette  {}", s.palette.name()));
                ui.label(format!("bright   {:.0}%", s.brightness * 100.0));
            });
        });

        ui.separator();
        egui::Grid::new("layers").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
            let t = &look.texture;
            row(ui, "energy", &format!("{:?}", look.energy));
            row(ui, "movement", &format!("{:?}", look.movement));
            row(ui, "style", &format!("{} (seed {})", STYLES[s.style].name, s.seed));
            row(ui, "texture", &format!("zoom {:.2}  focus {:.2}  {:?}", t.zoom, t.focus, t.ring));
            row(ui, "optics", &format!("gobo {}  prism {}  frost {:.2}", t.gobo, t.prism, t.frost));
        });

        ui.separator();
        ui.horizontal_wrapped(|ui| {
            ui.label("specials");
            match s.specials.is_empty() {
                true => {
                    ui.weak("none");
                }
                false => {
                    for a in &s.specials {
                        let Rgbw(r, g, b, _) = a.special.color;
                        chip(ui, a.special.name, rgb(r, g, b));
                    }
                }
            }
        });

        ui.separator();
        knob(ui, "room lx", &mut room.0, 0.0..=200.0);
        // Four decades of air, so logarithmic. The far left is still exactly
        // zero, which is the one setting with no beams at all.
        knob_log(ui, "haze /m", &mut haze.0, 0.0..=0.5);
        ui.horizontal_wrapped(|ui| {
            for (name, sigma) in AIR {
                if ui.selectable_label(haze.0 == sigma, name).clicked() {
                    haze.0 = sigma;
                }
            }
        });
    });
    Ok(())
}

fn rgb(r: f32, g: f32, b: f32) -> egui::Color32 {
    egui::Color32::from_rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
}

/// A rounded pill, matching the pads in the mirror.
fn chip(ui: &mut egui::Ui, text: &str, fill: egui::Color32) {
    let galley = ui.painter().layout_no_wrap(
        text.to_string(),
        egui::FontId::proportional(12.0),
        egui::Color32::from_gray(230),
    );
    let size = galley.size() + egui::vec2(14.0, 6.0);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    ui.painter().rect_filled(rect, 8.0, fill);
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, egui::Color32::PLACEHOLDER);
}

/// The colour a group of fixtures is being sent, with its white channel as an
/// inset bar since the swatch itself can only show rgb.
fn swatch(ui: &mut egui::Ui, Rgbw(r, g, b, w): Rgbw, label: &str) {
    ui.vertical(|ui| {
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(SWATCH, SWATCH), egui::Sense::hover());
        ui.painter().rect_filled(rect, 4.0, rgb(r, g, b));
        let white = egui::Rect::from_min_size(
            rect.left_bottom() - egui::vec2(0.0, 5.0),
            egui::vec2(rect.width() * w, 5.0),
        );
        ui.painter().rect_filled(white, 0.0, egui::Color32::WHITE);
        ui.painter().rect_stroke(
            rect,
            4.0,
            egui::Stroke::new(1.0_f32, egui::Color32::from_gray(60)),
            egui::StrokeKind::Inside,
        );
        ui.weak(label);
    });
}

/// Four beats of the bar, the current one lit.
fn beats(ui: &mut egui::Ui, phi: f32) {
    let beat = phi as usize % 4;
    let fr = phi.fract();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for i in 0..4 {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(28.0, 8.0), egui::Sense::hover());
            let level = match i == beat {
                true => 1.0 - fr * 0.7,
                false => 0.18,
            };
            let v = (level * 255.0) as u8;
            ui.painter().rect_filled(rect, 3.0, egui::Color32::from_rgb(v, v / 2, v));
        }
    });
}

fn row(ui: &mut egui::Ui, name: &str, value: &str) {
    ui.weak(name);
    ui.label(value);
    ui.end_row();
}
