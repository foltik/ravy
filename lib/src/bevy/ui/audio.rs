use bevy_egui::egui::{self, RichText};

use crate::bevy::widgets::LevelMeter;
use crate::prelude::*;

#[derive(Component)]
struct AudioUi;

pub fn setup(mut cmds: Commands) {
    // Assumes AudioPeakHold + AudioVU update elsewhere.
    cmds.spawn((AudioUi, AudioPeakHold::default(), AudioVU::default()));
}

pub fn draw(ui: &mut egui::Ui, world: &mut World) {
    let vu = world.query_filtered::<&AudioVU, With<AudioUi>>().single(world).unwrap().value();
    let peak_hold = world
        .query_filtered::<&AudioPeakHold, With<AudioUi>>()
        .single(world)
        .unwrap()
        .value();

    let mut audio = world.resource_mut::<Audio>();

    let rms = linear_to_dbfs(audio.rms());
    let peak = linear_to_dbfs(audio.peak());
    let peak_hold = linear_to_dbfs(peak_hold);

    ui.horizontal(|ui| {
        // Left: the two meters (tight stack)
        ui.scope(|ui| {
            ui.vertical(|ui| {
                ui.label("VU");
                LevelMeter {
                    min: -20.0,
                    max: 6.0,
                    yellow_start: -3.0,
                    red_start: 3.0,
                    size_px: egui::vec2(18.0, 140.0),
                    pad_px: 3.0,
                }
                .draw(ui, vu, None);
            });
            ui.vertical(|ui| {
                ui.label("dBFS");
                LevelMeter {
                    min: -30.0,
                    max: 0.0,
                    yellow_start: -12.0,
                    red_start: -6.0,
                    size_px: egui::vec2(18.0, 140.0),
                    pad_px: 3.0,
                }
                .draw(ui, peak, Some(peak_hold));
            });

            // Peak column
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Input").weak());
                    widgets::dropdown_opt(ui, "audio_input", &mut audio.input, Audio::available_inputs());
                });

                ui.horizontal(|ui| {
                    ui.label(RichText::new("Output").weak());
                    widgets::dropdown_opt(ui, "audio_output", &mut audio.output, Audio::available_outputs());
                });

                ui.add_space(2.0);

                ui.label(
                    RichText::new(format!("VU: {:+.1} (-14 dBFS ref)", vu))
                        .monospace()
                        .size(11.0)
                        .weak(),
                );
                ui.label(
                    RichText::new(format!("Peak: {:+.1} dBFS", peak_hold))
                        .monospace()
                        .size(11.0)
                        .weak(),
                );
                ui.label(RichText::new(format!("RMS: {:+.1} dBFS", rms)).monospace().size(11.0).weak());
            });
        });
    });
}
