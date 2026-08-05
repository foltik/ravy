//! Loads a .gdtf, assembles the fixture in 3d, and drives it from DMX sliders.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use bevy::asset::io::memory::{Dir, MemoryAssetReader};
use bevy::asset::io::{AssetSource, AssetSourceId};
use bevy::core_pipeline::bloom::Bloom;
use bevy::gltf::GltfMesh;
use bevy::core_pipeline::tonemapping::{DebandDither, Tonemapping};
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::math::primitives::{ConicalFrustum, Cone, Cuboid, Cylinder, Plane3d};
use bevy::pbr::NotShadowCaster;
use bevy::render::mesh::MeshAabb;
use bevy::render::primitives::Aabb;
use bevy::scene::SceneInstanceReady;
use lib::prelude::*;

mod dmx;
mod gdtf;
use dmx::Dmx;
use gdtf::{Gdtf, Geometry, Kind};

/// GDTF is z-up with the beam along -z; bevy is y-up.
const TO_BEVY: Quat = Quat::from_xyzw(-std::f32::consts::FRAC_1_SQRT_2, 0.0, 0.0, std::f32::consts::FRAC_1_SQRT_2);

/// Length of the visible beam cone, in meters.
const BEAM_RANGE: f32 = 12.0;

/// Tool for previewing a GDTF fixture and its DMX channels
#[derive(argh::FromArgs)]
struct Args {
    /// path to a .gdtf file
    #[argh(positional)]
    path: PathBuf,
    /// serial port of the ENTTEC widget (default: auto-detect)
    #[argh(option)]
    port: Option<String>,
    /// enable debug logging
    #[argh(switch, short = 'v')]
    debug: bool,
    /// enable trace logging
    #[argh(switch, short = 'V')]
    trace: bool,
}

fn main() -> Result {
    let args: Args = argh::from_env();
    let gdtf = Gdtf::open(&args.path)?;

    let dir = Dir::default();
    for (name, bytes) in gdtf.meshes.iter().chain(&gdtf.images) {
        dir.insert_asset(Path::new(name), bytes.clone());
    }
    let reader = MemoryAssetReader { root: dir };

    App::new()
        .register_asset_source(
            AssetSourceId::from_static("gdtf"),
            AssetSource::build().with_reader(move || Box::new(reader.clone())),
        )
        .add_plugins(RavyPlugin { module: module_path!(), debug: args.debug, trace: args.trace })
        .insert_resource(Dmx::spawn(args.port.as_deref()))
        .insert_resource(Rig::new(gdtf))
        .add_systems(Startup, setup)
        .add_systems(Update, (rebuild, apply, orbit, room_light, dmx::poll, dmx::output))
        .add_systems(
            PostUpdate,
            fit.after(TransformSystem::TransformPropagate),
        )
        .add_systems(EguiPrimaryContextPass, draw_ui)
        .run();
    Ok(())
}

#[derive(Resource)]
struct Rig {
    gdtf: Gdtf,
    mode: usize,
    /// Raw DMX values, one per channel of the current mode.
    values: Vec<u32>,
    /// Multiplier on the GDTF's luminous flux. 1.0 is the real value.
    gain: f32,
    /// House light level in lux. 0 is a blacked-out room.
    room: f32,
    /// Mode the spawned fixture was built for.
    built: Option<usize>,
    models: HashMap<String, Handle<Gltf>>,
    /// Gobo images, keyed by asset name.
    images: HashMap<String, Handle<Image>>,
    /// Hang the fixture beam-down instead of standing it on its base.
    hang: bool,
    /// Pan/tilt home offset in degrees, correcting a mis-authored GDTF.
    home: [f32; 2],
    /// Pan/tilt reversal set in the fixture's menu, read over RDM when
    /// available. Separate from the flip that standing the fixture up implies.
    invert: [bool; 2],
    /// Cleared whenever the fixture is respawned or reoriented.
    fitted: bool,
}

