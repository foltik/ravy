//! The venue in 3d: the stage model, and a GDTF fixture at every mount point.
//!
//! `Kosmic.glb` carries an empty per fixture, named for the struct that drives
//! it and indexed to match the address tables in [`crate::lights`]. The empty's
//! transform is the GDTF root frame: its origin is where the fixture sits, and
//! -y is the beam at home. Fixtures read the same universe the ENTTEC does, so
//! what the sim shows is what the wire says.

use bevy::camera::{Exposure, Hdr, PerspectiveProjection, Projection};
use bevy::core_pipeline::prepass::DepthPrepass;
use bevy::core_pipeline::tonemapping::{DebandDither, Tonemapping};
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::post_process::bloom::Bloom;
use lib::gdtf::{GdtfDevice, Standing};
use lib::lights::fixture::{ColoradoSolo, HydroSpot, OutcastBeamwash};
use lib::prelude::*;

use crate::dmx::Patch;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/fixtures");

/// House light level in lux. The show is what's meant to light the stage, so
/// this only keeps the structure from going pitch black.
#[derive(Resource)]
pub struct Room(pub f32);

impl Default for Room {
    fn default() -> Self {
        Self(60.0)
    }
}

/// Vertical field of view. A 28mm lens on full frame, which is about what the
/// room looks like from the front of the crowd.
const FOV: f32 = 46.0;

#[derive(Component, Clone)]
pub struct Stage;

/// Set once the glb's fixture empties have been patched.
#[derive(Component)]
pub struct Patched;

/// Where the movers sit in the room, indexed as the look code counts them:
/// outcasts 0..2, hydros 2..4. Identity until the model loads.
#[derive(Resource, Default)]
pub struct Movers(pub [Transform; 4]);

#[derive(Resource)]
pub struct Orbit {
    yaw: f32,
    pitch: f32,
    dist: f32,
    focus: Vec3,
}

pub fn setup(mut cmds: Commands, assets: Res<AssetServer>, mut library: ResMut<GdtfLibrary>) -> Result {
    library.load_device::<ColoradoSolo>(format!("{FIXTURES}/colorado_solo.gdtf"), &assets)?;
    library.load_device::<HydroSpot>(format!("{FIXTURES}/hydro_spot.gdtf"), &assets)?;
    library.load_device::<OutcastBeamwash>(format!("{FIXTURES}/outcast_beamwash.gdtf"), &assets)?;

    cmds.spawn((
        Camera3d::default(),
        Camera { clear_color: ClearColorConfig::Custom(Color::BLACK), ..default() },
        Projection::from(PerspectiveProjection { fov: FOV.to_radians(), ..default() }),
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
    // Out front and stage right, eye level in the crowd.
    cmds.insert_resource(Orbit {
        yaw: -0.5,
        pitch: -0.05,
        dist: 7.5,
        focus: Vec3::new(0.0, 1.1, -0.3),
    });

    cmds.insert_resource(GlobalAmbientLight { color: Color::srgb(0.6, 0.7, 1.0), ..default() });
    cmds.spawn((
        WorkLight,
        DirectionalLight { shadow_maps_enabled: true, ..default() },
        Transform::from_xyz(4.0, 12.0, 8.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    GltfSceneBuilder::new().insert(Stage).spawn("Kosmic.glb", &mut cmds, &assets);
    Ok(())
}

/// Hang a fixture off every empty the model names.
pub fn patch(
    mut cmds: Commands,
    patch: Res<Patch>,
    scene: Query<(Entity, &GltfScene), (With<Stage>, Without<Patched>)>,
) {
    let Ok((entity, scene)) = scene.single() else {
        return;
    };

    let mut patched = 0;
    patched += patch_kind::<OutcastBeamwash>(&mut cmds, scene, &patch);
    patched += patch_kind::<HydroSpot>(&mut cmds, scene, &patch);
    patched += patch_kind::<ColoradoSolo>(&mut cmds, scene, &patch);

    info!("patched {patched} fixtures into {} nodes", scene.nodes().count());
    cmds.entity(entity).insert(Patched);
}

/// Hang one device off each of its nodes, and report how many landed.
fn patch_kind<D: GdtfDevice>(cmds: &mut Commands, scene: &GltfScene, patch: &Patch) -> usize {
    let mut patched = 0;
    for slot in patch.slots.iter().filter(|s| s.name.starts_with(D::KIND)) {
        let Some((_, node)) = scene.nodes().find(|(n, _)| *n == slot.name) else {
            warn!("{} is not in the model", slot.name);
            continue;
        };
        // Standing: the pars are aimed by hand, so their transform carries an
        // aim their unpatched tilt axle has to absorb.
        cmds.entity(node).insert((
            Name::new(slot.name.clone()),
            GdtfFixture::of::<D>(slot.address),
            Standing,
        ));
        patched += 1;
    }
    patched
}

/// Follow the patch when a scan or an assignment moves a fixture.
pub fn readdress(patch: Res<Patch>, mut fixtures: Query<(&Name, &mut GdtfFixture)>) {
    if !patch.is_changed() {
        return;
    }
    for (name, mut fixture) in &mut fixtures {
        let Some(slot) = patch.slot(name.as_str()) else { continue };
        if fixture.address != slot.address {
            fixture.address = slot.address;
        }
    }
}

/// House lights, so the room can be dimmed against the show.
#[derive(Component)]
pub struct WorkLight;

pub fn room_light(
    room: Res<Room>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut lights: Query<&mut DirectionalLight, With<WorkLight>>,
) {
    if !room.is_changed() {
        return;
    }
    // Ambient is a luminance, so an evenly lit room's illuminance over pi.
    ambient.brightness = room.0 / std::f32::consts::PI;
    for mut light in &mut lights {
        light.illuminance = room.0;
    }
}

/// Publish the movers' world transforms for the LookAt patterns.
pub fn movers(mut movers: ResMut<Movers>, fixtures: Query<(&Name, &GlobalTransform)>) {
    for (name, global) in &fixtures {
        let slot = match name.as_str() {
            "OutcastBeamwash.0" => 0,
            "OutcastBeamwash.1" => 1,
            "HydroSpot.0" => 2,
            "HydroSpot.1" => 3,
            _ => continue,
        };
        movers.0[slot] = global.compute_transform();
    }
}

pub fn orbit(
    mut orbit: ResMut<Orbit>,
    mut camera: Single<&mut Transform, With<Camera3d>>,
    mut egui_ctx: EguiContexts,
    buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
) -> Result {
    // Hover, not focus: wants_pointer_input() stays true after any widget drag
    // and would swallow the scroll wheel.
    let ctx = egui_ctx.ctx_mut()?;
    let free = !ctx.is_pointer_over_egui() && !ctx.egui_is_using_pointer();

    if free && buttons.pressed(MouseButton::Left) {
        orbit.yaw -= motion.delta.x * 0.005;
        orbit.pitch = (orbit.pitch - motion.delta.y * 0.005).clamp(-1.5, 1.5);
    }
    if free && scroll.delta.y != 0.0 {
        orbit.dist = (orbit.dist * (1.0 - scroll.delta.y.clamp(-3.0, 3.0) * 0.12)).clamp(1.0, 200.0);
    }

    let offset = Quat::from_rotation_y(orbit.yaw) * Quat::from_rotation_x(orbit.pitch) * Vec3::Z;
    camera.translation = orbit.focus + offset * orbit.dist;
    camera.look_at(orbit.focus, Vec3::Y);
    Ok(())
}
