//! The room in 3d: a bare box, and every fixture where [`LAYOUT`] stands it.
//!
//! There is no model of the house, so the room and the rig are built here.
//! The big rig is GDTF, driven off the same universe the ENTTEC gets. The
//! classic rig swaps in the fixture models from `lib/assets/fixtures` with
//! motors and emitters the sim drives straight from [`Lights`].
//!
//! Frame: meters, x to the right as seen from the dance floor, y up, z toward
//! the dance floor, origin on the floor under the middle of the room. A
//! fixture's -y is its beam at home.

use std::f32::consts::{FRAC_PI_2, PI};

use bevy::asset::RenderAssetUsages;
use bevy::camera::primitives::Aabb;
use bevy::camera::{Exposure, Hdr, PerspectiveProjection, Projection};
use bevy::core_pipeline::tonemapping::{DebandDither, Tonemapping};
use bevy::gltf::GltfMaterialName;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::light::NotShadowCaster;
use bevy::mesh::PrimitiveTopology;
use bevy::post_process::bloom::Bloom;
use bevy::render::render_resource::Face;
use lib::dmx::device::beam_rgbw_60w::BeamRing;
use lib::gdtf::{Axle, Emitter, GdtfDevice, Motor as GdtfMotor, Standing, declared_travel};
use lib::lights::fixture::{ColoradoSolo, HydroSpot, OutcastBeamwash};
use lib::prelude::*;
use lib::sim::motor::{Motor, MotorDynamics};

use crate::blackout::{Arc, Blackout, Zone, direction, unhomed};
use crate::dmx::{MOVERS, Patch};
use crate::home::{Home, Rest};
use crate::lights::Lights;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/fixtures");

///////////////////////// LAYOUT /////////////////////////

/// Width, height and depth of the room.
const ROOM: Vec3 = Vec3::new(7.0, 2.7, 9.0);
/// The wall the rig is set against.
const BACK: f32 = -ROOM.z / 2.0;

/// How a fixture sits where it stands.
enum Mount {
    /// A moving head on its base, home beam straight up, facing the floor.
    Mover,
    /// A fixed head pointed at somewhere in the room.
    Aimed(Vec3),
    /// Lying flat, turned `yaw` radians round from facing the dance floor.
    Flat(f32),
    /// Facing the dance floor, tipped back `tilt` radians.
    Tilted(f32),
}

struct Spot {
    /// `<Kind>.<index>`, as the patch, the show and the panels name it.
    name: &'static str,
    at: Vec3,
    mount: Mount,
}

