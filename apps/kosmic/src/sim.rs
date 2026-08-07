//! The venue in 3d: the stage model, and a GDTF fixture at every mount point.
//!
//! `Kosmic.glb` carries an empty per fixture, named for the struct that drives
//! it and indexed to match the address tables in [`crate::lights`]. The empty's
//! transform is the GDTF root frame: its origin is where the fixture sits, and
//! -y is the beam at home. Fixtures read the same universe the ENTTEC does, so
//! what the sim shows is what the wire says.

use bevy::asset::RenderAssetUsages;
use bevy::camera::primitives::Aabb;
use bevy::camera::{Exposure, Hdr, PerspectiveProjection, Projection};
use bevy::core_pipeline::tonemapping::{DebandDither, Tonemapping};
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::light::NotShadowCaster;
use bevy::mesh::PrimitiveTopology;
use bevy::post_process::bloom::Bloom;
use bevy::render::render_resource::Face;
use lib::gdtf::{Axle, Emitter, GdtfDevice, Motor, Standing, declared_travel};
use lib::lights::fixture::{ColoradoSolo, HydroSpot, OutcastBeamwash};
use lib::prelude::*;

use crate::blackout::{Arc, Blackout, Zone, direction, unhomed};
use crate::dmx::{MOVERS, Patch};
use crate::home::{Home, Rest};

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
pub struct Movers {
    /// The fixture's frame: origin where it meets its mount, -y the rest aim.
    pub at: [Transform; 4],
    /// Where the head actually turns about, which is up the yoke from that.
    pub pivot: [Vec3; 4],
    /// How far the lens sits from that, in meters. Turning the head swings the
    /// lens round a circle this wide, so it is where the light starts.
    pub lens: [f32; 4],
    /// Where the head is really pointing, in the fixture's own frame, once the
    /// motors have got there. Built out of the fixture's own geometry, so it
    /// carries whatever rest turn the yoke was modelled with; none of it is
    /// known until the model is loaded.
    pub aim: [Option<Vec3>; 4],
}

#[derive(Resource)]
pub struct Orbit {
    yaw: f32,
    pitch: f32,
    dist: f32,
    focus: Vec3,
}