impl Rig {
    fn new(gdtf: Gdtf) -> Self {
        let mut rig = Self {
            gdtf,
            mode: 0,
            values: vec![],
            gain: 1.0,
            room: 30.0,
            built: None,
            models: HashMap::new(),
            images: HashMap::new(),
            hang: false,
            home: [0.0, 0.0],
            invert: [false, false],
            fitted: false,
        };
        // Prefer the smallest mode: fewest channels to fiddle with.
        rig.mode = (0..rig.gdtf.modes.len())
            .min_by_key(|&i| rig.gdtf.modes[i].channels.len())
            .unwrap_or(0);
        rig.home();
        rig
    }

    fn home(&mut self) {
        self.values = self.gdtf.modes[self.mode].channels.iter().map(|c| c.default).collect();
    }

    /// Physical value of a resolved channel, or `fallback` if unpatched.
    fn physical(&self, channel: Option<usize>, fallback: f32) -> f32 {
        match channel {
            Some(i) => self.gdtf.modes[self.mode].channels[i].physical(self.values[i]),
            None => fallback,
        }
    }

    /// Wheel slot a channel currently selects.
    fn slot(&self, channel: usize) -> Option<&gdtf::Slot> {
        let (wheel, slot) = self.gdtf.modes[self.mode].channels[channel].slot(self.values[channel])?;
        self.gdtf.wheels.get(wheel)?.get(slot)
    }

    fn slot_index(&self, channel: usize) -> Option<usize> {
        Some(self.gdtf.modes[self.mode].channels[channel].slot(self.values[channel])?.1)
    }
}

/// CIE xyY to linear rgb, plus the slot's transmission (its Y, as a fraction).
fn cie_to_rgb(cie: Vec3) -> (Rgb, f32) {
    let Vec3 { x, y, z: luminance } = cie;
    if y <= 0.0 {
        return (Rgb::WHITE, 1.0);
    }
    // Normalized to Y = 1 so chromaticity and transmission stay independent.
    let (big_x, big_z) = (x / y, (1.0 - x - y) / y);
    let r = 3.2406 * big_x - 1.5372 - 0.4986 * big_z;
    let g = -0.9689 * big_x + 1.8758 + 0.0415 * big_z;
    let b = 0.0557 * big_x - 0.2040 + 1.0570 * big_z;

    let rgb = Vec3::new(r, g, b).max(Vec3::ZERO);
    let peak = rgb.max_element().max(1e-4);
    (Rgb::from(rgb / peak), (luminance / 100.0).clamp(0.0, 1.0))
}

/// A pan or tilt axis driven by a DMX channel.
#[derive(Component)]
struct Motor {
    channel: usize,
    kind: Axle,
    axis: Vec3,
    rest: Quat,
}

#[derive(Clone, Copy)]
enum Axle {
    Pan = 0,
    Tilt = 1,
}

#[derive(Component)]
struct Emitter {
    dimmer: Option<usize>,
    shutter: Option<usize>,
    zoom: Option<usize>,
    rgbw: [Option<usize>; 4],
    /// False for white sources that colour with wheels instead of mixing.
    additive: bool,
    /// Colour wheel channels, applied on top of the source colour.
    wheels: Vec<usize>,
    /// Gobo wheel channel.
    gobo: Option<usize>,
    /// Gobo image currently on the lens, to avoid restamping it every frame.
    gobo_slot: Option<usize>,
    /// Luminous flux in lumens, at full dimmer.
    flux: f32,
    /// Field angle to use when the mode has no zoom channel.
    angle: f32,
    /// BeamAngle / FieldAngle: where the cone falls off from hot to soft.
    hotspot: f32,
    light: Option<Entity>,
    lens: Handle<StandardMaterial>,
    beam: Option<(Handle<Mesh>, Handle<StandardMaterial>, f32)>,
}

#[derive(Component)]
struct Fixture;

/// Excluded from bounds: the cone is metres long and would swamp the fit.
#[derive(Component)]
struct BeamCone;