/// Where everything stands. The big rig lines the back wall, the classic
/// movers a step in front of it, the classic pars round the walls.
#[rustfmt::skip]
const LAYOUT: &[Spot] = &[
    Spot { name: "OutcastBeamwash.0", at: Vec3::new(-1.2, 1.2, BACK + 0.5), mount: Mount::Mover },
    Spot { name: "OutcastBeamwash.1", at: Vec3::new( 1.2, 1.2, BACK + 0.5), mount: Mount::Mover },
    Spot { name: "HydroSpot.0",       at: Vec3::new(-2.6, 0.0, BACK + 0.5), mount: Mount::Mover },
    Spot { name: "HydroSpot.1",       at: Vec3::new( 2.6, 0.0, BACK + 0.5), mount: Mount::Mover },
    Spot { name: "ColoradoSolo.0", at: Vec3::new(-3.0, 0.0, BACK + 0.3), mount: Mount::Aimed(Vec3::new(-2.4, 2.5, BACK)) },
    Spot { name: "ColoradoSolo.1", at: Vec3::new( 3.0, 0.0, BACK + 0.3), mount: Mount::Aimed(Vec3::new( 2.4, 2.5, BACK)) },
    Spot { name: "ColoradoSolo.2", at: Vec3::new(-1.8, 0.0, BACK + 0.3), mount: Mount::Aimed(Vec3::new(-1.4, 2.5, BACK)) },
    Spot { name: "ColoradoSolo.3", at: Vec3::new( 1.8, 0.0, BACK + 0.3), mount: Mount::Aimed(Vec3::new( 1.4, 2.5, BACK)) },
    Spot { name: "ColoradoSolo.4", at: Vec3::new(-0.6, 0.0, BACK + 0.3), mount: Mount::Aimed(Vec3::new(-0.5, 2.5, BACK)) },
    Spot { name: "ColoradoSolo.5", at: Vec3::new( 0.6, 0.0, BACK + 0.3), mount: Mount::Aimed(Vec3::new( 0.5, 2.5, BACK)) },

    // The line, in the order the show counts it from the left.
    Spot { name: "BigBeam.0", at: Vec3::new(-3.0, 0.0, BACK + 1.3), mount: Mount::Flat(0.0) },
    Spot { name: "Beam.0",    at: Vec3::new(-2.1, 0.0, BACK + 1.3), mount: Mount::Flat(0.0) },
    Spot { name: "Beam.1",    at: Vec3::new(-1.3, 0.0, BACK + 1.3), mount: Mount::Flat(0.0) },
    Spot { name: "BigBeam.1", at: Vec3::new(-0.4, 0.0, BACK + 1.3), mount: Mount::Flat(0.0) },
    Spot { name: "BigBeam.2", at: Vec3::new( 0.4, 0.0, BACK + 1.3), mount: Mount::Flat(0.0) },
    Spot { name: "Beam.2",    at: Vec3::new( 1.3, 0.0, BACK + 1.3), mount: Mount::Flat(0.0) },
    Spot { name: "Beam.3",    at: Vec3::new( 2.1, 0.0, BACK + 1.3), mount: Mount::Flat(0.0) },
    Spot { name: "BigBeam.3", at: Vec3::new( 3.0, 0.0, BACK + 1.3), mount: Mount::Flat(0.0) },

    // Pars up the side walls and into the corners.
    Spot { name: "Par.0", at: Vec3::new(-3.3, 0.0, BACK + 2.0), mount: Mount::Aimed(Vec3::new(-3.5, 2.6, BACK + 3.0)) },
    Spot { name: "Par.1", at: Vec3::new( 3.3, 0.0, BACK + 2.0), mount: Mount::Aimed(Vec3::new( 3.5, 2.6, BACK + 3.0)) },
    Spot { name: "Par.2", at: Vec3::new(-3.3, 0.0, 0.0),        mount: Mount::Aimed(Vec3::new(-3.5, 2.6, 1.0)) },
    Spot { name: "Par.3", at: Vec3::new( 3.3, 0.0, 0.0),        mount: Mount::Aimed(Vec3::new( 3.5, 2.6, 1.0)) },
    Spot { name: "Par.4", at: Vec3::new(-3.3, 0.0, 2.0),        mount: Mount::Aimed(Vec3::new(-3.5, 2.6, 3.0)) },
    Spot { name: "Par.5", at: Vec3::new( 3.3, 0.0, 2.0),        mount: Mount::Aimed(Vec3::new( 3.5, 2.6, 3.0)) },
    Spot { name: "Par.6", at: Vec3::new(-3.3, 0.0, 4.0),        mount: Mount::Aimed(Vec3::new(-2.5, 2.6, 4.4)) },
    Spot { name: "Par.7", at: Vec3::new( 3.3, 0.0, 4.0),        mount: Mount::Aimed(Vec3::new( 2.5, 2.6, 4.4)) },
    Spot { name: "Par.8", at: Vec3::new( 0.0, 0.0, 4.2),        mount: Mount::Aimed(Vec3::new( 0.0, 2.6, 4.4)) },

    Spot { name: "Spider.0", at: Vec3::new(-3.0, 0.0, BACK + 2.2), mount: Mount::Flat(0.0) },
    Spot { name: "Spider.1", at: Vec3::new( 3.0, 0.0, BACK + 2.2), mount: Mount::Flat(0.0) },
    Spot { name: "Bar.0",    at: Vec3::new(-2.0, 0.0, BACK + 0.15), mount: Mount::Flat(0.0) },
    Spot { name: "Bar.1",    at: Vec3::new( 2.0, 0.0, BACK + 0.15), mount: Mount::Flat(0.0) },
    Spot { name: "Strobe",   at: Vec3::new( 0.0, 0.0, BACK + 0.9), mount: Mount::Tilted(0.7) },
];

