//! Loads a .gdtf, assembles the fixture in 3d, and drives it from DMX sliders.

use std::path::PathBuf;

use bevy::camera::primitives::Aabb;
use bevy::camera::{Exposure, Hdr};
use bevy::core_pipeline::prepass::DepthPrepass;
use bevy::core_pipeline::tonemapping::{DebandDither, Tonemapping};
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::math::primitives::{Cone, Cuboid, Plane3d};
use bevy::post_process::bloom::Bloom;
use lib::gdtf::{
    Axle, BeamCone, Emitter, GdtfFixture, GdtfLibrary, GdtfType, Haze, Motor, corners, motion,
};
use lib::prelude::*;

mod dmx;
use dmx::Dmx;


/// Library key for the one fixture this app previews.
pub const KIND: &str = "fixture";

const PI: f32 = std::f32::consts::PI;
const TAU_4: f32 = std::f32::consts::FRAC_PI_2;

/// Tool for previewing a GDTF fixture and its DMX channels
#[derive(argh::FromArgs)]
struct Args {
    /// path to a .gdtf file
    #[argh(positional)]
    path: PathBuf,
    /// serial port of the ENTTEC widget (default: auto-detect)
    #[argh(option)]
    port: Option<String>,
    /// starting levels, as `Attribute=fraction` of each channel's range
    /// (e.g. "Dimmer=1,Zoom=0.5")
    #[argh(option)]
    set: Option<String>,
    /// camera distance in meters (default: frame the fixture)
    #[argh(option)]
    view: Option<f32>,
    /// hang the fixture beam-down instead of standing it on its base
    #[argh(switch)]
    hang: bool,
    /// enable debug logging
    #[argh(switch, short = 'v')]
    debug: bool,
    /// enable trace logging
    #[argh(switch, short = 'V')]
    trace: bool,
}

fn main() -> Result {
    let args: Args = argh::from_env();

    App::new()
        .add_plugins(RavyPlugin { module: module_path!(), debug: args.debug, trace: args.trace })
        .insert_resource(Dmx::spawn(args.port.as_deref()))
        .insert_resource(Rig {
            path: args.path,
            preset: args.set,
            hang: args.hang,
            ev100: Exposure::EV100_INDOOR,
            room: 30.0,
            fitted: false,
        })
        .insert_resource(Framing(args.view))
        .add_systems(Startup, setup)
        .add_systems(Update, (patch, orbit, room_light, dmx::poll, dmx::output))
        .add_systems(PostUpdate, fit.after(TransformSystems::Propagate))
        .add_systems(EguiPrimaryContextPass, draw_ui)
        .run();
    Ok(())
}

/// Everything about the preview that isn't the fixture itself.
#[derive(Resource)]
struct Rig {
    path: PathBuf,
    /// Starting levels from the command line, reapplied on a mode change.
    preset: Option<String>,
    /// Hang the fixture beam-down instead of standing it on its base.
    hang: bool,
    /// Camera exposure. The lights are in real units, so this is the only place
    /// left to trade the room's brightness against the beam's.
    ev100: f32,
    /// House light level in lux. 0 is a blacked-out room.
    room: f32,
    /// Cleared whenever the fixture is respawned or reoriented.
    fitted: bool,
}

/// Camera distance from the command line, which overrides framing the fixture.
#[derive(Resource)]
struct Framing(Option<f32>);

/// The previewed fixture, so its parts can be told apart from the room.
#[derive(Component)]
pub struct Fixture;

/// Load the fixture once the asset server is up, and reorient it on demand.
fn patch(
    mut cmds: Commands,
    mut rig: ResMut<Rig>,
    mut library: ResMut<GdtfLibrary>,
    assets: Res<AssetServer>,
    mut fixture: Option<Single<(&mut GdtfFixture, &mut Transform), With<Fixture>>>,
    mut loaded: Local<bool>,
) -> Result {
    if !*loaded {
        *loaded = true;
        library.load(KIND, &rig.path, None, &assets)?;
        cmds.spawn((Fixture, GdtfFixture::new(KIND, 0), upright(rig.hang), Visibility::default()));
        return Ok(());
    }

    let Some(fixture) = &mut fixture else { return Ok(()) };
    let (fixture, transform) = &mut **fixture;
    let want = upright(rig.hang);
    if transform.rotation != want.rotation {
        **transform = want;
        fixture.rebuild();
        rig.fitted = false;
    }
    // Values land on the first build; the preset goes over the top of them.
    if fixture.is_built() && !rig.fitted && let Some(ty) = library.get(KIND) {
        apply_preset(fixture, ty, rig.preset.as_deref());
    }
    Ok(())
}

