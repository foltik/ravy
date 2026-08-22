//! The venue in 3d: `Jetlag.glb` built by `stage.py`, with a fixture at
//! every node the model names.
//!
//! Beams and big beams swap the blend's proxy bodies for `BeamFixture.glb` /
//! `BigBeamFixture.glb` with pan/tilt [`Motor`]s, spiders for
//! `SpiderFixture.glb` with a tilt [`Motor`] per bank, bars for
//! `BarFixture.glb`; the pars and the strobe keep the blend's hand-aimed
//! `ParFixture.glb` / `StrobeFixture.glb` copies and just have their lenses
//! driven. The `Screen` node's mesh takes the LED wall's render target.

use bevy::camera::primitives::Aabb;
use bevy::camera::{Exposure, Hdr, PerspectiveProjection, Projection};
use bevy::core_pipeline::tonemapping::{DebandDither, Tonemapping};
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::light::NotShadowCaster;
use bevy::post_process::bloom::Bloom;
use bevy::gltf::GltfMaterialName;
use bevy::render::render_resource::Face;
use lib::dmx::device::beam_rgbw_60w::BeamRing;
use lib::gdtf::Emitter;
use lib::prelude::*;
use lib::sim::motor::{Motor, MotorDynamics};

use crate::lights::Lights;
use crate::ledwall::{LedWall, Screen};

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
/// venue looks like from the crowd.
const FOV: f32 = 46.0;

#[derive(Component, Clone)]
pub struct Stage;

/// Set once the glb's fixture nodes have been patched.
#[derive(Component)]
pub struct Patched;

#[derive(Resource)]
pub struct Orbit {
    yaw: f32,
    pitch: f32,
    dist: f32,
    focus: Vec3,
}

///////////////////////// FIXTURES /////////////////////////

/// A par in the sim: the venue model's lens material and the volumetric
/// emitter behind the face. Aim and yoke angle live in the blend.
#[derive(Component)]
pub struct ParSim {
    pub index: usize,
    emit: Handle<StandardMaterial>,
    emitter: Entity,
}

/// A beam in the sim: pan and tilt motor nodes, the glowing lens and LED
/// ring, and the volumetric emitter behind the lens.
#[derive(Component)]
pub struct BeamSim {
    pub index: usize,
    yoke: Entity,
    head: Entity,
    emit: Handle<StandardMaterial>,
    ring: Handle<StandardMaterial>,
    emitter: Entity,
}

/// A beam node whose BeamFixture.glb is still loading.
#[derive(Component)]
pub struct BeamLoading {
    pub index: usize,
}

/// A big beam in the sim: pan and tilt motor nodes, the glowing dome lens,
/// and the volumetric emitter inside it.
#[derive(Component)]
pub struct BigBeamSim {
    pub index: usize,
    yoke: Entity,
    head: Entity,
    emit: Handle<StandardMaterial>,
    emitter: Entity,
}

/// A big beam node whose BigBeamFixture.glb is still loading.
#[derive(Component)]
pub struct BigBeamLoading {
    pub index: usize,
}

/// One bank of a spider: the tilting head node with its motor, the glowing
/// lens faces, and the volumetric emitter behind each lens.
struct SpiderBank {
    head: Entity,
    emit: Handle<StandardMaterial>,
    emitters: Vec<Entity>,
}

/// A spider in the sim: two banks that tilt against each other.
#[derive(Component)]
pub struct SpiderSim {
    pub index: usize,
    banks: [SpiderBank; 2],
}

/// A spider node whose SpiderFixture.glb is still loading.
#[derive(Component)]
pub struct SpiderLoading {
    pub index: usize,
}

/// A bar in the sim: the glowing LED window and the volumetric emitter over
/// it.
#[derive(Component)]
pub struct BarSim {
    pub index: usize,
    emit: Handle<StandardMaterial>,
    emitter: Entity,
}

/// A bar node whose BarFixture.glb is still loading.
#[derive(Component)]
pub struct BarLoading {
    pub index: usize,
}

/// The strobe in the sim: the venue model's window material and the
/// volumetric emitter over it, driven just like a bar.
#[derive(Component)]
pub struct StrobeSim {
    material: Handle<StandardMaterial>,
    emitter: Entity,
}

///////////////////////// SETUP /////////////////////////