impl Spot {
    fn transform(&self) -> Transform {
        let rotation = match self.mount {
            // Turned over, so -y is straight up and +z still faces the floor.
            Mount::Mover => Quat::from_rotation_z(PI),
            Mount::Aimed(target) => {
                let dir = (target - self.at).normalize();
                // Something off the beam for the roll to be measured against.
                let up = match dir.y.abs() > 0.99 {
                    true => Vec3::Z,
                    false => Vec3::Y,
                };
                // looking_to puts -z down the beam; the quarter turn brings
                // -y round to it.
                Transform::IDENTITY.looking_to(dir, up).rotation * Quat::from_rotation_x(FRAC_PI_2)
            }
            Mount::Flat(yaw) => Quat::from_rotation_y(yaw),
            // The strobe's window faces -z at rest.
            Mount::Tilted(tilt) => Quat::from_rotation_y(PI) * Quat::from_rotation_x(tilt),
        };
        Transform { translation: self.at, rotation, scale: Vec3::ONE }
    }

    /// Kind and index: `Strobe` is the one of its kind.
    fn kind(&self) -> (&'static str, usize) {
        match self.name.split_once('.') {
            Some((kind, i)) => (kind, i.parse().expect("layout index")),
            None => (self.name, 0),
        }
    }
}

///////////////////////// RESOURCES /////////////////////////

/// House light level in lux. The show is what's meant to light the room, so
/// this only keeps the walls from going pitch black.
#[derive(Resource)]
pub struct Room(pub f32);

impl Default for Room {
    fn default() -> Self {
        Self(60.0)
    }
}

/// Vertical field of view. A 28mm lens on full frame, which is about what the
/// room looks like from the dance floor.
const FOV: f32 = 46.0;

/// Where the big movers sit in the room, indexed as the look code counts them:
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

///////////////////////// CLASSIC FIXTURES /////////////////////////

/// A classic beam in the sim: pan and tilt motor nodes, the glowing lens and
/// LED ring, and the volumetric emitter behind the lens.
#[derive(Component)]
pub struct BeamSim {
    pub index: usize,
    yoke: Entity,
    head: Entity,
    emit: Handle<StandardMaterial>,
    ring: Handle<StandardMaterial>,
    emitter: Entity,
}

/// A beam whose model is still loading.
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

#[derive(Component)]
pub struct SpiderLoading {
    pub index: usize,
}

/// A par, bar or strobe in the sim: no moving parts, just the glowing window
/// and the volumetric emitter over it.
#[derive(Component)]
pub struct LampSim {
    pub kind: &'static str,
    pub index: usize,
    emit: Handle<StandardMaterial>,
    emitter: Entity,
}

#[derive(Component)]
pub struct LampLoading {
    pub kind: &'static str,
    pub index: usize,
}

/// Estimated photometry, `(beam, field, flux, aperture)`: lumens out of the
/// whole face at full over a beam/field cone in degrees, from a lens of that
/// radius in meters.
type Photometry = (f32, f32, f32, f32);

/// The par's 12x3W RGBW LEDs behind their collimators; the plate shy of the
/// rim tube around it.
const PAR: Photometry = (25.0, 45.0, 600.0, 0.05);
/// The beam's 60W RGBW LED behind its collimator.
const BEAM: Photometry = (5.0, 9.0, 1_300.0, 0.025);
/// The big beam's 90W RGBW LED behind its dome collimator.
const BIGBEAM: Photometry = (4.0, 8.0, 2_000.0, 0.0425);
/// One of the spider's 3W RGBW quad LEDs behind its square collimator.
const SPIDER: Photometry = (6.0, 11.0, 70.0, 0.021);
/// The bar's LED array: the aperture is half the window length, so the column
/// leaves the fixture window-wide and just widens from there.
const BAR: Photometry = (20.0, 40.0, 500.0, 0.23);
/// The strobe reads like a bar over its shorter window, with the aperture
/// kept off the floor.
const STROBE: Photometry = (20.0, 40.0, 500.0, 0.03);

fn volumetric((beam, field, flux, aperture): Photometry) -> Emitter {
    Emitter::manual(beam, field, flux, aperture)
}

fn photometry(kind: &str) -> Photometry {
    match kind {
        "Par" => PAR,
        "Bar" => BAR,
        _ => STROBE,
    }
}

///////////////////////// SETUP /////////////////////////