/// Material override applied to a part's meshes once its scene spawns.
#[derive(Component)]
struct Recolor(Handle<StandardMaterial>);

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
        Camera { hdr: true, clear_color: ClearColorConfig::Custom(Color::BLACK), ..default() },
        Bloom::NATURAL,
        Tonemapping::TonyMcMapface,
        DebandDither::Enabled,
    ));
    cmds.insert_resource(Orbit { yaw: 0.6, pitch: 0.25, dist: 2.0, focus: Vec3::Y });

    // Work light only: the fixture is what's meant to light the room. Ambient
    // alone would render the housing as a flat silhouette, hence the key.
    cmds.spawn((
        WorkLight,
        DirectionalLight { illuminance: 0.0, shadows_enabled: true, ..default() },
        Transform::from_xyz(4.0, 8.0, 6.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    cmds.spawn((
        WorkLight,
        DirectionalLight { illuminance: 0.0, color: Color::srgb(0.7, 0.8, 1.0), ..default() },
        Transform::from_xyz(-6.0, 2.0, -3.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    cmds.insert_resource(AmbientLight { color: Color::srgb(0.6, 0.7, 1.0), brightness: 0.0, ..default() });

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
    let material = mats.add(StandardMaterial {
        base_color: Color::srgb(0.9, 0.5, 0.1),
        unlit: true,
        ..default()
    });
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

fn room_light(rig: Res<Rig>, mut ambient: ResMut<AmbientLight>, mut lights: Query<&mut DirectionalLight, With<WorkLight>>) {
    if !rig.is_changed() {
        return;
    }
    ambient.brightness = rig.room * 0.05;
    for (i, mut light) in lights.iter_mut().enumerate() {
        light.illuminance = rig.room * if i == 0 { 1.0 } else { 0.3 };
    }
}

/// Size and orient a part mesh against its `<Model>` box.
///
/// Part meshes are raw-unit: real size comes from the box, applied per axis.
/// They also ship in either glTF's y-up or GDTF's own z-up (Chauvet exports
/// metres y-up, ADJ ships ~113-units-per-metre z-up). Whichever axis mapping
/// is right yields three near-equal scale factors, so the spread between them
/// picks it, which holds whatever units the mesh is in.
fn part_transform(measured: Vec3, size: Vec3) -> Transform {
    let measured = measured.max(Vec3::splat(1e-6));
    let Vec3 { x: length, y: width, z: height } = size;
    let zup = Vec3::new(length, width, height);
    let yup = Vec3::new(length, height, width);

    let spread = |target: Vec3| {
        let ratio = target / measured;
        let mean = (ratio.x + ratio.y + ratio.z) / 3.0;
        match mean > 0.0 {
            true => ((ratio - Vec3::splat(mean)) / mean).abs().element_sum(),
            false => f32::MAX,
        }
    };

    let zup_wins = spread(zup) < spread(yup);
    let target = match zup_wins {
        true => zup,
        false => yup,
    };
    Transform {
        rotation: match zup_wins {
            true => Quat::from_rotation_x(-TAU_4),
            false => Quat::IDENTITY,
        },
        // A zero-sized box means the model doesn't declare that axis.
        scale: Vec3::select(target.cmpgt(Vec3::ZERO), target / measured, Vec3::ONE),
        ..default()
    }
}

/// Raw extents of a glTF's meshes, ignoring node transforms.
fn mesh_extents(gltf: &Gltf, gltf_meshes: &Assets<GltfMesh>, meshes: &Assets<Mesh>) -> Option<Vec3> {
    let (mut min, mut max) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for handle in &gltf.meshes {
        for primitive in &gltf_meshes.get(handle)?.primitives {
            if let Some(aabb) = meshes.get(&primitive.mesh)?.compute_aabb() {
                min = min.min(Vec3::from(aabb.min()));
                max = max.max(Vec3::from(aabb.max()));
            }
        }
    }
    (min.x <= max.x).then(|| max - min)
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
        for corner in 0..8u32 {
            let sign = Vec3::new(
                if corner & 1 == 0 { -1.0 } else { 1.0 },
                if corner & 2 == 0 { -1.0 } else { 1.0 },
                if corner & 4 == 0 { -1.0 } else { 1.0 },
            );
            let local = Vec3::from(aabb.center) + sign * Vec3::from(aabb.half_extents);
            let point = inverse.transform_point3(child_global.transform_point(local));
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
    orbit.dist = size.length() * 1.6;

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

fn rebuild(
    mut cmds: Commands,
    mut rig: ResMut<Rig>,
    spawned: Query<Entity, With<Fixture>>,
    assets: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
    gltf_meshes: Res<Assets<GltfMesh>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    if rig.built == Some(rig.mode) {
        return;
    }

    if rig.models.is_empty() {
        rig.models = rig
            .gdtf
            .meshes
            .keys()
            .map(|name| (name.clone(), assets.load::<Gltf>(format!("gdtf://{name}"))))
            .collect();
    }
    if rig.images.is_empty() {
        rig.images = rig
            .gdtf
            .images
            .keys()
            .map(|name| (name.clone(), assets.load::<Image>(format!("gdtf://{name}"))))
            .collect();
    }
    if rig.models.values().any(|h| gltfs.get(h).is_none()) {
        return;
    }

    for entity in &spawned {
        cmds.entity(entity).despawn();
    }

    // GDTF authors a fixture hanging, base at the origin with everything below
    // it. Standing it up is a half turn about its own front-back axis, which
    // keeps the front facing forward and mirrors pan and tilt exactly as
    // inverting the real fixture's mount does.
    let upright = match rig.hang {
        true => Quat::IDENTITY,
        false => Quat::from_rotation_z(PI),
    };
    let root = cmds
        .spawn((Fixture, Transform::from_rotation(upright), Visibility::default()))
        .id();
    rig.fitted = false;

    let mut builder = Builder {
        cmds: &mut cmds,
        rig: &rig,
        gltfs: &gltfs,
        gltf_meshes: &gltf_meshes,
        meshes: &mut meshes,
        mats: &mut mats,
        parts: 0,
        emitters: 0,
        motors: 0,
    };
    for geometry in &rig.gdtf.geometries {
        // Beam geometries at the top level exist only as reference targets.
        if matches!(geometry.kind, Kind::Beam(_)) {
            continue;
        }
        let child = builder.geometry(geometry, &mut vec![]);
        builder.cmds.entity(root).add_child(child);
    }
    let (parts, emitters, motors) = (builder.parts, builder.emitters, builder.motors);

    let mode = &rig.gdtf.modes[rig.mode];
    info!(
        "{}: {} ({}ch) — {parts} meshes, {emitters} emitters, {motors} motors",
        rig.gdtf.name,
        mode.name,
        mode.footprint()
    );
    rig.built = Some(rig.mode);
}

struct Builder<'a, 'w, 's> {
    cmds: &'a mut Commands<'w, 's>,
    rig: &'a Rig,
    gltfs: &'a Assets<Gltf>,
    gltf_meshes: &'a Assets<GltfMesh>,
    meshes: &'a mut Assets<Mesh>,
    mats: &'a mut Assets<StandardMaterial>,
    parts: usize,
    emitters: usize,
    motors: usize,
}

impl Builder<'_, '_, '_> {
    fn geometry(&mut self, geometry: &Geometry, chain: &mut Vec<String>) -> Entity {
        let mode = &self.rig.gdtf.modes[self.rig.mode];
        chain.push(geometry.name.clone());

        let local = Transform::from_matrix(convert(geometry.transform));
        let entity = self
            .cmds
            .spawn((Name::new(geometry.name.clone()), local, Visibility::default()))
            .id();

        // A reference instances a top-level geometry at this node's position.
        let target = match &geometry.kind {
            Kind::Reference(name) => self.rig.gdtf.find(name),
            _ => None,
        };
        let (kind, model) = match target {
            Some(g) => (&g.kind, g.model.as_deref()),
            None => (&geometry.kind, geometry.model.as_deref()),
        };

        if matches!(kind, Kind::Axis) {
            // GDTF turns pan about the geometry's z and tilt about its x.
            for (attribute, kind, axis) in
                [("Pan", Axle::Pan, Vec3::Y), ("Tilt", Axle::Tilt, Vec3::X)]
            {
                if let Some(channel) = mode.channels.iter().position(|c| {
                    c.attribute == attribute && c.geometry == geometry.name
                }) {
                    self.cmds
                        .entity(entity)
                        .insert(Motor { channel, kind, axis, rest: local.rotation });
                    self.motors += 1;
                }
            }
        }

        let lens = match kind {
            Kind::Beam(beam) => {
                let lens = self.mats.add(StandardMaterial {
                    base_color: Color::BLACK,
                    emissive: LinearRgba::BLACK,
                    ..default()
                });
                self.emitter(entity, beam, model, chain, lens.clone());
                self.emitters += 1;
                Some(lens)
            }
            _ => None,
        };

        let model = model.and_then(|m| self.rig.gdtf.models.get(m));
        if let Some((model, mesh)) = model.and_then(|m| Some((m, m.mesh.as_ref()?))) {
            let gltf = self.gltfs.get(&self.rig.models[mesh]).unwrap();
            let scene = gltf.scenes[0].clone();
            let local = mesh_extents(gltf, self.gltf_meshes, self.meshes)
                .map(|measured| part_transform(measured, model.size))
                .unwrap_or_default();
            let part = match &lens {
                Some(lens) => {
                    let part = self.cmds.spawn((SceneRoot(scene), local, Recolor(lens.clone()))).id();
                    self.cmds.entity(part).observe(recolor);
                    part
                }
                None => self.cmds.spawn((SceneRoot(scene), local)).id(),
            };
            self.cmds.entity(entity).add_child(part);
            self.parts += 1;
        } else if let (Some(lens), Some(model)) = (&lens, model) {
            // Meshless beams still have a size: stand in with a disc.
            let disc = self.meshes.add(Cylinder::new(model.size.x / 2.0, model.size.z.max(0.001)));
            let part = self.cmds.spawn((Mesh3d(disc), MeshMaterial3d(lens.clone()))).id();
            self.cmds.entity(entity).add_child(part);
        }

        for child in &geometry.children {
            let child = self.geometry(child, chain);
            self.cmds.entity(entity).add_child(child);
        }

        chain.pop();
        entity
    }

    fn emitter(&mut self, entity: Entity, beam: &gdtf::Beam, model: Option<&str>, chain: &[String], lens: Handle<StandardMaterial>) {
        let mode = &self.rig.gdtf.modes[self.rig.mode];
        let gdtf = &self.rig.gdtf;
        let resolve = |attribute: &str| mode.resolve(gdtf, chain, attribute);

        // FieldAngle is the full cone; BeamAngle is just the hot core.
        let field = match beam.field > 0.0 {
            true => beam.field,
            false => beam.angle,
        };

        let (mut light, mut cone, mut beam_handles) = (None, None, None);
        if !beam.glow {
            light = Some(
                self.cmds
                    .spawn((
                        SpotLight {
                            color: Color::BLACK,
                            intensity: 0.0,
                            inner_angle: beam.angle.to_radians() / 2.0,
                            outer_angle: field.to_radians() / 2.0,
                            range: BEAM_RANGE,
                            radius: 0.0,
                            shadows_enabled: false,
                            ..default()
                        },
                        // Bevy spots point along -z, GDTF beams along -y once converted.
                        Transform::from_rotation(Quat::from_rotation_x(-TAU_4)),
                    ))
                    .id(),
            );

            // Prefer the lens size over BeamRadius, which is the far-field radius.
            let near = model
                .and_then(|m| gdtf.models.get(m))
                .map(|m| m.size.x / 2.0)
                .filter(|r| *r > 0.0)
                .unwrap_or(beam.radius);
            let mesh = self.meshes.add(cone_mesh(near, field));
            let material = self.mats.add(StandardMaterial {
                base_color: Color::NONE,
                unlit: true,
                alpha_mode: AlphaMode::Add,
                ..default()
            });
            cone = Some(
                self.cmds
                    .spawn((
                        BeamCone,
                        // Otherwise the work light throws a beam-shaped shadow.
                        NotShadowCaster,
                        Mesh3d(mesh.clone()),
                        MeshMaterial3d(material.clone()),
                        Transform::from_xyz(0.0, -BEAM_RANGE / 2.0, 0.0),
                    ))
                    .id(),
            );
            beam_handles = Some((mesh, material, near));
        }

        let rgbw = [
            resolve("ColorAdd_R"),
            resolve("ColorAdd_G"),
            resolve("ColorAdd_B"),
            resolve("ColorAdd_W"),
        ];
        self.cmds.entity(entity).insert(Emitter {
            dimmer: resolve("Dimmer"),
            shutter: resolve("Shutter1"),
            zoom: resolve("Zoom"),
            rgbw,
            additive: rgbw.iter().any(Option::is_some),
            wheels: ["Color1", "Color2"].iter().filter_map(|a| resolve(a)).collect(),
            gobo: resolve("Gobo1"),
            gobo_slot: None,
            flux: beam.flux,
            angle: field,
            hotspot: match field > 0.0 {
                true => (beam.angle / field).clamp(0.0, 1.0),
                false => 0.7,
            },
            light,
            lens,
            beam: beam_handles,
        });

        for child in light.iter().chain(cone.iter()) {
            self.cmds.entity(entity).add_child(*child);
        }
    }
}

fn recolor(
    trigger: Trigger<SceneInstanceReady>,
    mut cmds: Commands,
    children: Query<&Children>,
    parts: Query<&Recolor>,
    meshes: Query<(), With<Mesh3d>>,
) {
    let root = trigger.target();
    let Ok(Recolor(material)) = parts.get(root) else {
        return;
    };
    for child in children.iter_descendants(root) {
        if meshes.contains(child) {
            cmds.entity(child).insert(MeshMaterial3d(material.clone()));
        }
    }
}

fn apply(
    rig: Res<Rig>,
    mut motors: Query<(&Motor, &mut Transform)>,
    mut emitters: Query<&mut Emitter>,
    mut lights: Query<&mut SpotLight>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    // Emitters cache channel indices, so they're stale until rebuild catches up.
    if rig.built != Some(rig.mode) {
        return;
    }
    let mode = &rig.gdtf.modes[rig.mode];

    for (motor, mut transform) in &mut motors {
        let axle = motor.kind as usize;
        let mut degrees = mode.channels[motor.channel].physical(rig.values[motor.channel]);
        if rig.invert[axle] {
            degrees = -degrees;
        }
        let angle = (degrees + rig.home[axle]).to_radians();
        transform.rotation = motor.rest * Quat::from_axis_angle(motor.axis, angle);
    }

    for mut emitter in &mut emitters {
        let [r, g, b, w] = emitter.rgbw.map(|c| rig.physical(c, 0.0));
        let Rgb(mut r, mut g, mut b) = match emitter.additive {
            true => Rgb::from(Rgbw(r, g, b, w)),
            false => Rgb::WHITE,
        };

        // Colour wheels tint and dim: a slot's CIE Y is its transmission.
        let mut transmission = 1.0;
        for &channel in &emitter.wheels {
            let Some(slot) = rig.slot(channel) else { continue };
            let Some(cie) = slot.color else { continue };
            let (Rgb(sr, sg, sb), pass) = cie_to_rgb(cie);
            r *= sr;
            g *= sg;
            b *= sb;
            transmission *= pass;
        }

        let open = emitter.shutter.is_none_or(|c| {
            let channel = &mode.channels[c];
            let function = channel.function(rig.values[c]);
            // Strobe functions all keep the shutter nominally open; we don't blink.
            !function.attribute.ends_with("Shutter1") || channel.physical(rig.values[c]) >= 0.5
        });
        let level = rig.physical(emitter.dimmer, 1.0) * transmission * if open { 1.0 } else { 0.0 };
        let angle = rig.physical(emitter.zoom, emitter.angle);

        // Gobo: shows on the lens face. Projecting it down the beam would need
        // a spotlight cookie, which bevy has no support for.
        let gobo = emitter.gobo.and_then(|c| rig.slot(c)).and_then(|s| s.media.as_ref());
        let gobo_slot = emitter.gobo.and_then(|c| rig.slot_index(c));
        if emitter.gobo_slot != gobo_slot {
            emitter.gobo_slot = gobo_slot;
            let texture = gobo.and_then(|name| rig.images.get(name)).cloned();
            let lens = mats.get_mut(&emitter.lens).unwrap();
            lens.emissive_texture = texture.clone();
            lens.base_color_texture = texture;
        }

        let lens = mats.get_mut(&emitter.lens).unwrap();
        let e = 12.0 * level;
        lens.emissive = LinearRgba::rgb(r * e, g * e, b * e);

        if let Some(light) = emitter.light {
            let mut light = lights.get_mut(light).unwrap();
            light.color = Color::linear_rgb(r, g, b);
            // GDTF flux is lumens, which is what bevy wants.
            light.intensity = emitter.flux * rig.gain * level;
            light.outer_angle = (angle.to_radians() / 2.0).clamp(0.01, TAU_4 - 0.01);
            light.inner_angle = light.outer_angle * emitter.hotspot;
        }

        if let Some((mesh, material, near)) = emitter.beam.clone() {
            let material = mats.get_mut(&material).unwrap();
            material.base_color = Color::linear_rgba(r, g, b, 0.06 * level);

            let stale = (angle - emitter.angle).abs() > 0.01;
            if stale {
                *meshes.get_mut(&mesh).unwrap() = cone_mesh(near, angle);
                emitter.angle = angle;
            }
        }
    }
}

/// A cone spanning `BEAM_RANGE`, with its tip end (radius `near`) at +y.
fn cone_mesh(near: f32, angle: f32) -> Mesh {
    let spread = BEAM_RANGE * (angle.clamp(0.5, 175.0).to_radians() / 2.0).tan();
    Mesh::from(ConicalFrustum {
        height: BEAM_RANGE,
        radius_top: near,
        radius_bottom: near + spread,
    })
}

/// Convert a GDTF transform into bevy's coordinate system.
fn convert(m: Mat4) -> Mat4 {
    let c = Mat4::from_quat(TO_BEVY);
    c * m * c.inverse()
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
    let over_ui = egui_ctx.ctx_mut()?.is_pointer_over_area();

    if !over_ui {
        if buttons.pressed(MouseButton::Left) {
            orbit.yaw -= motion.delta.x * 0.005;
            orbit.pitch = (orbit.pitch - motion.delta.y * 0.005).clamp(-1.5, 1.5);
        }
        if scroll.delta.y != 0.0 {
            orbit.dist = (orbit.dist * (1.0 - scroll.delta.y.clamp(-3.0, 3.0) * 0.12)).clamp(0.05, 200.0);
        }
    }

    let offset = Quat::from_rotation_y(orbit.yaw) * Quat::from_rotation_x(orbit.pitch) * Vec3::Z;
    camera.translation = orbit.focus + offset * orbit.dist;
    camera.look_at(orbit.focus, Vec3::Y);
    Ok(())
}

fn draw_ui(mut egui_ctx: EguiContexts, mut rig: ResMut<Rig>, mut dmx: ResMut<Dmx>) -> Result {
    let ctx = egui_ctx.ctx_mut()?;
    egui::Window::new(format!("{} {}", rig.gdtf.manufacturer, rig.gdtf.name))
        .default_width(520.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                let order = modes_by_personality(&rig, dmx.target());
                let label = |rig: &Rig, i: usize| match personality(rig, dmx.target(), i) {
                    Some(n) => format!("{n}. {} ({}ch)", rig.gdtf.modes[i].name, rig.gdtf.modes[i].footprint()),
                    None => format!("-  {} ({}ch)", rig.gdtf.modes[i].name, rig.gdtf.modes[i].footprint()),
                };
                let selected = label(&rig, rig.mode);
                egui::ComboBox::from_label("Mode").selected_text(selected).show_ui(ui, |cb| {
                    let mut mode = rig.mode;
                    for i in order {
                        cb.selectable_value(&mut mode, i, label(&rig, i));
                    }
                    if mode != rig.mode {
                        rig.mode = mode;
                        rig.home();
                    }
                });
                if ui.button("Home").clicked() {
                    rig.home();
                }
                if ui.button("Blackout").clicked() {
                    rig.values.fill(0);
                }
                if ui.checkbox(&mut rig.hang, "Hang").changed() {
                    rig.built = None;
                }
            });

            ui.horizontal(|ui| {
                ui.add(egui::Slider::new(&mut rig.gain, 0.05..=20.0).logarithmic(true).text("Gain"));
                ui.add(egui::Slider::new(&mut rig.room, 0.0..=500.0).text("Room (lux)"));
            });

            ui.collapsing("Calibration", |ui| {
                ui.label("Model only, never sent on the wire.");
                egui::Grid::new("calibration").num_columns(3).show(ui, |ui| {
                    for (axle, name) in [(0, "Pan"), (1, "Tilt")] {
                        ui.label(name);
                        ui.add(egui::DragValue::new(&mut rig.home[axle]).speed(1.0).suffix("° home"));
                        ui.checkbox(&mut rig.invert[axle], "invert");
                        ui.end_row();
                    }
                });
            });

            ui.separator();
            output_ui(ui, &mut rig, &mut dmx);

            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::Grid::new("channels").num_columns(4).striped(true).show(ui, |ui| {
                    let Rig { gdtf, mode, values, .. } = &mut *rig;
                    for (i, channel) in gdtf.modes[*mode].channels.iter().enumerate() {
                        ui.label(channel.label());
                        // smart_aim snaps 16-bit drags to round thousands.
                        ui.add(egui::Slider::new(&mut values[i], 0..=channel.max()).smart_aim(false).show_value(false));
                        ui.add(egui::DragValue::new(&mut values[i]).range(0..=channel.max()));
                        let function = channel.function(values[i]);
                        ui.label(format!("{} = {:.2}", function.name, channel.physical(values[i])));
                        ui.end_row();
                    }
                });
            });
        });
    Ok(())
}

/// RDM personality number for a mode. The fixture's own list wins; a GDTF's
/// `<FTRDM>` map is often stale (the outcast's omits 37Ch entirely).
fn personality(rig: &Rig, target: Option<&dmx::Responder>, mode: usize) -> Option<u8> {
    let footprint = rig.gdtf.modes[mode].footprint() as u16;
    if let Some(target) = target
        && let Some((n, _, _)) = target.personalities.iter().find(|(_, _, f)| *f == footprint)
    {
        return Some(*n);
    }
    let rdm = rig.gdtf.rdm.as_ref()?;
    rdm.personalities.get(&rig.gdtf.modes[mode].name).copied()
}

fn modes_by_personality(rig: &Rig, target: Option<&dmx::Responder>) -> Vec<usize> {
    let mut order: Vec<usize> = (0..rig.gdtf.modes.len()).collect();
    order.sort_by_key(|&i| (personality(rig, target, i).unwrap_or(u8::MAX), rig.gdtf.modes[i].footprint()));
    order
}

fn output_ui(ui: &mut egui::Ui, rig: &mut Rig, dmx: &mut Dmx) {
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
        let label = format!("{}  {}  @{}  {}ch  pers {}", r.uid, r.model, r.address, r.footprint, r.personality);
        ui.radio_value(&mut selected, Some(i), label);
    }
    dmx.selected = selected;

    let Some(target) = dmx.target().cloned() else {
        return;
    };
    ui.checkbox(&mut dmx.enabled, format!("Send to {} @ {}", target.uid, target.address));

    // Discovery already switched to the fixture's personality; warn only if
    // the mode was then changed by hand.
    let footprint = rig.gdtf.modes[rig.mode].footprint();
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