pub fn setup(
    mut cmds: Commands,
    assets: Res<AssetServer>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
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

    cmds.spawn((
        Camera3d::default(),
        Camera { clear_color: ClearColorConfig::Custom(Color::BLACK), ..default() },
        PrimaryEguiContext,
        Projection::from(PerspectiveProjection {
            fov: FOV.to_radians(),
            // Close enough to inspect the LED wall's emitters.
            near: 0.01,
            ..default()
        }),
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
        dist: 11.0,
        focus: Vec3::new(0.0, 1.8, 0.0),
    });

    cmds.insert_resource(GlobalAmbientLight { color: Color::srgb(0.6, 0.7, 1.0), ..default() });
    cmds.spawn((
        WorkLight,
        DirectionalLight { shadow_maps_enabled: true, ..default() },
        Transform::from_xyz(4.0, 12.0, 8.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    GltfSceneBuilder::new().insert(Stage).spawn("Jetlag.glb", &mut cmds, &assets);
}

///////////////////////// PATCH /////////////////////////

/// Hang a fixture off every node the model names. The beams, big beams,
/// spiders and bars swap their blend proxies for rigs the sim can drive; the
/// pars and the strobe keep their hand-aimed venue models.
pub fn patch(
    mut cmds: Commands,
    screen: Res<Screen>,
    assets: Res<AssetServer>,
    scene: Query<(Entity, &GltfScene), (With<Stage>, Without<Patched>)>,
    children: Query<&Children>,
    names: Query<&Name>,
    meshed: Query<(), With<Mesh3d>>,
    gltf_materials: Query<&GltfMaterialName>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Ok((entity, scene)) = scene.single() else {
        return;
    };

    let mut patched = 0;
    for (name, node) in scene.nodes() {
        let clear = |cmds: &mut Commands| {
            if let Ok(kids) = children.get(node) {
                for kid in kids.iter() {
                    cmds.entity(kid).try_despawn();
                }
            }
        };
        // Proxy parts are named `Beam.0/Base`, so only a clean index is a
        // fixture node.
        match name.split_once('.') {
            Some(("Beam", i)) => {
                let Ok(index) = i.parse() else { continue };
                clear(&mut cmds);
                let handle = assets.load::<Gltf>("memory://fixtures/BeamFixture.glb");
                // XXX: don't construct this manually, same as moving_head
                let loader = GltfSceneLoader { handle, builder: Default::default() };
                cmds.entity(node).insert((BeamLoading { index }, loader));
            }
            Some(("BigBeam", i)) => {
                let Ok(index) = i.parse() else { continue };
                clear(&mut cmds);
                let handle = assets.load::<Gltf>("memory://fixtures/BigBeamFixture.glb");
                // XXX: don't construct this manually, same as moving_head
                let loader = GltfSceneLoader { handle, builder: Default::default() };
                cmds.entity(node).insert((BigBeamLoading { index }, loader));
            }
            // Pars keep their venue models like the strobe: aim and yoke
            // angle are set by hand in the blend. The lens material and the
            // volumetric emitter behind it are driven.
            Some(("Par", i)) => {
                let Ok(index) = i.parse() else { continue };
                let emit = materials.add(black_emissive());
                let mut emitter = Entity::PLACEHOLDER;
                for part in children.iter_descendants(node) {
                    // Lenses off the beam path only; see spider_setup.
                    if gltf_materials.get(part).is_ok_and(|name| name.0.starts_with("Emit")) {
                        cmds.entity(part).insert((MeshMaterial3d(emit.clone()), NotShadowCaster));
                    }
                    if names.get(part).is_ok_and(|name| name.starts_with("Emitter")) {
                        cmds.entity(part).insert(Emitter::manual(
                            PAR_BEAM,
                            PAR_FIELD,
                            PAR_FLUX,
                            PAR_APERTURE,
                        ));
                        emitter = part;
                    }
                }
                cmds.entity(node).insert(ParSim { index, emit, emitter });
            }
            Some(("Spider", i)) => {
                let Ok(index) = i.parse() else { continue };
                clear(&mut cmds);
                let handle = assets.load::<Gltf>("memory://fixtures/SpiderFixture.glb");
                // XXX: don't construct this manually, same as moving_head
                let loader = GltfSceneLoader { handle, builder: Default::default() };
                cmds.entity(node).insert((SpiderLoading { index }, loader));
            }
            Some(("Bar", i)) => {
                let Ok(index) = i.parse() else { continue };
                clear(&mut cmds);
                let handle = assets.load::<Gltf>("memory://fixtures/BarFixture.glb");
                // XXX: don't construct this manually, same as moving_head
                let loader = GltfSceneLoader { handle, builder: Default::default() };
                cmds.entity(node).insert((BarLoading { index }, loader));
            }
            // The strobe keeps its venue model: the yoke angle is set by hand
            // in the blend. The window material and the volumetric emitter
            // behind it are driven, same recipe as a bar.
            None if name == "Strobe" => {
                let material = materials.add(black_emissive());
                let mut emitter = Entity::PLACEHOLDER;
                for part in children.iter_descendants(node) {
                    if gltf_materials.get(part).is_ok_and(|name| name.0.starts_with("Emit")) {
                        cmds.entity(part)
                            .insert((MeshMaterial3d(material.clone()), NotShadowCaster));
                    }
                    if names.get(part).is_ok_and(|name| name.starts_with("Emitter")) {
                        cmds.entity(part).insert(Emitter::manual(
                            STROBE_BEAM,
                            STROBE_FIELD,
                            STROBE_FLUX,
                            STROBE_APERTURE,
                        ));
                        emitter = part;
                    }
                }
                cmds.entity(node).insert(StrobeSim { material, emitter });
            }
            // The wall's mesh takes the LED panel material over the render
            // target; the LedWall component holds its tweakables.
            None if name == "Screen" => {
                cmds.entity(node).insert(LedWall::default());
                for part in std::iter::once(node).chain(children.iter_descendants(node)) {
                    if meshed.get(part).is_ok() {
                        cmds.entity(part).insert(MeshMaterial3d(screen.wall.clone()));
                    }
                }
            }
            _ => continue,
        }
        patched += 1;
    }

    info!("patched {patched} fixtures into {} nodes", scene.nodes().count());
    cmds.entity(entity).insert(Patched);
}

fn black_emissive() -> StandardMaterial {
    StandardMaterial {
        base_color: Color::BLACK,
        emissive: Color::BLACK.into(),
        perceptual_roughness: 1.0,
        ..default()
    }
}

/// Estimated photometry for the par's 12x3W RGBW LEDs behind their
/// collimators: lumens out of the whole face at full, over a beam/field cone.
const PAR_FLUX: f32 = 600.0;
const PAR_BEAM: f32 = 25.0;
const PAR_FIELD: f32 = 45.0;
/// Lens plate radius in meters, shy of the rim tube around it.
const PAR_APERTURE: f32 = 0.05;

/// Estimated photometry for the beam's 60W RGBW LED behind its collimator:
/// lumens out of the lens at full, over a beam/field cone.
const BEAM_FLUX: f32 = 1_300.0;
const BEAM_BEAM: f32 = 5.0;
const BEAM_FIELD: f32 = 9.0;
/// Lens radius in meters.
const BEAM_APERTURE: f32 = 0.025;

/// Once a beam's model is in, hang pan/tilt motors, the driven lens and ring
/// materials and the volumetric emitter off it.
pub fn beam_setup(
    mut cmds: Commands,
    beams: Query<(Entity, &BeamLoading, &GltfScene)>,
    children: Query<&Children>,
    names: Query<&Name>,
    gltf_materials: Query<&GltfMaterialName>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (entity, loading, scene) in beams {
        let yoke = scene.node("Yoke");
        cmds.entity(yoke).insert(Motor::new(
            Axis::Y,
            MotorDynamics {
                θ_min: 0.0,
                θ_max: -540.0,
                v_max: 480.0,
                a_max: 1_200.0,
                j_max: 100_000.0,
                linear_threshold: 80.0,
                linear_gain: 1.5,
            },
        ));

        // ±95° from straight up about the yoke's arm axis.
        let head = scene.node("Head");
        cmds.entity(head).insert(Motor::new(
            Axis::X,
            MotorDynamics {
                θ_min: -95.0,
                θ_max: 95.0,
                v_max: 450.0,
                a_max: 3_000.0,
                j_max: 100_000.0,
                linear_threshold: 15.0,
                linear_gain: 5.0,
            },
        ));

        let emit = materials.add(black_emissive());
        let ring = materials.add(black_emissive());
        let mut emitter = Entity::PLACEHOLDER;
        for part in children.iter_descendants(head) {
            // Like the spider, only the lens plane stays out of the beam
            // pass's acceleration structure; the housing occludes.
            if let Ok(name) = gltf_materials.get(part) {
                match name.0.as_str() {
                    "Emit" => {
                        cmds.entity(part).insert((MeshMaterial3d(emit.clone()), NotShadowCaster));
                    }
                    "Ring" => {
                        cmds.entity(part).insert((MeshMaterial3d(ring.clone()), NotShadowCaster));
                    }
                    _ => {}
                }
            }
            if names.get(part).is_ok_and(|name| name.as_str() == "Emitter") {
                cmds.entity(part).insert(Emitter::manual(
                    BEAM_BEAM,
                    BEAM_FIELD,
                    BEAM_FLUX,
                    BEAM_APERTURE,
                ));
                emitter = part;
            }
        }
        cmds.entity(entity).remove::<BeamLoading>();
        cmds.entity(entity)
            .insert(BeamSim { index: loading.index, yoke, head, emit, ring, emitter });
    }
}

/// Estimated photometry for the big beam's 90W RGBW LED behind its dome
/// collimator: lumens out of the lens at full, over a beam/field cone.
const BIGBEAM_FLUX: f32 = 2_000.0;
const BIGBEAM_BEAM: f32 = 4.0;
const BIGBEAM_FIELD: f32 = 8.0;
/// Dome lens radius in meters.
const BIGBEAM_APERTURE: f32 = 0.0425;

/// Once a big beam's model is in, hang pan/tilt motors, the driven dome
/// material and the volumetric emitter off it.
pub fn bigbeam_setup(
    mut cmds: Commands,
    beams: Query<(Entity, &BigBeamLoading, &GltfScene)>,
    children: Query<&Children>,
    names: Query<&Name>,
    gltf_materials: Query<&GltfMaterialName>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (entity, loading, scene) in beams {
        let yoke = scene.node("Yoke");
        cmds.entity(yoke).insert(Motor::new(
            Axis::Y,
            MotorDynamics {
                θ_min: 0.0,
                θ_max: -540.0,
                v_max: 420.0,
                a_max: 1_000.0,
                j_max: 100_000.0,
                linear_threshold: 80.0,
                linear_gain: 1.5,
            },
        ));

        // ±95° from straight up about the yoke's arm axis.
        let head = scene.node("Head");
        cmds.entity(head).insert(Motor::new(
            Axis::X,
            MotorDynamics {
                θ_min: -95.0,
                θ_max: 95.0,
                v_max: 400.0,
                a_max: 2_500.0,
                j_max: 100_000.0,
                linear_threshold: 15.0,
                linear_gain: 5.0,
            },
        ));

        let emit = materials.add(black_emissive());
        let mut emitter = Entity::PLACEHOLDER;
        for part in children.iter_descendants(head) {
            // The dome off the beam path only; see spider_setup.
            if gltf_materials.get(part).is_ok_and(|name| name.0 == "Emit") {
                cmds.entity(part).insert((MeshMaterial3d(emit.clone()), NotShadowCaster));
            }
            if names.get(part).is_ok_and(|name| name.as_str() == "Emitter") {
                cmds.entity(part).insert(Emitter::manual(
                    BIGBEAM_BEAM,
                    BIGBEAM_FIELD,
                    BIGBEAM_FLUX,
                    BIGBEAM_APERTURE,
                ));
                emitter = part;
            }
        }
        cmds.entity(entity).remove::<BigBeamLoading>();
        cmds.entity(entity)
            .insert(BigBeamSim { index: loading.index, yoke, head, emit, emitter });
    }
}

/// Estimated photometry for the spider's 3W RGBW quad LEDs behind their
/// square collimators: lumens out of one lens at full, over a beam/field cone.
const SPIDER_FLUX: f32 = 70.0;
const SPIDER_BEAM: f32 = 6.0;
const SPIDER_FIELD: f32 = 11.0;
/// Lens radius in meters.
const SPIDER_APERTURE: f32 = 0.021;

/// The tilt channels as mslive calibrated them: where each bank points
/// straight up, which way it travels from there, and the total travel (Down's
/// 0.67 puts bank 0 flat). The banks counter-rotate, so equal positions splay
/// them apart until the top of the range crosses them inward at each other,
/// and mirrored positions sweep them together at a fixed splay.
const SPIDER_UP: [f32; 2] = [0.0, 0.52];
const SPIDER_DIR: [f32; 2] = [-1.0, 1.0];
const SPIDER_SWEEP: f32 = 135.0;

/// Once a spider's model is in, hang a tilt motor, driven lens material and
/// volumetric emitters off each bank.
pub fn spider_setup(
    mut cmds: Commands,
    spiders: Query<(Entity, &SpiderLoading, &GltfScene)>,
    children: Query<&Children>,
    names: Query<&Name>,
    gltf_materials: Query<&GltfMaterialName>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (entity, loading, scene) in spiders {
        let banks = [0, 1].map(|i| {
            let head = scene.node(&format!("Head.{i}"));
            cmds.entity(head).insert(Motor::new(
                Axis::X,
                MotorDynamics {
                    θ_min: SPIDER_DIR[i] * (0.0 - SPIDER_UP[i]) * SPIDER_SWEEP,
                    θ_max: SPIDER_DIR[i] * (1.0 - SPIDER_UP[i]) * SPIDER_SWEEP,
                    v_max: 400.0,
                    a_max: 2_500.0,
                    j_max: 100_000.0,
                    // No linear regime: the show sweeps these continuously,
                    // and its 60 deg/s cap limit-cycled against the S-curve.
                    linear_threshold: 0.0,
                    linear_gain: 0.0,
                },
            ));

            let emit = materials.add(black_emissive());
            let mut emitters = Vec::new();
            for part in children.iter_descendants(head) {
                // Only the lens faces stay out of the beam pass's acceleration
                // structure: they sit on the very point the occlusion rays aim
                // at. The rest of the housing is a solid that blocks beams,
                // its own included.
                if gltf_materials.get(part).is_ok_and(|name| name.0 == format!("Emit.{i}")) {
                    cmds.entity(part).insert((MeshMaterial3d(emit.clone()), NotShadowCaster));
                }
                if names.get(part).is_ok_and(|name| name.starts_with("Emitter.")) {
                    cmds.entity(part).insert(Emitter::manual(
                        SPIDER_BEAM,
                        SPIDER_FIELD,
                        SPIDER_FLUX,
                        SPIDER_APERTURE,
                    ));
                    emitters.push(part);
                }
            }
            SpiderBank { head, emit, emitters }
        });
        cmds.entity(entity).remove::<SpiderLoading>();
        cmds.entity(entity).insert(SpiderSim { index: loading.index, banks });
    }
}

/// Estimated photometry for the bar's LED array: lumens out of the whole
/// window at full, over one beam/field cone.
const BAR_FLUX: f32 = 500.0;
const BAR_BEAM: f32 = 20.0;
const BAR_FIELD: f32 = 40.0;
/// Aperture radius in meters: half the window length, so the column leaves
/// the fixture window-wide and just widens from there.
const BAR_APERTURE: f32 = 0.23;

/// The strobe reads like a bar: same photometry over its shorter window,
/// with the aperture kept off the floor.
const STROBE_FLUX: f32 = 500.0;
const STROBE_BEAM: f32 = 20.0;
const STROBE_FIELD: f32 = 40.0;
const STROBE_APERTURE: f32 = 0.03;

/// Once a bar's model is in, hang the driven window material and volumetric
/// emitter off it.
pub fn bar_setup(
    mut cmds: Commands,
    bars: Query<(Entity, &BarLoading, &GltfScene)>,
    children: Query<&Children>,
    names: Query<&Name>,
    gltf_materials: Query<&GltfMaterialName>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (entity, loading, _scene) in bars {
        let emit = materials.add(black_emissive());
        let mut emitter = Entity::PLACEHOLDER;
        for part in children.iter_descendants(entity) {
            // Window off the beam path only; see spider_setup.
            if gltf_materials.get(part).is_ok_and(|name| name.0 == "Emit") {
                cmds.entity(part).insert((MeshMaterial3d(emit.clone()), NotShadowCaster));
            }
            if names.get(part).is_ok_and(|name| name.as_str() == "Emitter") {
                cmds.entity(part).insert(Emitter::manual(
                    BAR_BEAM,
                    BAR_FIELD,
                    BAR_FLUX,
                    BAR_APERTURE,
                ));
                emitter = part;
            }
        }
        cmds.entity(entity).remove::<BarLoading>();
        cmds.entity(entity).insert(BarSim { index: loading.index, emit, emitter });
    }
}

///////////////////////// HIGHLIGHT /////////////////////////

/// The fixture the DMX panel is pointing at, by kind and world index.
#[derive(Resource, Default, PartialEq)]
pub struct Highlight(pub Option<(&'static str, usize)>);

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
    pars: Query<(Entity, &ParSim)>,
    beams: Query<(Entity, &BeamSim)>,
    bigbeams: Query<(Entity, &BigBeamSim)>,
    spiders: Query<(Entity, &SpiderSim)>,
    bars: Query<(Entity, &BarSim)>,
    strobes: Query<Entity, With<StrobeSim>>,
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

    let Some((kind, world)) = highlight.0 else { return };
    let fixture = match kind {
        "Par" => pars.iter().find(|(_, s)| s.index == world).map(|(e, _)| e),
        "Beam" => beams.iter().find(|(_, s)| s.index == world).map(|(e, _)| e),
        "BigBeam" => bigbeams.iter().find(|(_, s)| s.index == world).map(|(e, _)| e),
        "Spider" => spiders.iter().find(|(_, s)| s.index == world).map(|(e, _)| e),
        "Bar" => bars.iter().find(|(_, s)| s.index == world).map(|(e, _)| e),
        "Strobe" => strobes.iter().next(),
        _ => None,
    };
    let Some(fixture) = fixture else { return };
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

///////////////////////// SYNC /////////////////////////

/// How hard the emissive primitives glow per unit of colour.
const STRIP_EMIT: f32 = 40.0;

/// Copy the frame the show wrote onto the sim's fixtures.
pub fn sync(
    lights: Res<Lights>,
    pars: Query<&ParSim>,
    beams: Query<&BeamSim>,
    bigbeams: Query<&BigBeamSim>,
    spiders: Query<&SpiderSim>,
    bars: Query<&BarSim>,
    strobes: Query<&StrobeSim>,
    mut motors: Query<&mut Motor>,
    mut emitters: Query<&mut Emitter>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for beam in &beams {
        let state = &lights.beams[beam.index];
        let Rgb(r, g, b) = Rgb::from(state.color) * state.alpha;
        if let Some(mut material) = materials.get_mut(&beam.emit) {
            material.emissive = (Color::linear_rgb(r, g, b).to_linear() * STRIP_EMIT).into();
        }
        let Rgb(rr, rg, rb) = ring_rgb(state.ring);
        if let Some(mut material) = materials.get_mut(&beam.ring) {
            material.emissive = (Color::linear_rgb(rr, rg, rb).to_linear() * STRIP_EMIT).into();
        }
        if let Ok(mut emitter) = emitters.get_mut(beam.emitter) {
            emitter.set(Vec3::new(r, g, b));
        }
        if let Ok(mut motor) = motors.get_mut(beam.yoke) {
            motor.rotate(state.yaw);
        }
        if let Ok(mut motor) = motors.get_mut(beam.head) {
            motor.rotate(state.pitch);
        }
    }

    for beam in &bigbeams {
        let state = &lights.bigbeams[beam.index];
        let Rgb(r, g, b) = Rgb::from(state.color) * state.alpha;
        if let Some(mut material) = materials.get_mut(&beam.emit) {
            material.emissive = (Color::linear_rgb(r, g, b).to_linear() * STRIP_EMIT).into();
        }
        if let Ok(mut emitter) = emitters.get_mut(beam.emitter) {
            emitter.set(Vec3::new(r, g, b));
        }
        if let Ok(mut motor) = motors.get_mut(beam.yoke) {
            motor.rotate(state.yaw);
        }
        if let Ok(mut motor) = motors.get_mut(beam.head) {
            motor.rotate(state.pitch);
        }
    }

    for par in &pars {
        let Rgb(r, g, b) = Rgb::from(lights.pars[par.index].color);
        if let Some(mut material) = materials.get_mut(&par.emit) {
            material.emissive = (Color::linear_rgb(r, g, b).to_linear() * STRIP_EMIT).into();
        }
        if let Ok(mut emitter) = emitters.get_mut(par.emitter) {
            emitter.set(Vec3::new(r, g, b));
        }
    }

    for spider in &spiders {
        let state = &lights.spiders[spider.index];
        for (bank, (color, pos)) in
            spider.banks.iter().zip([(state.color0, state.pos0), (state.color1, state.pos1)])
        {
            let Rgb(r, g, b) = Rgb::from(color) * state.alpha;
            if let Some(mut material) = materials.get_mut(&bank.emit) {
                material.emissive = (Color::linear_rgb(r, g, b).to_linear() * STRIP_EMIT).into();
            }
            for &entity in &bank.emitters {
                if let Ok(mut emitter) = emitters.get_mut(entity) {
                    emitter.set(Vec3::new(r, g, b));
                }
            }
            if let Ok(mut motor) = motors.get_mut(bank.head) {
                motor.rotate(pos);
            }
        }
    }

    for bar in &bars {
        let state = &lights.bars[bar.index];
        let Rgb(r, g, b) = Rgb::from(state.color) * state.alpha;
        if let Some(mut material) = materials.get_mut(&bar.emit) {
            material.emissive = (Color::linear_rgb(r, g, b).to_linear() * STRIP_EMIT).into();
        }
        if let Ok(mut emitter) = emitters.get_mut(bar.emitter) {
            emitter.set(Vec3::new(r, g, b));
        }
    }

    for strobe in &strobes {
        let state = &lights.strobe;
        let Rgb(r, g, b) = state.color * state.alpha;
        if let Some(mut material) = materials.get_mut(&strobe.material) {
            material.emissive = (Color::linear_rgb(r, g, b).to_linear() * STRIP_EMIT).into();
        }
        if let Ok(mut emitter) = emitters.get_mut(strobe.emitter) {
            emitter.set(Vec3::new(r, g, b));
        }
    }
}

/// The lens ring's preset colours. The real ring runs its two-colour splits
/// as alternating segments; one material blends them to a single tone.
fn ring_rgb(ring: BeamRing) -> Rgb {
    match ring {
        BeamRing::Off => Rgb(0.0, 0.0, 0.0),
        BeamRing::Red => Rgb(1.0, 0.0, 0.0),
        BeamRing::Green => Rgb(0.0, 1.0, 0.0),
        BeamRing::Blue => Rgb(0.0, 0.0, 1.0),
        BeamRing::Yellow => Rgb(1.0, 1.0, 0.0),
        BeamRing::Purple => Rgb(1.0, 0.0, 1.0),
        BeamRing::Teal => Rgb(0.0, 1.0, 1.0),
        BeamRing::White | BeamRing::Cycle | BeamRing::Raw(_) => Rgb(1.0, 1.0, 1.0),
        BeamRing::RedYellow => Rgb(1.0, 0.5, 0.0),
        BeamRing::RedPurple => Rgb(1.0, 0.0, 0.5),
        BeamRing::RedWhite => Rgb(1.0, 0.5, 0.5),
        BeamRing::GreenYellow => Rgb(0.5, 1.0, 0.0),
        BeamRing::GreenBlue => Rgb(0.0, 0.5, 0.5),
        BeamRing::GreenWhite => Rgb(0.5, 1.0, 0.5),
        BeamRing::BluePurple => Rgb(0.5, 0.0, 1.0),
        BeamRing::BlueTeal => Rgb(0.0, 0.5, 1.0),
        BeamRing::BlueWhite => Rgb(0.5, 0.5, 1.0),
    }
}

///////////////////////// ROOM /////////////////////////

/// House lights, so the venue can be dimmed against the show.
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

///////////////////////// ORBIT /////////////////////////

pub fn orbit(
    mut orbit: ResMut<Orbit>,
    mut camera: Single<&mut Transform, (With<Camera3d>, With<Bloom>)>,
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
        orbit.dist =
            (orbit.dist * (1.0 - scroll.delta.y.clamp(-3.0, 3.0) * 0.12)).clamp(0.05, 200.0);
    }

    let offset = Quat::from_rotation_y(orbit.yaw) * Quat::from_rotation_x(orbit.pitch) * Vec3::Z;
    camera.translation = orbit.focus + offset * orbit.dist;
    camera.look_at(orbit.focus, Vec3::Y);
    Ok(())
}