pub fn setup(
    mut cmds: Commands,
    assets: Res<AssetServer>,
    patch: Res<Patch>,
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
        // A blacked-out room, not bevy's default daylight.
        Exposure { ev100: Exposure::EV100_INDOOR },
        Bloom::NATURAL,
        Tonemapping::TonyMcMapface,
        DebandDither::Enabled,
    ));
    // On the dance floor, eye level, looking at the rig.
    cmds.insert_resource(Orbit { yaw: -0.4, pitch: -0.05, dist: 7.0, focus: Vec3::new(0.0, 1.2, BACK + 1.5) });

    cmds.insert_resource(GlobalAmbientLight { color: Color::srgb(0.6, 0.7, 1.0), ..default() });
    cmds.spawn((
        WorkLight,
        DirectionalLight { shadow_maps_enabled: true, ..default() },
        Transform::from_xyz(4.0, 12.0, 8.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    room(&mut cmds, &mut meshes, &mut materials);

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

    let mut placed = 0;
    for spot in LAYOUT {
        let (kind, index) = spot.kind();
        let mut entity = cmds.spawn((Name::new(spot.name), spot.transform(), Visibility::default()));
        let classic = |model: &str| GltfSceneLoader {
            handle: assets.load::<Gltf>(format!("memory://fixtures/{model}Fixture.glb")),
            builder: default(),
        };
        match kind {
            OutcastBeamwash::KIND => _ = entity.insert(gdtf::<OutcastBeamwash>(&patch, spot.name)),
            HydroSpot::KIND => _ = entity.insert(gdtf::<HydroSpot>(&patch, spot.name)),
            // Standing: the pars are aimed by hand, so their transform carries
            // an aim their unpatched tilt axle has to absorb.
            ColoradoSolo::KIND => _ = entity.insert((gdtf::<ColoradoSolo>(&patch, spot.name), Standing)),
            "Beam" => _ = entity.insert((BeamLoading { index }, classic("Beam"))),
            "BigBeam" => _ = entity.insert((BigBeamLoading { index }, classic("BigBeam"))),
            "Spider" => _ = entity.insert((SpiderLoading { index }, classic("Spider"))),
            "Par" | "Bar" | "Strobe" => _ = entity.insert((LampLoading { kind, index }, classic(kind))),
            _ => {
                warn!("{} is not a fixture the sim can build", spot.name);
                continue;
            }
        }
        placed += 1;
    }
    info!("placed {placed} fixtures");
    Ok(())
}

/// A GDTF fixture at the address its patch slot holds.
fn gdtf<D: GdtfDevice>(patch: &Patch, name: &str) -> GdtfFixture {
    let address = patch.slot(name).map_or(0, |slot| slot.address);
    GdtfFixture::of::<D>(address)
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

/// The floor, ceiling and four walls, which is what the beams land on.
fn room(
    cmds: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let paint = materials.add(StandardMaterial {
        base_color: Color::srgb(0.55, 0.53, 0.50),
        perceptual_roughness: 0.95,
        ..default()
    });
    let floor = materials.add(StandardMaterial {
        base_color: Color::srgb(0.30, 0.24, 0.18),
        perceptual_roughness: 0.9,
        ..default()
    });
    let (w, h, d) = (ROOM.x, ROOM.y, ROOM.z);
    // A plane faces +y; each surface is turned to face into the room.
    let surfaces: [(Vec2, Transform, &Handle<StandardMaterial>); 6] = [
        (Vec2::new(w, d), Transform::IDENTITY, &floor),
        (Vec2::new(w, d), Transform::from_xyz(0.0, h, 0.0).with_rotation(Quat::from_rotation_x(PI)), &paint),
        (Vec2::new(w, h), Transform::from_xyz(0.0, h / 2.0, -d / 2.0).with_rotation(Quat::from_rotation_x(FRAC_PI_2)), &paint),
        (Vec2::new(w, h), Transform::from_xyz(0.0, h / 2.0, d / 2.0).with_rotation(Quat::from_rotation_x(-FRAC_PI_2)), &paint),
        (Vec2::new(h, d), Transform::from_xyz(-w / 2.0, h / 2.0, 0.0).with_rotation(Quat::from_rotation_z(-FRAC_PI_2)), &paint),
        (Vec2::new(h, d), Transform::from_xyz(w / 2.0, h / 2.0, 0.0).with_rotation(Quat::from_rotation_z(FRAC_PI_2)), &paint),
    ];
    for (size, transform, material) in surfaces {
        cmds.spawn((
            Name::new("Room"),
            Mesh3d(meshes.add(Plane3d::default().mesh().size(size.x, size.y))),
            MeshMaterial3d(material.clone()),
            transform,
        ));
    }
}

///////////////////////// BLACKOUT VOLUMES /////////////////////////

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

/// The fixture a panel is pointing at, by name.
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

///////////////////////// BIG RIG /////////////////////////

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

/// Publish the movers' world transforms for the LookAt patterns, and where each
/// head hangs, which is what the blackout volume comes out of.
pub fn movers(
    mut movers: ResMut<Movers>,
    fixtures: Query<(Entity, &Name, &GlobalTransform)>,
    motors: Query<(&GdtfMotor, &GlobalTransform)>,
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

///////////////////////// CLASSIC RIG /////////////////////////

fn black_emissive() -> StandardMaterial {
    StandardMaterial {
        base_color: Color::BLACK,
        emissive: Color::BLACK.into(),
        perceptual_roughness: 1.0,
        ..default()
    }
}

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
            // Only the lens plane stays out of the beam pass's acceleration
            // structure; the housing occludes.
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
                cmds.entity(part).insert(volumetric(BEAM));
                emitter = part;
            }
        }
        cmds.entity(entity).remove::<BeamLoading>();
        cmds.entity(entity).insert(BeamSim { index: loading.index, yoke, head, emit, ring, emitter });
    }
}

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
            if gltf_materials.get(part).is_ok_and(|name| name.0 == "Emit") {
                cmds.entity(part).insert((MeshMaterial3d(emit.clone()), NotShadowCaster));
            }
            if names.get(part).is_ok_and(|name| name.as_str() == "Emitter") {
                cmds.entity(part).insert(volumetric(BIGBEAM));
                emitter = part;
            }
        }
        cmds.entity(entity).remove::<BigBeamLoading>();
        cmds.entity(entity).insert(BigBeamSim { index: loading.index, yoke, head, emit, emitter });
    }
}

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
                    cmds.entity(part).insert(volumetric(SPIDER));
                    emitters.push(part);
                }
            }
            SpiderBank { head, emit, emitters }
        });
        cmds.entity(entity).remove::<SpiderLoading>();
        cmds.entity(entity).insert(SpiderSim { index: loading.index, banks });
    }
}