pub fn setup(
    mut cmds: Commands,
    assets: Res<AssetServer>,
    mut library: ResMut<GdtfLibrary>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) -> Result {
    library.load_device::<ColoradoSolo>(format!("{FIXTURES}/colorado_solo.gdtf"), &assets)?;
    library.load_device::<HydroSpot>(format!("{FIXTURES}/hydro_spot.gdtf"), &assets)?;
    library.load_device::<OutcastBeamwash>(format!("{FIXTURES}/outcast_beamwash.gdtf"), &assets)?;
    check_travel::<HydroSpot>(&library);
    check_travel::<OutcastBeamwash>(&library);

    cmds.spawn((
        Camera3d::default(),
        Camera { clear_color: ClearColorConfig::Custom(Color::BLACK), ..default() },
        Projection::from(PerspectiveProjection { fov: FOV.to_radians(), ..default() }),
        Hdr,
        // A blacked-out venue, not bevy's default daylight.
        Exposure { ev100: Exposure::EV100_INDOOR },
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

    // Lit and back faces only, so the volume reads as one solid: each face is
    // shaded by which way it points, and only the near side of it is drawn.
    // NotShadowCaster: it is a marker, not scenery, so it must not block the
    // beams it is drawn over.
    let mut shade = |color| {
        materials.add(StandardMaterial {
            base_color: color,
            alpha_mode: AlphaMode::Blend,
            perceptual_roughness: 1.0,
            cull_mode: Some(Face::Back),
            ..default()
        })
    };
    // One shade per window, so which of a mover's two a solid belongs to is
    // plain. The fade stands around its window and is drawn over it, so it is
    // lighter and thinner: what shows through it is the window itself.
    let shades = [
        [shade(Color::srgba(0.20, 0.21, 0.26, 0.55)), shade(Color::srgba(0.55, 0.58, 0.70, 0.14))],
        [shade(Color::srgba(0.28, 0.22, 0.19, 0.55)), shade(Color::srgba(0.72, 0.60, 0.50, 0.14))],
    ];
    // Rebuilt against the real window and rest aim on the first frame.
    let rest = Rest::default();
    for slot in 0..MOVERS.len() {
        for (window, pair) in shades.iter().enumerate() {
            for (fade, material) in [(false, &pair[0]), (true, &pair[1])] {
                cmds.spawn((
                    Name::new(match fade {
                        true => format!("Blackout.{slot}.{}.fade", window + 1),
                        false => format!("Blackout.{slot}.{}", window + 1),
                    }),
                    BlackoutVolume { slot, window, fade, lens: 0.0 },
                    Mesh3d(meshes.add(volume_mesh(&Zone::default(), &rest, 0.0, fade))),
                    MeshMaterial3d(material.clone()),
                    Visibility::Hidden,
                    NotShadowCaster,
                ));
            }
        }
    }

    // Front faces culled, so only the far side of the blown-up copy is drawn
    // and the part itself hides all of it but the rim. Over 1 so bloom takes
    // it, and biased away from the camera so it can't fight the part's own
    // surface where the two nearly meet.
    cmds.insert_resource(OutlineMaterial(materials.add(StandardMaterial {
        base_color: Color::LinearRgba(LinearRgba::rgb(0.4, 4.0, 6.0)),
        unlit: true,
        cull_mode: Some(Face::Front),
        depth_bias: -100.0,
        ..default()
    })));

    GltfSceneBuilder::new().insert(Stage).spawn("Kosmic.glb", &mut cmds, &assets);
    Ok(())
}

/// Complain if a fixture's declared travel is not the one its `.gdtf` says it
/// has. Everything that aims a head in angles works off the fixture's figure,
/// and a wrong one puts the beam somewhere other than where it was asked for
/// without anything looking broken.
fn check_travel<D: GdtfDevice>(library: &GdtfLibrary) {
    let Some(ty) = library.get(D::KIND) else { return };
    for (axle, declared, fitted) in
        ["pan", "tilt"].iter().zip(declared_travel(ty)).zip(D::TRAVEL).map(|((a, d), f)| (a, d, f))
    {
        let Some(declared) = declared else { continue };
        if (declared - fitted).abs() > 0.5 {
            warn!("{}: {axle} travels {declared}, not the {fitted} it declares", D::KIND);
        }
    }
}

/// The volume a mover's blackout window covers, drawn out of its head.
#[derive(Component)]
pub struct BlackoutVolume {
    slot: usize,
    /// Which of the mover's windows this one is drawing.
    window: usize,
    /// The fade around the window rather than the window itself.
    fade: bool,
    /// Lens distance the mesh was last built for, since the model is not loaded
    /// when the first one is.
    lens: f32,
}

/// Rays across each axis of the window, which is what rounds off the far cap.
const VOLUME_STEPS: usize = 8;
/// How far the drawn volume reaches, in meters.
const VOLUME_REACH: f32 = 8.0;

/// Sit each volume on its mover, and rebuild it whenever the window moves.
pub fn blackout_volumes(
    mut cmds: Commands,
    blackout: Res<Blackout>,
    home: Res<Home>,
    movers: Res<Movers>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut volumes: Query<(Entity, &mut BlackoutVolume, &mut Transform, &mut Visibility)>,
) {
    for (entity, mut volume, mut transform, mut visibility) in &mut volumes {
        let zone = blackout.movers[volume.slot][volume.window];
        let rest = home.movers[volume.slot];
        let lens = movers.lens[volume.slot];
        // Out of the head, not the base it stands on, and unscaled however the
        // model scales the fixture: the window is in meters off the pivot.
        transform.set_if_neq(Transform {
            translation: movers.pivot[volume.slot],
            rotation: movers.at[volume.slot].rotation,
            scale: Vec3::ONE,
        });
        visibility.set_if_neq(match zone.show && (!volume.fade || zone.show_fade) {
            true => Visibility::Inherited,
            false => Visibility::Hidden,
        });
        // Home as well as the window: the window is written round where the head
        // rests, so moving home carries it with it.
        if blackout.is_changed() || home.is_changed() || (volume.lens - lens).abs() > 1e-3 {
            volume.lens = lens;
            let mesh = volume_mesh(&zone, &rest, lens, volume.fade);
            cmds.entity(entity).insert(Mesh3d(meshes.add(mesh)));
        }
    }
}

/// The window as a solid: a cap across the directions it covers, another
/// `lens` meters out where the light leaves the head, and four walls between
/// them. `fade` draws what the fade reaches instead, which is the same solid
/// grown by the fade on both axes.
///
/// It starts at the lens rather than at a point on the pan axis because that is
/// where the beam does: turning the head swings the lens round a circle of that
/// radius, so nothing inside it is ever lit and a solid drawn to a point reads
/// as far tighter at the fixture than the window really is.
///
/// The head at rest points down the fixture's own -y, pitch swings it about
/// local x and the bearing carries that round local y, which is the frame the
/// window is written in.
///
/// Flat shaded, each face wound so its front is the one facing out of the
/// solid: nothing shares a vertex, so every face keeps its own normal and the
/// near side of the volume is the only side drawn.
fn volume_mesh(zone: &Zone, rest: &Rest, lens: f32, fade: bool) -> Mesh {
    let steps = VOLUME_STEPS;
    // The same convention the window is measured in, so the solid drawn and the
    // beam faded are the one thing.
    let aim = direction;

    // The arc that holds the level, or everything the window touches when it is
    // the fade being drawn. Pitch is not held to 0..180: a window taken past a
    // pole carries on down the far side, which is where it fades.
    let span = |arc: &Arc| match fade {
        true => arc.reach(),
        false => arc.held(),
    };
    let (up, round) = (zone.pitch, zone.yaw);
    let reach = span(&up).min(180.0);
    let (lo, hi) = (up.at - reach, up.at + reach);
    let half = span(&round).min(180.0);
    // The window's bearing is round from where the head rests; the mesh is drawn
    // in the fixture's own frame, so it is taken back to the pan travel.
    let facing = unhomed(rest, round.at);
    let (left, right) = (facing - half, facing + half);

    let point = |i: usize, j: usize, out: f32| {
        let tip = (i as f32 / steps as f32).lerp(lo..hi);
        let bearing = (j as f32 / steps as f32).lerp(left..right);
        aim(tip, bearing) * out
    };

    let (mut positions, mut normals) = (vec![], vec![]);
    // `out` is any direction on the outside of the face, which is what decides
    // which way round it is wound.
    let mut face = |a: Vec3, b: Vec3, c: Vec3, out: Vec3| {
        let (b, c) = match (b - a).cross(c - a).dot(out) < 0.0 {
            true => (c, b),
            false => (b, c),
        };
        positions.extend([a, b, c]);
        normals.extend([(b - a).cross(c - a).normalize_or(Vec3::Y); 3]);
    };

    // Both caps, the near one facing back at the fixture.
    for i in 0..steps {
        for j in 0..steps {
            for (out, side) in [(VOLUME_REACH, 1.0), (lens, -1.0)] {
                let (a, b, c, d) = (
                    point(i, j, out),
                    point(i + 1, j, out),
                    point(i + 1, j + 1, out),
                    point(i, j + 1, out),
                );
                face(a, b, c, (a + b + c) * side);
                face(a, c, d, (a + c + d) * side);
            }
        }
    }
    // And a wall along each edge of the window, from one cap to the other.
    for k in 0..steps {
        for ((i, j), (m, n), out) in [
            ((k, 0), (k + 1, 0), aim(up.at, left - 1.0)),
            ((k, steps), (k + 1, steps), aim(up.at, right + 1.0)),
            ((0, k), (0, k + 1), aim(lo - 1.0, facing)),
            ((steps, k), (steps, k + 1), aim(hi + 1.0, facing)),
        ] {
            let (a, b) = (point(i, j, lens), point(m, n, lens));
            let (c, d) = (point(i, j, VOLUME_REACH), point(m, n, VOLUME_REACH));
            face(a, b, d, out);
            face(a, d, c, out);
        }
    }

    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
}

///////////////////////// HIGHLIGHT /////////////////////////

/// The fixture the panel is pointing at, by the name its slot carries.
#[derive(Resource, Default)]
pub struct Highlight(pub Option<String>);

/// A copy of a fixture part, blown up around itself and drawn back faces only,
/// which leaves a rim of it showing around the part it stands behind.
#[derive(Component)]
pub struct Outline;

/// Shared by every part of the fixture being pointed at.
#[derive(Resource)]
pub struct OutlineMaterial(Handle<StandardMaterial>);

/// How thick the rim is, in meters.
const OUTLINE: f32 = 0.05;

/// Put the outline on whichever fixture the panel is pointing at.
pub fn outline(
    mut cmds: Commands,
    highlight: Res<Highlight>,
    material: Res<OutlineMaterial>,
    fixtures: Query<(Entity, &Name)>,
    children: Query<&Children>,
    parts: Query<(&Mesh3d, &Aabb), Without<Outline>>,
    outlines: Query<Entity, With<Outline>>,
) {
    if !highlight.is_changed() {
        return;
    }
    for entity in &outlines {
        cmds.entity(entity).despawn();
    }

    let Some(name) = &highlight.0 else { return };
    let Some((fixture, _)) = fixtures.iter().find(|(_, n)| n.as_str() == name) else {
        return;
    };
    for part in children.iter_descendants(fixture) {
        let Ok((mesh, aabb)) = parts.get(part) else { continue };
        // Blow the part up about its own middle, by enough to leave the same
        // rim whatever size it is.
        let center = Vec3::from(aabb.center);
        let scale = 1.0 + OUTLINE / Vec3::from(aabb.half_extents).length().max(1e-3);
        let copy = cmds
            .spawn((
                Outline,
                Mesh3d(mesh.0.clone()),
                MeshMaterial3d(material.0.clone()),
                Transform::from_translation(center * (1.0 - scale)).with_scale(Vec3::splat(scale)),
                NotShadowCaster,
            ))
            .id();
        cmds.entity(part).add_child(copy);
    }
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

/// Publish the movers' world transforms for the LookAt patterns, and where each
/// head hangs, which is what the blackout volume comes out of.
pub fn movers(
    mut movers: ResMut<Movers>,
    fixtures: Query<(Entity, &Name, &GlobalTransform)>,
    motors: Query<(&Motor, &GlobalTransform)>,
    emitters: Query<(&Emitter, &GlobalTransform)>,
) {
    for (entity, name, global) in &fixtures {
        let Some(slot) = MOVERS.iter().position(|m| *m == name.as_str()) else { continue };
        let at = global.compute_transform();
        // The tilt axle is the head's own centre of rotation; without one the
        // fixture's origin is the best that can be said.
        let head = motors
            .iter()
            .find(|(motor, _)| motor.fixture == entity && motor.kind == Axle::Tilt)
            .map(|(_, global)| global.translation());
        let pivot = head.unwrap_or(at.translation);
        // Beams only: a glow panel lights up without casting anything and can
        // sit anywhere on the body.
        let beams = || emitters.iter().filter(|(emitter, _)| emitter.fixture == entity && !emitter.glow);
        // The furthest of them, so the volume clears the whole front of the
        // head rather than starting inside it.
        movers.lens[slot] = beams().map(|(_, g)| g.translation().distance(pivot)).fold(0.0, f32::max);
        // Where they point, taken back out of the fixture's own frame. A cell
        // off the head's axis pulls its own way, so they are taken together.
        let aim: Vec3 = beams().map(|(_, g)| g.rotation() * Vec3::NEG_Y).sum();
        movers.aim[slot] = (at.rotation.inverse() * aim).try_normalize();
        movers.pivot[slot] = pivot;
        movers.at[slot] = at;
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
