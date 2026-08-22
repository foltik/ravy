//! Debug view: what the composed look currently resolves to, and the trims.

use lib::prelude::*;

use crate::ledwall::{
    self, BallParams, BoxesParams, CubeParams, Fx, KaleidoParams, Pattern, RingsParams,
    Screen, ScreenParams, SpiralParams, WormholeParams,
};
use crate::logic::{STYLES, State};
use crate::rig::{Rig, Trim};
use crate::sim::Room;
use crate::ui::{knob, knob_log, pane};

const SWATCH: f32 = 26.0;

/// Scattering coefficients to hang the haze slider off, per metre. See [`Haze`].
const AIR: [(&str, f32); 5] =
    [("clean", 1e-5), ("desert", 5e-4), ("dusty", 5e-3), ("hazer", 2e-2), ("fog", 2e-1)];

pub fn draw(
    mut ctxs: EguiContexts,
    mut cmds: Commands,
    s: Res<State>,
    mut trim: ResMut<Trim>,
    mut room: ResMut<Room>,
    mut haze: ResMut<Haze>,
    mut screen: ResMut<Screen>,
    mut pattern: ResMut<ScreenParams>,
    mut cube: ResMut<CubeParams>,
    mut fx: ResMut<Fx>,
    patterns: (
        ResMut<SpiralParams>,
        ResMut<BallParams>,
        ResMut<WormholeParams>,
        ResMut<KaleidoParams>,
        ResMut<RingsParams>,
        ResMut<BoxesParams>,
    ),
) -> Result {
    let (mut spiral, mut ball, mut worm, mut kal, mut rings, mut boxes) = patterns;
    let ctx = ctxs.ctx_mut()?;
    let look = s.look();

    let panel = pane(ctx, "Jetlag", egui::Align2::LEFT_TOP, egui::Vec2::ZERO);
    panel.default_width(320.0).show(ctx, |ui| {
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
                ui.label(format!("palette  {} ({})", s.palette.name(), s.banks[s.bank].name));
                ui.label(format!("bright   {:.0}%", s.brightness * 100.0));
            });
        });

        ui.separator();
        egui::Grid::new("layers").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
            row(ui, "energy", &format!("{:?}", look.energy));
            row(ui, "movement", &format!("{:?}", look.movement));
            row(ui, "style", &format!("{} (seed {})", STYLES[s.style].name, s.seed));
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

        // Trim: what each family is worth once the show has had its say, for
        // matching fixtures that are nowhere near each other in output.
        ui.separator();
        knob(ui, "global", &mut trim.global, 0.0..=1.0);
        knob(ui, "par", &mut trim.par, 0.0..=1.0);
        knob(ui, "beam", &mut trim.beam, 0.0..=1.0);
        knob(ui, "bigbeam", &mut trim.bigbeam, 0.0..=1.0);
        knob(ui, "spider", &mut trim.spider, 0.0..=1.0);
        knob(ui, "bar", &mut trim.bar, 0.0..=1.0);
        knob(ui, "strobe", &mut trim.strobe, 0.0..=1.0);
        knob(ui, "hut", &mut trim.hut, 0.0..=1.0);

        ui.separator();
        knob(ui, "room lx", &mut room.0, 0.0..=200.0);

        ui.collapsing("screen", |ui| {
            ui.horizontal_wrapped(|ui| {
                for (name, p) in [
                    ("isolines", Pattern::Isolines),
                    ("cuberoom", Pattern::CubeRoom),
                    ("spiral", Pattern::Spiral),
                    ("ball", Pattern::Ball),
                    ("wormhole", Pattern::Wormhole),
                    ("kaleido", Pattern::Kaleido),
                    ("rings", Pattern::Rings),
                    ("boxes", Pattern::Boxes),
                ] {
                    if ui.selectable_label(screen.pattern == p, name).clicked() {
                        screen.pattern = p;
                    }
                }
            });
            // Where this preset sits against the others, applied in the fx
            // pass on top of the show's master brightness.
            knob(ui, "trim", &mut fx.of(screen.pattern).trim, 0.0..=2.0);
            match screen.pattern {
                Pattern::Isolines => {
                    knob(ui, "drift", &mut pattern.drift, 0.0..=0.2);
                    knob(ui, "kick", &mut pattern.kick, 0.0..=3.0);
                    knob(ui, "snap", &mut pattern.snap, 1.0..=10.0);
                    knob(ui, "settle", &mut pattern.settle, 0.25..=1.0);
                    knob(ui, "wave", &mut pattern.wave, 0.0..=0.6);
                    knob(ui, "sway", &mut pattern.sway, 0.0..=0.5);
                    knob(ui, "cam", &mut pattern.cam, 0.5..=6.0);
                    knob(ui, "fade", &mut pattern.fade, 0.0..=1.5);
                    knob(ui, "lines", &mut pattern.lines, 4.0..=30.0);
                    knob(ui, "thick", &mut pattern.thick, 0.4..=1.0);
                    knob(ui, "pump", &mut pattern.pump, 0.0..=0.5);
                }
                Pattern::CubeRoom => {
                    knob(ui, "spin", &mut cube.spin, 0.0..=3.0);
                    knob(ui, "surge", &mut cube.surge, 0.0..=20.0);
                    knob(ui, "pump", &mut cube.pump, 0.0..=0.3);
                    knob(ui, "mega", &mut cube.mega, 0.0..=2.0);
                    knob(ui, "wire", &mut cube.wire, 0.01..=0.3);
                }
                Pattern::Spiral => {
                    knob(ui, "swirl", &mut spiral.swirl, 0.0..=3.0);
                    knob(ui, "speed", &mut spiral.speed, 0.0..=30.0);
                    knob(ui, "spokes", &mut spiral.spokes, 2.0..=12.0);
                    knob(ui, "cutoff", &mut spiral.cutoff, 0.0..=1.0);
                    knob(ui, "amount", &mut spiral.amount, 0.0..=2.0);
                    knob(ui, "pulse", &mut spiral.pulse, 0.0..=2.0);
                    knob(ui, "width", &mut spiral.width, 0.05..=0.95);
                    knob(ui, "every", &mut spiral.every, 1.0..=16.0);
                }
                Pattern::Ball => {
                    knob(ui, "size", &mut ball.size, 0.2..=3.0);
                    knob(ui, "checks", &mut ball.checks, 2.0..=16.0);
                    knob(ui, "spin", &mut ball.spin, 0.0..=10.0);
                    knob(ui, "boost", &mut ball.boost, 0.0..=20.0);
                    knob(ui, "brake", &mut ball.brake, 1.0..=60.0);
                    knob(ui, "every", &mut ball.every, 1.0..=16.0);
                }
                Pattern::Wormhole => {
                    knob(ui, "speed", &mut worm.speed, 0.0..=2.0);
                    knob(ui, "surge", &mut worm.surge, 0.0..=8.0);
                    knob(ui, "warp", &mut worm.warp, 0.3..=0.9);
                    knob(ui, "pulse", &mut worm.pulse, 0.0..=0.3);
                    knob(ui, "every", &mut worm.every, 1.0..=16.0);
                }
                Pattern::Kaleido => {
                    knob(ui, "speed", &mut kal.speed, 0.0..=2.0);
                    knob(ui, "surge", &mut kal.surge, 0.0..=8.0);
                    knob(ui, "zoom", &mut kal.zoom, 0.1..=1.5);
                    knob(ui, "breathe", &mut kal.breathe, 0.0..=0.2);
                    knob(ui, "sharp", &mut kal.sharp, 2.0..=24.0);
                    knob(ui, "fade", &mut kal.fade, 0.0..=2.0);
                    knob(ui, "iters", &mut kal.iters, 2.0..=8.0);
                    knob(ui, "every", &mut kal.every, 1.0..=16.0);
                }
                Pattern::Rings => {
                    knob(ui, "reach", &mut rings.reach, 0.3..=2.5);
                    knob(ui, "width", &mut rings.width, 0.01..=0.2);
                    knob(ui, "life", &mut rings.life, 1.0..=6.0);
                    knob(ui, "glow", &mut rings.glow, 0.0..=2.0);
                    knob(ui, "snap", &mut rings.snap, 1.0..=8.0);
                    knob(ui, "wob", &mut rings.wob, 0.0..=0.5);
                    knob(ui, "pump", &mut rings.pump, 0.0..=1.0);
                    knob(ui, "every", &mut rings.every, 1.0..=16.0);
                }
                Pattern::Boxes => {
                    knob(ui, "speed", &mut boxes.speed, 0.0..=2.0);
                    knob(ui, "surge", &mut boxes.surge, 0.0..=6.0);
                    knob(ui, "twist", &mut boxes.twist, 0.0..=2.0);
                    knob(ui, "density", &mut boxes.density, 1.0..=8.0);
                    knob(ui, "thick", &mut boxes.thick, 0.02..=0.4);
                    knob(ui, "cutoff", &mut boxes.cutoff, 0.0..=1.0);
                    knob(ui, "every", &mut boxes.every, 1.0..=16.0);
                }
            }
        });
        // The chain rides the selected pattern: these edit its settings.
        // Ranges follow each amount's reach in the shader: glitch, edge and
        // invert saturate at 1, the uv warps keep going.
        ui.collapsing("fx", |ui| {
            let f = fx.of(screen.pattern);
            knob(ui, "glitch", &mut f.glitch, 0.0..=1.0);
            knob(ui, "vhs", &mut f.vhs, 0.0..=2.0);
            knob(ui, "shake", &mut f.shake, 0.0..=3.0);
            knob(ui, "mega", &mut f.mega, 0.0..=2.0);
            knob(ui, "pause", &mut f.pause, 0.0..=2.0);
            knob(ui, "edge", &mut f.edge, 0.0..=1.0);
            knob(ui, "invert", &mut f.invert, 0.0..=1.0);
            knob(ui, "red", &mut f.red, 0.0..=1.0);
            knob(ui, "flash", &mut f.flash, 0.0..=1.0);
        });
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

        ui.horizontal(|ui| {
            if ui
                .add_enabled(screen.output.is_none(), egui::Button::new("Screen"))
                .clicked()
            {
                ledwall::popout(&mut cmds, &mut screen);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Save").clicked()
                    && let Err(e) = trim.save(Rig {
                        room: &mut room,
                        haze: &mut haze,
                        iso: &mut pattern,
                        cube: &mut cube,
                        spiral: &mut spiral,
                        ball: &mut ball,
                        worm: &mut worm,
                        kal: &mut kal,
                        rings: &mut rings,
                        boxes: &mut boxes,
                        fx: &mut fx,
                    })
                {
                    warn!("failed to save rig: {e}");
                }
                if ui.button("Reload").clicked() {
                    trim.reload(Rig {
                        room: &mut room,
                        haze: &mut haze,
                        iso: &mut pattern,
                        cube: &mut cube,
                        spiral: &mut spiral,
                        ball: &mut ball,
                        worm: &mut worm,
                        kal: &mut kal,
                        rings: &mut rings,
                        boxes: &mut boxes,
                        fx: &mut fx,
                    });
                }
            });
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