/// GDTF authors a fixture hanging, base at the origin with everything below it.
/// Standing it up is a half turn about its own front-back axis, which keeps the
/// front facing forward and mirrors pan and tilt exactly as inverting the real
/// fixture's mount does.
fn upright(hang: bool) -> Transform {
    match hang {
        true => Transform::IDENTITY,
        false => Transform::from_rotation(Quat::from_rotation_z(PI)),
    }
}

/// Apply the command line's starting levels over the homed values.
fn apply_preset(fixture: &mut GdtfFixture, ty: &GdtfType, preset: Option<&str>) {
    let Some(preset) = preset else { return };
    for pair in preset.split(',') {
        let Some((attribute, fraction)) = pair.split_once('=') else {
            continue;
        };
        let Ok(fraction) = fraction.trim().parse::<f32>() else {
            continue;
        };
        for (i, channel) in ty.mode().channels.iter().enumerate() {
            if channel.attribute == attribute.trim() {
                fixture.values[i] = (fraction.clamp(0.0, 1.0) * channel.max() as f32) as u32;
            }
        }
    }
}

#[derive(Resource)]
struct Orbit {
    yaw: f32,
    pitch: f32,
    dist: f32,
    focus: Vec3,
}

fn setup(mut cmds: Commands, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    cmds.spawn((
        Camera3d::default(),
        Camera { clear_color: ClearColorConfig::Custom(Color::BLACK), ..default() },
        Hdr,
        // A blacked-out venue, not bevy's default daylight.
        Exposure { ev100: Exposure::EV100_INDOOR },
        // The beam volume ends its march at this, so a shaft stops
        // at whatever it lands on instead of carrying on through it.
        DepthPrepass,
        Bloom::NATURAL,
        Tonemapping::TonyMcMapface,
        DebandDither::Enabled,
    ));
    cmds.insert_resource(Orbit { yaw: 0.6, pitch: 0.25, dist: 2.0, focus: Vec3::Y });

    // Work light only: the fixture is what's meant to light the room. Ambient
    // alone would render the housing as a flat silhouette, hence the key.
    cmds.spawn((
        WorkLight,
        DirectionalLight { illuminance: 0.0, shadow_maps_enabled: true, ..default() },
        Transform::from_xyz(4.0, 8.0, 6.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    cmds.spawn((
        WorkLight,
        DirectionalLight { illuminance: 0.0, color: Color::srgb(0.7, 0.8, 1.0), ..default() },
        Transform::from_xyz(-6.0, 2.0, -3.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    cmds.insert_resource(GlobalAmbientLight {
        color: Color::srgb(0.6, 0.7, 1.0),
        brightness: 0.0,
        ..default()
    });

    cmds.spawn((
        Mesh3d(meshes.add(Plane3d::new(Vec3::Y, Vec2::splat(15.0)))),
        MeshMaterial3d(mats.add(StandardMaterial {
            base_color: Color::srgb(0.04, 0.04, 0.045),
            perceptual_roughness: 1.0,
            ..default()
        })),
    ));

    spawn_arrow(&mut cmds, &mut meshes, &mut mats);
}

/// House lights, scaled by `Rig::room`.
#[derive(Component)]
struct WorkLight;

/// Floor marker for the fixture's front. GDTF has no explicit front marker,
/// but its axes are x right / y front-back / z up, and the fixture faces -y.
#[derive(Component)]
struct FrontArrow;

fn spawn_arrow(cmds: &mut Commands, meshes: &mut Assets<Mesh>, mats: &mut Assets<StandardMaterial>) {
    let material =
        mats.add(StandardMaterial { base_color: Color::srgb(0.9, 0.5, 0.1), unlit: true, ..default() });
    // Built pointing along -z so `looking_to` aims it.
    cmds.spawn((FrontArrow, Transform::default(), Visibility::default()))
        .with_children(|arrow| {
            arrow.spawn((
                Mesh3d(meshes.add(Cuboid::new(0.06, 0.001, 0.7))),
                MeshMaterial3d(material.clone()),
                Transform::from_xyz(0.0, 0.0, -0.35),
            ));
            arrow.spawn((
                Mesh3d(meshes.add(Cone { radius: 0.11, height: 0.3 })),
                MeshMaterial3d(material),
                Transform::from_xyz(0.0, 0.0, -0.85).with_rotation(Quat::from_rotation_x(-TAU_4)),
            ));
        });
}

fn room_light(
    rig: Res<Rig>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut lights: Query<&mut DirectionalLight, With<WorkLight>>,
    mut exposure: Single<&mut Exposure>,
) {
    if !rig.is_changed() {
        return;
    }
    // Ambient is a luminance, so an evenly lit room's illuminance over pi.
    ambient.brightness = rig.room / PI;
    for (i, mut light) in lights.iter_mut().enumerate() {
        light.illuminance = rig.room * if i == 0 { 1.0 } else { 0.3 };
    }
    exposure.ev100 = rig.ev100;
}

/// Bounds of everything under `entity`, in that entity's own space.
fn local_bounds(
    entity: Entity,
    global: &GlobalTransform,
    children: &Query<&Children>,
    bounds: &Query<(&Aabb, &GlobalTransform), Without<BeamCone>>,
) -> Option<(Vec3, Vec3)> {
    let inverse = global.affine().inverse();
    let (mut min, mut max) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for child in children.iter_descendants(entity) {
        let Ok((aabb, child_global)) = bounds.get(child) else {
            continue;
        };
        for corner in corners(aabb) {
            let point = inverse.transform_point3(child_global.transform_point(corner));
            min = min.min(point);
            max = max.max(point);
        }
    }
    (min.x <= max.x).then_some((min, max))
}

/// Sit the fixture on the floor and frame it, once its meshes have bounds.
fn fit(
    mut rig: ResMut<Rig>,
    mut orbit: ResMut<Orbit>,
    framing: Res<Framing>,
    root: Single<(Entity, &mut Transform), With<Fixture>>,
    children: Query<&Children>,
    bounds: Query<(&Aabb, &GlobalTransform), Without<BeamCone>>,
    mut arrow: Single<&mut Transform, (With<FrontArrow>, Without<Fixture>)>,
) {
    if rig.fitted {
        return;
    }
    let (root, mut transform) = root.into_inner();

    let Some((min, max)) = local_bounds(root, &GlobalTransform::IDENTITY, &children, &bounds) else {
        return; // transforms haven't propagated yet
    };

    let size = max - min;
    transform.translation.y -= min.y;
    orbit.focus = Vec3::new(0.0, size.y / 2.0, 0.0);
    orbit.dist = framing.0.unwrap_or(size.length() * 1.6);
    if let Some(dist) = framing.0 {
        // Far enough out to want the beam in shot, not just the fixture.
        orbit.focus.y = dist / 3.0;
    }

    // Local +z is GDTF -y, the front; the upright flip turns it around.
    let front = (transform.rotation * Vec3::Z).with_y(0.0);
    let front = match front.length_squared() > 1e-6 {
        true => front.normalize(),
        false => Vec3::Z,
    };
    let reach = size.x.max(size.z) / 2.0;
    **arrow = Transform::from_xyz(0.0, 0.002, 0.0)
        .looking_to(front, Vec3::Y)
        .with_scale(Vec3::splat(reach * 3.0));

    rig.fitted = true;
}

fn orbit(
    mut orbit: ResMut<Orbit>,
    mut camera: Single<&mut Transform, With<Camera3d>>,
    mut egui_ctx: EguiContexts,
    buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
) -> Result {
    // Hover, not focus: wants_pointer_input() stays true after any widget
    // drag and would swallow the scroll wheel.
    let ctx = egui_ctx.ctx_mut()?;
    let over_ui = ctx.is_pointer_over_egui();
    // A drag begun on a widget keeps the pointer even once it leaves the
    // panel, e.g. pulling a slider or a window's resize edge.
    let using_ui = ctx.egui_is_using_pointer();

    if !over_ui && !using_ui && buttons.pressed(MouseButton::Left) {
        orbit.yaw -= motion.delta.x * 0.005;
        orbit.pitch = (orbit.pitch - motion.delta.y * 0.005).clamp(-1.5, 1.5);
    }
    if !over_ui && scroll.delta.y != 0.0 {
        orbit.dist = (orbit.dist * (1.0 - scroll.delta.y.clamp(-3.0, 3.0) * 0.12)).clamp(0.05, 200.0);
    }

    let offset = Quat::from_rotation_y(orbit.yaw) * Quat::from_rotation_x(orbit.pitch) * Vec3::Z;
    camera.translation = orbit.focus + offset * orbit.dist;
    camera.look_at(orbit.focus, Vec3::Y);
    Ok(())
}

/// What the brightest emitter is putting out, in the terms a photometric
/// report uses.
fn readout(emitters: &Query<&Emitter>) -> String {
    let cells = || emitters.iter().filter(|emitter| !emitter.glow);
    let Some(brightest) = cells().max_by(|a, b| a.peak.total_cmp(&b.peak)) else {
        return String::new();
    };
    // Cells of an array all point the same way, so their intensities add.
    let peak: f32 = cells().map(|emitter| emitter.peak).sum();
    format!(
        "{peak:.0} cd peak  |  {:.0} lx at 5 m  |  beam {:.1}\u{b0}  field {:.1}\u{b0}",
        peak / 25.0,
        brightest.angles.x,
        brightest.angles.y,
    )
}

fn draw_ui(
    mut egui_ctx: EguiContexts,
    mut rig: ResMut<Rig>,
    mut haze: ResMut<Haze>,
    mut library: ResMut<GdtfLibrary>,
    mut dmx: ResMut<Dmx>,
    time: Res<Time>,
    emitters: Query<&Emitter>,
    motors: Query<&Motor>,
    fixture: Option<Single<&mut GdtfFixture, With<Fixture>>>,
    mut view: Local<motion::View>,
    mut fps: Local<f32>,
    mut shrunk: Local<bool>,
) -> Result {
    let Some(mut fixture) = fixture else { return Ok(()) };
    let Some(ty) = library.get(KIND) else { return Ok(()) };
    if !fixture.is_built() {
        return Ok(());
    }

    let output = readout(&emitters);
    // Worth showing: the motion charts are sampled once a frame, so a slow
    // frame rate is what a coarse trace looks like.
    let dt = time.delta_secs();
    if dt > 0.0 {
        *fps += (1.0 / dt - *fps) * (dt * 2.0).min(1.0);
    }
    let ctx = egui_ctx.ctx_mut()?;
    // Smaller than egui's default, to leave more of the fixture visible.
    if !*shrunk {
        ctx.set_zoom_factor(0.8);
        *shrunk = true;
    }

    let (name, manufacturer) = (ty.gdtf.name.clone(), ty.gdtf.manufacturer.clone());
    let mut mode = ty.mode;
    let mut rehome = false;

    egui::Window::new(format!("{manufacturer} {name}")).default_width(520.0).show(ctx, |ui| {
        let ty = library.get(KIND).unwrap();
        ui.horizontal(|ui| {
            let order = modes_by_personality(ty, dmx.target());
            let label = |i: usize| {
                let m = &ty.gdtf.modes[i];
                match personality(ty, dmx.target(), i) {
                    Some(n) => format!("{n}. {} ({}ch)", m.name, m.footprint()),
                    None => format!("-  {} ({}ch)", m.name, m.footprint()),
                }
            };
            egui::ComboBox::from_label("Mode").selected_text(label(ty.mode)).show_ui(ui, |cb| {
                for i in order {
                    cb.selectable_value(&mut mode, i, label(i));
                }
            });
            if ui.button("Home").clicked() {
                rehome = true;
            }
            if ui.button("Blackout").clicked() {
                fixture.values.fill(0);
            }
            ui.checkbox(&mut rig.hang, "Hang");
        });

        ui.horizontal(|ui| {
            ui.add(egui::Slider::new(&mut rig.ev100, 2.0..=14.0).text("Exposure (EV100)"));
            ui.add(egui::Slider::new(&mut rig.room, 0.0..=500.0).text("Room (lux)"));
        });
        ui.horizontal(|ui| {
            // Clean desert air scatters a thousandth of what show haze does.
            ui.add(
                egui::Slider::new(&mut haze.0, 0.0001..=0.5)
                    .logarithmic(true)
                    .custom_formatter(|v, _| format!("{v:.4}"))
                    .text("Haze (1/m)"),
            );
            if !ty.photometry.is_measured() {
                ui.label("GDTF photometry only: no zoom sweep for this fixture");
            }
        });
        ui.horizontal(|ui| {
            ui.label(output.to_string());
            ui.label(format!("|  {:.0} fps", *fps));
        });

        ui.collapsing("Calibration", |ui| {
            ui.label("Model only, never sent on the wire.");
            egui::Grid::new("calibration").num_columns(3).show(ui, |ui| {
                for (axle, name) in [(0, "Pan"), (1, "Tilt")] {
                    ui.label(name);
                    ui.add(egui::DragValue::new(&mut fixture.home[axle]).speed(1.0).suffix("° home"));
                    ui.checkbox(&mut fixture.invert[axle], "invert");
                    ui.end_row();
                }
            });
        });

        ui.separator();
        output_ui(ui, ty, &mut dmx);

        ui.separator();
        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("channels").num_columns(4).striped(true).show(ui, |ui| {
                let values = &mut fixture.values;
                for (i, channel) in ty.mode().channels.iter().enumerate() {
                    ui.label(channel.label());
                    match is_level(&channel.attribute) {
                        true => level_slider(ui, &mut values[i], channel.max()),
                        // smart_aim snaps 16-bit drags to round thousands.
                        false => {
                            ui.add(
                                egui::Slider::new(&mut values[i], 0..=channel.max())
                                    .smart_aim(false)
                                    .show_value(false),
                            );
                        }
                    }
                    ui.add(egui::DragValue::new(&mut values[i]).range(0..=channel.max()));
                    let function = channel.function(values[i]);
                    let slot = channel.slot(values[i]).and_then(|(w, s)| ty.gdtf.wheels.get(w)?.get(s));
                    ui.label(match slot {
                        Some(slot) => format!("{} = {}", function.name, slot.name),
                        None => format!("{} = {:.2}", function.name, channel.physical(values[i])),
                    });
                    ui.end_row();
                }
            });
        });
    });

    // Charts consume all leftover height, which is what makes the height
    // resizable: the window never auto-shrinks back around its content.
    let screen = ctx.content_rect();
    egui::Window::new("Motion")
        .default_size([780.0, 900.0])
        .default_pos([screen.right() - 800.0, 30.0])
        .resizable(true)
        .show(ctx, |ui| {
            let ty = library.get(KIND).unwrap();
            let channels = &ty.mode().channels;

            // Dedicated pan/tilt sliders, same values as the channel grid.
            // Scoped: a leaked slider width feeds back through the window's
            // auto-size and grows it without bound.
            ui.scope(|ui| {
                ui.spacing_mut().slider_width = ui.available_width() - 90.0;
                egui::Grid::new("motion_sliders").num_columns(2).show(ui, |ui| {
                    for attribute in ["Pan", "Tilt", "Zoom"] {
                        let Some(i) = channels.iter().position(|c| c.attribute == attribute) else {
                            continue;
                        };
                        ui.label(attribute);
                        ui.add(
                            egui::Slider::new(&mut fixture.values[i], 0..=channels[i].max())
                                .smart_aim(false)
                                .show_value(false),
                        );
                        ui.end_row();
                    }
                });
            });

            ui.collapsing("Tuning", |ui| {
                let [pan, tilt, zoom] = &mut fixture.motion;
                motion::tune(ui, "Pan", "°", pan);
                motion::tune(ui, "Tilt", "°", tilt);
                motion::tune(ui, "Zoom", "%", zoom);
            });

            let params = fixture.motion;
            let mut axes: Vec<_> = motors
                .iter()
                .map(|m| {
                    let name = match m.kind {
                        Axle::Pan => "Pan",
                        Axle::Tilt => "Tilt",
                        Axle::Zoom => "Zoom",
                    };
                    (m.kind as usize, name, &m.trace)
                })
                .collect();
            axes.sort_by_key(|&(order, ..)| order);
            let axes: Vec<_> =
                axes.into_iter().map(|(order, name, trace)| (name, trace, params[order])).collect();
            motion::plot(ui, &mut view, &axes);
        });

    if mode != library.get(KIND).unwrap().mode {
        library.get_mut(KIND).unwrap().mode = mode;
        fixture.rebuild();
        rig.fitted = false;
    } else if rehome {
        let ty = library.get(KIND).unwrap();
        fixture.home(ty);
        apply_preset(&mut fixture, ty, rig.preset.as_deref());
    }
    Ok(())
}

/// Channels that set an amount of light, where the eye's response is closer to
/// logarithmic than the wire's.
fn is_level(attribute: &str) -> bool {
    attribute == "Dimmer" || attribute.starts_with("ColorAdd_") || attribute.starts_with("ColorSub_")
}

/// Slider whose travel is the cube of the level it sets, so the bottom percent
/// of a dimmer gets a fifth of the bar instead of the single pixel a linear
/// 16-bit channel gives it. Writes back only while being dragged, so the round
/// trip through the cube cannot walk the value.
fn level_slider(ui: &mut egui::Ui, value: &mut u32, max: u32) {
    let full = max as f32;
    let mut travel = (*value as f32 / full).cbrt();
    let slider = egui::Slider::new(&mut travel, 0.0..=1.0).show_value(false);
    if ui.add(slider).changed() {
        *value = (travel.powi(3) * full).round() as u32;
    }
}

/// RDM personality number for a mode. The fixture's own list wins; a GDTF's
/// `<FTRDM>` map is often stale (the outcast's omits 37Ch entirely).
fn personality(ty: &GdtfType, target: Option<&dmx::Responder>, mode: usize) -> Option<u8> {
    let footprint = ty.gdtf.modes[mode].footprint() as u16;
    if let Some(target) = target
        && let Some((n, _, _)) = target.personalities.iter().find(|(_, _, f)| *f == footprint)
    {
        return Some(*n);
    }
    let rdm = ty.gdtf.rdm.as_ref()?;
    rdm.personalities.get(&ty.gdtf.modes[mode].name).copied()
}

fn modes_by_personality(ty: &GdtfType, target: Option<&dmx::Responder>) -> Vec<usize> {
    let mut order: Vec<usize> = (0..ty.gdtf.modes.len()).collect();
    order.sort_by_key(|&i| (personality(ty, target, i).unwrap_or(u8::MAX), ty.gdtf.modes[i].footprint()));
    order
}

fn output_ui(ui: &mut egui::Ui, ty: &GdtfType, dmx: &mut Dmx) {
    ui.horizontal(|ui| {
        ui.label(format!("DMX out: {}", dmx.status));
        if dmx.present() {
            if ui.add_enabled(!dmx.discovering, egui::Button::new("Rescan")).clicked() {
                dmx.rescan();
            }
            if dmx.discovering {
                ui.spinner();
            }
        }
    });

    if !dmx.present() {
        return;
    }
    if dmx.responders.is_empty() && !dmx.discovering {
        ui.label("no RDM responders found");
        return;
    }

    let mut selected = dmx.selected;
    for (i, r) in dmx.responders.iter().enumerate() {
        let label =
            format!("{}  {}  @{}  {}ch  pers {}", r.uid, r.model, r.address, r.footprint, r.personality);
        ui.radio_value(&mut selected, Some(i), label);
    }
    dmx.selected = selected;

    let Some(target) = dmx.target().cloned() else {
        return;
    };
    ui.checkbox(&mut dmx.enabled, format!("Send to {} @ {}", target.uid, target.address));

    // Discovery already switched to the fixture's personality; warn only if
    // the mode was then changed by hand.
    let footprint = ty.mode().footprint();
    if footprint != target.active_footprint() as usize {
        ui.colored_label(
            egui::Color32::YELLOW,
            format!(
                "fixture is on personality {} ({}ch), this mode is {footprint}ch",
                target.personality,
                target.active_footprint()
            ),
        );
    }
}