/// Once a par's, bar's or strobe's model is in, hang the driven window
/// material and volumetric emitter off it.
pub fn lamp_setup(
    mut cmds: Commands,
    lamps: Query<(Entity, &LampLoading, &GltfScene)>,
    children: Query<&Children>,
    names: Query<&Name>,
    gltf_materials: Query<&GltfMaterialName>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (entity, loading, _scene) in lamps {
        let emit = materials.add(black_emissive());
        let mut emitter = Entity::PLACEHOLDER;
        for part in children.iter_descendants(entity) {
            // Window off the beam path only; see spider_setup.
            if gltf_materials.get(part).is_ok_and(|name| name.0 == "Emit") {
                cmds.entity(part).insert((MeshMaterial3d(emit.clone()), NotShadowCaster));
            }
            if names.get(part).is_ok_and(|name| name.as_str() == "Emitter") {
                cmds.entity(part).insert(volumetric(photometry(loading.kind)));
                emitter = part;
            }
        }
        cmds.entity(entity).remove::<LampLoading>();
        cmds.entity(entity).insert(LampSim { kind: loading.kind, index: loading.index, emit, emitter });
    }
}

/// How hard the emissive primitives glow per unit of colour.
const STRIP_EMIT: f32 = 40.0;

/// Copy the frame the show wrote onto the classic fixtures.
pub fn sync(
    lights: Res<Lights>,
    beams: Query<&BeamSim>,
    bigbeams: Query<&BigBeamSim>,
    spiders: Query<&SpiderSim>,
    lamps: Query<&LampSim>,
    mut motors: Query<&mut Motor>,
    mut emitters: Query<&mut Emitter>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut glow = |material: &Handle<StandardMaterial>, Rgb(r, g, b): Rgb| {
        if let Some(mut material) = materials.get_mut(material) {
            material.emissive = (Color::linear_rgb(r, g, b).to_linear() * STRIP_EMIT).into();
        }
    };
    let mut cast = |entity: Entity, Rgb(r, g, b): Rgb| {
        if let Ok(mut emitter) = emitters.get_mut(entity) {
            emitter.set(Vec3::new(r, g, b));
        }
    };

    for beam in &beams {
        let state = &lights.beams[beam.index];
        let color = Rgb::from(state.color) * state.alpha;
        glow(&beam.emit, color);
        glow(&beam.ring, ring_rgb(state.ring));
        cast(beam.emitter, color);
        if let Ok(mut motor) = motors.get_mut(beam.yoke) {
            motor.rotate(state.yaw);
        }
        if let Ok(mut motor) = motors.get_mut(beam.head) {
            motor.rotate(state.pitch);
        }
    }

    for beam in &bigbeams {
        let state = &lights.bigbeams[beam.index];
        let color = Rgb::from(state.color) * state.alpha;
        glow(&beam.emit, color);
        cast(beam.emitter, color);
        if let Ok(mut motor) = motors.get_mut(beam.yoke) {
            motor.rotate(state.yaw);
        }
        if let Ok(mut motor) = motors.get_mut(beam.head) {
            motor.rotate(state.pitch);
        }
    }

    for spider in &spiders {
        let state = &lights.spiders[spider.index];
        for (bank, (color, pos)) in
            spider.banks.iter().zip([(state.color0, state.pos0), (state.color1, state.pos1)])
        {
            let color = Rgb::from(color) * state.alpha;
            glow(&bank.emit, color);
            for &entity in &bank.emitters {
                cast(entity, color);
            }
            if let Ok(mut motor) = motors.get_mut(bank.head) {
                motor.rotate(pos);
            }
        }
    }

    for lamp in &lamps {
        let color = match lamp.kind {
            "Par" => Rgb::from(lights.pars[lamp.index].color),
            "Bar" => Rgb::from(lights.bars[lamp.index].color) * lights.bars[lamp.index].alpha,
            _ => lights.strobe.color * lights.strobe.alpha,
        };
        glow(&lamp.emit, color);
        cast(lamp.emitter, color);
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
        orbit.dist = (orbit.dist * (1.0 - scroll.delta.y.clamp(-3.0, 3.0) * 0.12)).clamp(0.5, 200.0);
    }

    let offset = Quat::from_rotation_y(orbit.yaw) * Quat::from_rotation_x(orbit.pitch) * Vec3::Z;
    camera.translation = orbit.focus + offset * orbit.dist;
    camera.look_at(orbit.focus, Vec3::Y);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every fixture the show counts is somewhere in the room, once.
    #[test]
    fn the_layout_names_the_whole_rig() {
        let mut names: Vec<&str> = LAYOUT.iter().map(|s| s.name).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), LAYOUT.len(), "a fixture is placed twice");
        for slot in Patch::default().slots {
            assert!(names.contains(&slot.name.as_str()), "{} is not placed", slot.name);
        }
        for (kind, n) in [("Par", 9), ("Beam", 4), ("BigBeam", 4), ("Spider", 2), ("Bar", 2)] {
            for i in 0..n {
                let name = format!("{kind}.{i}");
                assert!(names.contains(&name.as_str()), "{name} is not placed");
            }
        }
        assert!(names.contains(&"Strobe"));
    }

    /// A mount's -y is its beam at home: straight up for a mover on its base,
    /// at the target for an aimed head.
    #[test]
    fn mounts_point_their_beams() {
        let beam = |spot: &Spot| spot.transform().rotation * Vec3::NEG_Y;
        let mover = Spot { name: "x", at: Vec3::ZERO, mount: Mount::Mover };
        assert!(beam(&mover).distance(Vec3::Y) < 1e-5);
        let aimed = Spot { name: "x", at: Vec3::ZERO, mount: Mount::Aimed(Vec3::new(1.0, 2.0, 2.0)) };
        assert!(beam(&aimed).distance(Vec3::new(1.0, 2.0, 2.0).normalize()) < 1e-5);
    }
}
