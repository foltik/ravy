//! Assembling GDTF fixtures in the world and driving them from DMX.
//!
//! A fixture type is parsed once into the [`GdtfLibrary`]; every instance is an
//! entity carrying [`GdtfFixture`], whose part meshes, motors and emitters are
//! spawned underneath it. Instances with a DMX address follow the [`Universe`]
//! the rig is being driven with, so the sim sees exactly what the wire does.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use bevy::asset::io::memory::Dir;
use bevy::camera::primitives::{Aabb, MeshAabb};
use bevy::gltf::{GltfMesh, GltfNode};
use bevy::light::NotShadowCaster;
use bevy::math::Affine3A;
use bevy::math::primitives::{ConicalFrustum, Cylinder};
use bevy::world_serialization::WorldInstanceReady;

use super::beam::{BeamMaterial, NoGobo};
use super::file::{self, Gdtf, Geometry, Kind};
use super::motion;
use super::photometry::{self, Led, Photometry, Profile};
use crate::prelude::*;

/// GDTF is z-up with the beam along -z; bevy is y-up.
pub const TO_BEVY: Quat =
    Quat::from_xyzw(-std::f32::consts::FRAC_1_SQRT_2, 0.0, 0.0, std::f32::consts::FRAC_1_SQRT_2);

/// Length of the visible beam cone, in meters.
pub const BEAM_RANGE: f32 = 12.0;

/// Range handed to the spot light, in meters. Bevy fades a light out over the
/// last of its range with a non-physical window, so this sits far enough past
/// the beam for the pool on the floor to stay inverse-square where it shows.
const LIGHT_RANGE: f32 = 4.0 * BEAM_RANGE;

/// Where the lens face's luminance starts compressing, in cd/m^2. Looking into
/// one of these really is millions of nits, and handing that to bloom smears it
/// over the whole frame. Compressing logarithmically above the knee keeps the
/// glow growing with the dimmer all the way down instead of pinning at the top.
const LENS_KNEE: f32 = 2.0e3;

const TAU_4: f32 = std::f32::consts::FRAC_PI_2;
const PI: f32 = std::f32::consts::PI;

/// The DMX frame the rig is being driven with. Whatever writes this drives the
/// hardware and the sim from the same channels.
#[derive(Resource)]
pub struct Universe(pub [u8; 512]);

impl Default for Universe {
    fn default() -> Self {
        Self([0; 512])
    }
}

/// A parsed fixture type and everything its instances share.
pub struct GdtfType {
    pub gdtf: Gdtf,
    pub photometry: Photometry,
    /// Mode the instances are patched in.
    pub mode: usize,
    /// Part meshes, keyed by asset name.
    pub models: HashMap<String, Handle<Gltf>>,
    /// Gobo images, keyed by asset name.
    pub images: HashMap<String, Handle<Image>>,
}

impl GdtfType {
    pub fn mode(&self) -> &file::Mode {
        &self.gdtf.modes[self.mode]
    }

    /// Whether every part mesh has finished loading.
    fn ready(&self, gltfs: &Assets<Gltf>) -> bool {
        self.models.values().all(|handle| gltfs.get(handle).is_some())
    }
}

/// Fixture types, and the asset source their part meshes are served from.
#[derive(Resource)]
pub struct GdtfLibrary {
    dir: Dir,
    types: HashMap<String, GdtfType>,
}

impl GdtfLibrary {
    pub(super) fn new(dir: Dir) -> Self {
        Self { dir, types: HashMap::new() }
    }

    /// Parse a `.gdtf` and make its meshes loadable. `key` names the type for
    /// [`GdtfFixture`] and namespaces its assets. `mode` is the DMX mode the
    /// fixtures are patched in, matched loosely ("STDY" finds "STD Y"); without
    /// one the smallest mode wins.
    pub fn load(
        &mut self,
        key: &str,
        path: impl AsRef<Path>,
        mode: Option<&str>,
        assets: &AssetServer,
    ) -> Result<(), anyhow::Error> {
        let gdtf = Gdtf::open(path.as_ref())?;

        let mode = match mode {
            Some(want) => gdtf
                .modes
                .iter()
                .position(|m| fold(&m.name) == fold(want))
                .ok_or_else(|| anyhow::anyhow!("{} has no mode {want}", gdtf.name))?,
            None => (0..gdtf.modes.len()).min_by_key(|&i| gdtf.modes[i].channels.len()).unwrap_or(0),
        };

        fn load<A: Asset>(
            dir: &Dir,
            key: &str,
            files: &HashMap<String, Vec<u8>>,
            assets: &AssetServer,
        ) -> HashMap<String, Handle<A>> {
            files
                .iter()
                .map(|(name, bytes)| {
                    dir.insert_asset(Path::new(&format!("{key}/{name}")), bytes.clone());
                    (name.clone(), assets.load(format!("gdtf://{key}/{name}")))
                })
                .collect()
        }
        let models = load::<Gltf>(&self.dir, key, &gdtf.meshes, assets);
        let images = load::<Image>(&self.dir, key, &gdtf.images, assets);

        info!("{key}: {} — {} ({}ch)", gdtf.name, gdtf.modes[mode].name, gdtf.modes[mode].footprint());
        self.types
            .insert(key.to_string(), GdtfType { photometry: Photometry::new(&gdtf), gdtf, mode, models, images });
        Ok(())
    }

    /// Parse the `.gdtf` behind a known device, in the mode it is set to.
    pub fn load_device<D: GdtfDevice>(
        &mut self,
        path: impl AsRef<Path>,
        assets: &AssetServer,
    ) -> Result<(), anyhow::Error> {
        self.load(D::KIND, path, Some(D::MODE), assets)
    }

    pub fn get(&self, key: &str) -> Option<&GdtfType> {
        self.types.get(key)
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut GdtfType> {
        self.types.get_mut(key)
    }
}

/// Fold a mode name for comparison: fixtures and GDTFs disagree on spacing and
/// case ("STDY" vs "STD Y").
fn fold(name: &str) -> String {
    name.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_lowercase()).collect()
}

/// One fixture in the world. Its transform is the GDTF root frame: the origin
/// sits where the fixture meets what it is mounted to, and -y is the beam at
/// its home position.
#[derive(Component)]
pub struct GdtfFixture {
    /// Type key in the [`GdtfLibrary`].
    pub kind: String,
    /// 1-based DMX start address, or 0 to drive the values directly.
    pub address: usize,
    /// Raw DMX values, one per channel of the type's mode.
    pub values: Vec<u32>,
    /// Pan/tilt home offset in degrees, correcting a mis-authored GDTF.
    pub home: [f32; 2],
    /// Pan/tilt reversal set in the fixture's menu.
    pub invert: [bool; 2],
    /// How each mechanism travels, indexed by [`Axle`].
    pub motion: [motion::Params; 3],
    /// Mode the parts under this fixture were built for.
    built: Option<usize>,
}

/// A device driven over DMX with a GDTF standing in for it in the sim.
pub trait GdtfDevice: DmxDevice {
    /// Type key in the [`GdtfLibrary`].
    const KIND: &'static str;
    /// DMX mode in the `.gdtf`, which is the personality the fixture is set to.
    const MODE: &'static str;

    /// Pan, tilt and zoom travel.
    fn motion() -> [motion::Params; 3] {
        [motion::Params::pan(), motion::Params::tilt(), motion::Params::zoom()]
    }
}

impl GdtfFixture {
    pub fn of<D: GdtfDevice>(address: usize) -> Self {
        Self { motion: D::motion(), ..Self::new(D::KIND, address) }
    }

    pub fn new(kind: impl Into<String>, address: usize) -> Self {
        Self {
            kind: kind.into(),
            address,
            values: vec![],
            home: [0.0, 0.0],
            invert: [false, false],
            motion: [motion::Params::pan(), motion::Params::tilt(), motion::Params::zoom()],
            built: None,
        }
    }

    pub fn is_built(&self) -> bool {
        self.built.is_some()
    }

    /// Drop the assembled parts, so the next build puts them back.
    pub fn rebuild(&mut self) {
        self.built = None;
    }

    /// Reset every channel to the value the fixture powers up at.
    pub fn home(&mut self, ty: &GdtfType) {
        self.values = ty.mode().channels.iter().map(|c| c.default).collect();
    }

    /// Level of a channel as a fraction of its own declared range. GDTF authors
    /// disagree on whether a dimmer runs 0-1 or 0-100 (the hydro says percent,
    /// both chauvets say unit), so the range is the only thing that reliably
    /// says where "full" is.
    fn level(&self, ty: &GdtfType, channel: Option<usize>, fallback: f32) -> f32 {
        match channel {
            Some(i) => ty.mode().channels[i].fraction(self.values[i]),
            None => fallback,
        }
    }

    fn slot<'a>(&self, ty: &'a GdtfType, channel: usize) -> Option<&'a file::Slot> {
        let (wheel, slot) = ty.mode().channels[channel].slot(self.values[channel])?;
        ty.gdtf.wheels.get(wheel)?.get(slot)
    }

    fn slot_index(&self, ty: &GdtfType, channel: usize) -> Option<usize> {
        Some(ty.mode().channels[channel].slot(self.values[channel])?.1)
    }
}

/// A moving mechanism driven by a DMX channel: a pan or tilt axle, or the
/// zoom carriage.
#[derive(Component)]
pub struct Motor {
    pub fixture: Entity,
    pub channel: usize,
    pub kind: Axle,
    axis: Vec3,
    rest: Quat,
    /// The wire and input smoothing between the slider and the profiler.
    pub command: motion::Command,
    /// Where the head actually is, which trails the DMX.
    pub travel: motion::Axis,
    /// Recent travel, for the realtime plot.
    pub trace: motion::Trace,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Axle {
    Pan = 0,
    Tilt = 1,
    /// Travels in percent of its range, not degrees, and moves no geometry.
    Zoom = 2,
}

#[derive(Component)]
pub struct Emitter {
    pub fixture: Entity,
    dimmer: Option<usize>,
    shutter: Option<usize>,
    zoom: Option<usize>,
    rgbw: [Option<usize>; 4],
    /// The LED behind each mixing channel.
    leds: [Led; 4],
    /// False for white sources that colour with wheels instead of mixing.
    additive: bool,
    /// Source colour of a fixture without mixing, at a luminance of 1.
    source: Vec3,
    /// Colour wheel channels, applied on top of the source colour.
    wheels: Vec<usize>,
    gobo: Option<usize>,
    /// Gobo indexing channel, which rotates the pattern.
    gobo_pos: Option<usize>,
    /// Accumulated rotation in degrees, for the continuous-spin function.
    spin: f32,
    /// Gobo slot currently applied, to avoid restamping it every frame.
    gobo_slot: Option<usize>,
    /// Luminous flux in lumens the GDTF claims, used where nothing measured
    /// this fixture.
    flux: f32,
    /// Field angle to use when the mode has no zoom channel.
    angle: f32,
    /// BeamAngle / FieldAngle: where the cone falls off from hot to soft.
    hotspot: f32,
    /// A surface that lights up but casts no beam.
    pub glow: bool,
    /// Fraction of the fixture's measured output this cell makes. The lab lit
    /// every cell of an array at once, so one of seven throws a seventh.
    share: f32,
    /// Area of the emitting face in square metres, which sets its luminance.
    area: f32,
    /// Current beam profile, shared with the volume shader.
    cone: Profile,
    /// What the emitter is putting out right now, in candela.
    pub peak: f32,
    /// Beam and field angle it is putting it out over, in degrees.
    pub angles: Vec2,
    /// Full outer angle the cone mesh was last built for, in degrees.
    built: f32,
    light: Option<Entity>,
    lens: Handle<StandardMaterial>,
    beam: Option<(Handle<Mesh>, Handle<BeamMaterial>, f32)>,
    /// The cone volume, hidden while the emitter is dark.
    volume: Option<Entity>,
}

/// Excluded from bounds: the cone is metres long and would swamp a fit.
#[derive(Component)]
pub struct BeamCone;

/// Material override applied to a part's meshes once its scene spawns.
#[derive(Component)]
struct Recolor(Handle<StandardMaterial>);

/// Matte black powder coat, which is what these fixtures are made of. It
/// reflects about 4% of what hits it, which reads as a dark grey; sRGB 0.04
/// would be a hundredth of that and leave the fixture invisible.
#[derive(Resource)]
pub(super) struct BodyMaterial(Handle<StandardMaterial>);

pub(super) fn setup(mut cmds: Commands, mut mats: ResMut<Assets<StandardMaterial>>) {
    let body = mats.add(StandardMaterial {
        base_color: Color::srgb(0.235, 0.235, 0.24),
        perceptual_roughness: 0.8,
        ..default()
    });
    cmds.insert_resource(BodyMaterial(body));
}

/// Copy each patched fixture's slice of the universe into its channel values.
pub(super) fn drive(
    universe: Option<Res<Universe>>,
    library: Res<GdtfLibrary>,
    mut fixtures: Query<&mut GdtfFixture>,
) {
    let Some(universe) = universe else { return };
    for mut fixture in &mut fixtures {
        if fixture.address == 0 || !fixture.is_built() {
            continue;
        }
        let Some(ty) = library.get(&fixture.kind) else { continue };
        let base = fixture.address - 1;
        let values: Vec<u32> = ty
            .mode()
            .channels
            .iter()
            .map(|channel| {
                channel.offsets.iter().fold(0u32, |acc, &slot| {
                    (acc << 8) | universe.0.get(base + slot - 1).copied().unwrap_or(0) as u32
                })
            })
            .collect();
        fixture.values = values;
    }
}

pub(super) fn build(
    mut cmds: Commands,
    library: Res<GdtfLibrary>,
    mut fixtures: Query<(Entity, &mut GdtfFixture, &GlobalTransform, Has<Standing>)>,
    children: Query<&Children>,
    gltfs: Res<Assets<Gltf>>,
    gltf_meshes: Res<Assets<GltfMesh>>,
    nodes: Res<Assets<GltfNode>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut beams: ResMut<Assets<BeamMaterial>>,
    blank: Res<NoGobo>,
    body: Res<BodyMaterial>,
) {
    for (entity, mut fixture, global, standing) in &mut fixtures {
        let Some(ty) = library.get(&fixture.kind) else { continue };
        if fixture.built == Some(ty.mode) || !ty.ready(&gltfs) {
            continue;
        }
        for child in children.iter_descendants(entity) {
            cmds.entity(child).try_despawn();
        }
        fixture.home(ty);
        fixture.built = Some(ty.mode);

        // A pixel array is measured as one fixture, so its cells split the total.
        let cells: usize = ty
            .gdtf
            .geometries
            .iter()
            .filter(|geometry| !matches!(geometry.kind, Kind::Beam(_)))
            .map(|geometry| projectors(&ty.gdtf, geometry))
            .sum();

        let mut builder = Builder {
            cmds: &mut cmds,
            fixture: entity,
            ty,
            gltfs: &gltfs,
            gltf_meshes: &gltf_meshes,
            nodes: &nodes,
            meshes: &mut meshes,
            mats: &mut mats,
            beams: &mut beams,
            blank: blank.0.clone(),
            body: body.0.clone(),
            share: 1.0 / cells.max(1) as f32,
            casts_shadows: true,
            manual: standing.then(|| manual_tilt(global, ty)).flatten(),
        };
        for geometry in &ty.gdtf.geometries {
            // Beam geometries at the top level exist only as reference targets.
            if matches!(geometry.kind, Kind::Beam(_)) {
                continue;
            }
            let child = builder.geometry(geometry, &mut vec![]);
            builder.cmds.entity(entity).add_child(child);
        }

        // Zoom is a carriage with its own travel; the emitters chase where it
        // is rather than what the wire says.
        if let Some(channel) = ty.mode().channels.iter().position(|c| c.attribute == "Zoom") {
            let motor = cmds
                .spawn((
                    Name::new("Zoom"),
                    Transform::default(),
                    Visibility::default(),
                    Motor {
                        fixture: entity,
                        channel,
                        kind: Axle::Zoom,
                        axis: Vec3::Y,
                        rest: Quat::IDENTITY,
                        command: default(),
                        travel: default(),
                        trace: default(),
                    },
                ))
                .id();
            cmds.entity(entity).add_child(motor);
        }
    }
}

/// A fixture resting on the surface under it, whose yoke is aimed by hand.
/// Without this it is placed exactly as its transform says, which is what a
/// hung fixture wants.
#[derive(Component)]
pub struct Standing;

/// How a fixture with a hand-set yoke sits: the base flat on its surface, and
/// the axle cranked round to where the fixture's transform is pointing.
///
/// A par is aimed by hand, so its tilt axle carries no DMX channel and the
/// mount frame alone cannot say where the beam goes. Both facts land on the
/// fixture's transform, and this splits them back apart.
struct Manual {
    /// Rotation of the fixture body, relative to the fixture's own frame.
    base: Quat,
    /// Angle the unpatched axle is set to, in radians.
    axle: f32,
}

fn manual_tilt(global: &GlobalTransform, ty: &GdtfType) -> Option<Manual> {
    let patched = |attribute: &str| ty.mode().channels.iter().any(|c| c.attribute == attribute);
    if patched("Tilt") {
        return None;
    }

    let rotation = global.rotation();
    let aim = rotation * Vec3::NEG_Y;
    let ground = Vec3::new(aim.x, 0.0, aim.z);
    let facing = match ground.length_squared() > 1e-6 {
        true => ground.normalize(),
        false => rotation * Vec3::NEG_Z,
    };

    // Standing the fixture on its base is a half turn about its own front-back
    // axis, which keeps the front facing forward; the yaw then points it.
    let upright = Quat::from_rotation_arc(Vec3::NEG_Z, facing) * Quat::from_rotation_z(PI);
    let base = rotation.inverse() * upright;

    // Signed angle from the beam at rest, about the axle, onto the aim.
    let (rest, axis) = (upright * Vec3::NEG_Y, upright * Vec3::X);
    let axle = f32::atan2(rest.cross(aim).dot(axis), rest.dot(aim));
    Some(Manual { base, axle })
}

struct Builder<'a, 'w, 's> {
    cmds: &'a mut Commands<'w, 's>,
    fixture: Entity,
    ty: &'a GdtfType,
    gltfs: &'a Assets<Gltf>,
    gltf_meshes: &'a Assets<GltfMesh>,
    nodes: &'a Assets<GltfNode>,
    meshes: &'a mut Assets<Mesh>,
    mats: &'a mut Assets<StandardMaterial>,
    beams: &'a mut Assets<BeamMaterial>,
    blank: Handle<Image>,
    /// Every part that is not a lens, since GDTF part meshes carry whatever
    /// material their exporter felt like.
    body: Handle<StandardMaterial>,
    /// Fraction of the fixture's measured output one beam cell makes.
    share: f32,
    /// Set until the fixture's first beam claims it.
    casts_shadows: bool,
    manual: Option<Manual>,
}

/// Beams that project, counting a reference as the geometry it instances.
/// Glow surfaces are excluded: they carry their own flux.
fn projectors(gdtf: &Gdtf, geometry: &Geometry) -> usize {
    let kind = match &geometry.kind {
        Kind::Reference(name) => gdtf.find(name).map(|target| &target.kind),
        kind => Some(kind),
    };
    let here = matches!(kind, Some(Kind::Beam(beam)) if !beam.glow);
    here as usize + geometry.children.iter().map(|c| projectors(gdtf, c)).sum::<usize>()
}

impl Builder<'_, '_, '_> {
    fn geometry(&mut self, geometry: &Geometry, chain: &mut Vec<String>) -> Entity {
        let mode = self.ty.mode();
        let root = chain.is_empty();
        chain.push(geometry.name.clone());

        let mut local = Transform::from_matrix(convert(geometry.transform));
        if let (true, Some(manual)) = (root, &self.manual) {
            local.rotation = manual.base * local.rotation;
        }
        let entity = self
            .cmds
            .spawn((Name::new(geometry.name.clone()), local, Visibility::default()))
            .id();

        // A reference instances a top-level geometry at this node's position.
        let target = match &geometry.kind {
            Kind::Reference(name) => self.ty.gdtf.find(name),
            _ => None,
        };
        let (kind, model) = match target {
            Some(g) => (&g.kind, g.model.as_deref()),
            None => (&geometry.kind, geometry.model.as_deref()),
        };

        if matches!(kind, Kind::Axis) {
            // GDTF turns pan about the geometry's z and tilt about its x.
            let axes = [("Pan", Axle::Pan, Vec3::Y), ("Tilt", Axle::Tilt, Vec3::X)];
            let mut driven = false;
            for (attribute, kind, axis) in axes {
                let channel = mode
                    .channels
                    .iter()
                    .position(|c| c.attribute == attribute && c.geometry == geometry.name);
                if let Some(channel) = channel {
                    self.cmds.entity(entity).insert(Motor {
                        fixture: self.fixture,
                        channel,
                        kind,
                        axis,
                        rest: local.rotation,
                        command: default(),
                        travel: default(),
                        trace: default(),
                    });
                    driven = true;
                }
            }
            // The hand-set axle: crank it to where the fixture is aimed.
            if let (false, Some(manual)) = (driven, &self.manual) {
                let mut set = local;
                set.rotation *= Quat::from_axis_angle(Vec3::X, manual.axle);
                self.cmds.entity(entity).insert(set);
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
                Some(lens)
            }
            _ => None,
        };

        let model = model.and_then(|m| self.ty.gdtf.models.get(m));
        if let Some((model, mesh)) = model.and_then(|m| Some((m, m.mesh.as_ref()?))) {
            let gltf = self.gltfs.get(&self.ty.models[mesh]).unwrap();
            let scene = gltf.scenes[0].clone();
            let local = mesh_extents(gltf, self.nodes, self.gltf_meshes, self.meshes)
                .map(|measured| part_transform(measured, model.size))
                .unwrap_or_default();
            let surface = lens.clone().unwrap_or_else(|| self.body.clone());
            let part = self.cmds.spawn((WorldAssetRoot(scene), local, Recolor(surface))).id();
            self.cmds.entity(part).observe(recolor);
            self.cmds.entity(entity).add_child(part);
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

    fn emitter(
        &mut self,
        entity: Entity,
        beam: &file::Beam,
        model: Option<&str>,
        chain: &[String],
        lens: Handle<StandardMaterial>,
    ) {
        let mode = self.ty.mode();
        let gdtf = &self.ty.gdtf;
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
                            range: LIGHT_RANGE,
                            radius: 0.0,
                            // One shadow map per fixture, not per cell. Every
                            // one is a whole extra pass over the scene, and the
                            // cells of an array sit within centimetres of each
                            // other, so they all throw the same shadow.
                            shadow_maps_enabled: std::mem::take(&mut self.casts_shadows),
                            shadow_depth_bias: 0.02,
                            shadow_normal_bias: 1.8,
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
            let material = self.beams.add(BeamMaterial { gobo: self.blank.clone(), ..default() });
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

        let mixing = ["ColorAdd_R", "ColorAdd_G", "ColorAdd_B", "ColorAdd_W"];
        let rgbw = mixing.map(resolve);
        self.cmds.entity(entity).insert(Emitter {
            fixture: self.fixture,
            dimmer: resolve("Dimmer"),
            shutter: resolve("Shutter1"),
            zoom: resolve("Zoom"),
            rgbw,
            leds: mixing.map(|a| self.ty.photometry.led(a)),
            additive: rgbw.iter().any(Option::is_some),
            source: self.ty.photometry.source(),
            wheels: ["Color1", "Color2"].iter().filter_map(|a| resolve(a)).collect(),
            gobo: resolve("Gobo1"),
            gobo_pos: resolve("Gobo1Pos"),
            spin: 0.0,
            gobo_slot: None,
            flux: beam.flux,
            angle: field,
            hotspot: match field > 0.0 {
                true => (beam.angle / field).clamp(0.0, 1.0),
                false => 0.7,
            },
            glow: beam.glow,
            share: match beam.glow {
                true => 1.0,
                false => self.share,
            },
            area: face_area(model.and_then(|m| gdtf.models.get(m)), beam.glow),
            cone: Profile::new(beam.angle, field),
            peak: 0.0,
            angles: Vec2::ZERO,
            built: 0.0,
            light,
            lens,
            beam: beam_handles,
            volume: cone,
        });

        for child in light.iter().chain(cone.iter()) {
            self.cmds.entity(entity).add_child(*child);
        }
    }
}

fn recolor(
    trigger: On<WorldInstanceReady>,
    mut cmds: Commands,
    children: Query<&Children>,
    parts: Query<&Recolor>,
    meshes: Query<(), With<Mesh3d>>,
) {
    let root = trigger.event_target();
    let Ok(Recolor(material)) = parts.get(root) else {
        return;
    };
    for child in children.iter_descendants(root) {
        if meshes.contains(child) {
            // A fixture's own housing encloses its light: GDTF puts the beam
            // origin a centimetre or so inside the head's model box, so a
            // casting housing shadows the whole beam away and nothing it points
            // at is ever lit.
            cmds.entity(child).insert((MeshMaterial3d(material.clone()), NotShadowCaster));
        }
    }
}

/// A wheel slot's filter: the hue it passes, at a luminance of 1, and how much
/// of the source it lets through (its CIE Y, as a fraction).
fn filter(cie: Vec3) -> (Vec3, f32) {
    let Vec3 { x, y, z: transmission } = cie;
    if y <= 0.0 {
        return (Vec3::ONE, 1.0);
    }
    (photometry::xy_to_rgb(x, y), (transmission / 100.0).clamp(0.0, 1.0))
}

pub(super) fn apply(
    library: Res<GdtfLibrary>,
    time: Res<Time>,
    fixtures: Query<&GdtfFixture>,
    mut motors: Query<(&mut Motor, &mut Transform)>,
    mut emitters: Query<&mut Emitter>,
    mut lights: Query<&mut SpotLight>,
    mut visible: Query<&mut Visibility>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut beams: ResMut<Assets<BeamMaterial>>,
    blank: Res<NoGobo>,
) {
    let resolve = |entity| {
        let fixture: &GdtfFixture = fixtures.get(entity).ok()?;
        let ty = library.get(&fixture.kind)?;
        // Motors and emitters cache channel indices, so they're stale until the
        // build catches up with a mode change.
        (fixture.built == Some(ty.mode)).then_some((fixture, ty))
    };

    for (mut motor, mut transform) in &mut motors {
        let Some((fixture, ty)) = resolve(motor.fixture) else { continue };
        let motor = &mut *motor;
        let axle = motor.kind as usize;
        let channel = &ty.mode().channels[motor.channel];
        let target = match motor.kind {
            Axle::Zoom => channel.fraction(fixture.values[motor.channel]) * 100.0,
            _ => {
                let mut degrees = channel.physical(fixture.values[motor.channel]);
                if fixture.invert[axle] {
                    degrees = -degrees;
                }
                degrees + fixture.home[axle]
            }
        };
        let (now, dt, p) = (time.elapsed_secs_f64(), time.delta_secs(), fixture.motion[axle]);
        let command = motor.command.step(now, target, p.smooth, dt);
        let travelled = motor.travel.step(command, p, dt);
        motor.trace.push(now, motor.command.wire(), &motor.travel);
        if motor.kind != Axle::Zoom {
            transform.rotation = motor.rest * Quat::from_axis_angle(motor.axis, travelled.to_radians());
        }
    }

    // Simulated zoom position per fixture, back as a fraction of the range.
    let zooms: HashMap<Entity, f32> = motors
        .iter()
        .filter(|(motor, _)| motor.kind == Axle::Zoom)
        .map(|(motor, _)| (motor.fixture, motor.travel.pos / 100.0))
        .collect();

    for mut emitter in &mut emitters {
        let Some((fixture, ty)) = resolve(emitter.fixture) else { continue };
        let mode = ty.mode();

        // Mix in absolute terms: each LED contributes its own measured share of
        // the fixture's output, so red at full is not as bright as green at full.
        let (mut rgb, mut weight) = (Vec3::ZERO, 0.0);
        if emitter.additive {
            for (&channel, led) in emitter.rgbw.iter().zip(&emitter.leds) {
                let lumens = fixture.level(ty, channel, 0.0) * led.weight;
                rgb += led.rgb * lumens;
                weight += lumens;
            }
        } else {
            weight = ty.photometry.full;
            rgb = emitter.source * weight;
        }
        // Back to a luminance of 1, with the output it stands for kept apart.
        let mut level = ty.photometry.output(weight);
        rgb = match weight > 1e-6 {
            true => rgb / weight,
            false => Vec3::ONE,
        };

        // Colour wheels tint and dim. The slot's chromaticity is the hue it
        // passes; its CIE Y is the whole of what passing it costs, so shape the
        // hue with a peak-normalised transmission and leave luminance to Y.
        for &channel in &emitter.wheels {
            let Some(slot) = fixture.slot(ty, channel) else { continue };
            let Some(cie) = slot.color else { continue };
            let (tint, pass) = filter(cie);
            let shaped = rgb * tint / tint.max_element().max(1e-6);
            rgb = shaped / photometry::luminance(shaped).max(1e-6);
            level *= pass;
        }

        let open = emitter.shutter.is_none_or(|c| {
            let channel = &mode.channels[c];
            let function = channel.function(fixture.values[c]);
            // Strobe functions all keep the shutter nominally open; we don't blink.
            !function.attribute.ends_with("Shutter1") || channel.physical(fixture.values[c]) >= 0.5
        });
        level *= fixture.level(ty, emitter.dimmer, 1.0) * if open { 1.0 } else { 0.0 };

        // Zoom position rather than the degrees the GDTF claims for it: the
        // measured sweep is sampled across the travel, and the declared
        // endpoints are only approximately where the fixture lands. The
        // position is the motor's simulated travel, not the wire. A mode
        // without zoom sits at the wide end, which is where a GDTF's own
        // declared angles are taken.
        let zoom = match zooms.get(&emitter.fixture) {
            Some(travelled) => *travelled,
            None => emitter.zoom.map_or(1.0, |c| mode.channels[c].fraction(fixture.values[c])),
        };
        let angle = match emitter.zoom {
            Some(c) => {
                let channel = &mode.channels[c];
                channel.physical((zoom * channel.max() as f32).round() as u32)
            }
            None => emitter.angle,
        };
        let nominal = (angle * emitter.hotspot, angle, emitter.flux);
        let (hot, field, full) = ty.photometry.beam(zoom, nominal);
        emitter.cone = Profile::new(hot, field);

        // A decorative panel is no part of the beam the lab measured, and it is
        // a diffuse surface rather than a projector.
        let glow = emitter.glow.then_some(emitter.flux);

        // Peak luminous intensity, which is what the fixture is specified by and
        // what both the light and the volume need.
        let peak = match glow {
            Some(flux) => flux / emitter.cone.solid_angle(),
            None => full * emitter.share,
        } * level;
        emitter.peak = peak;
        emitter.angles = Vec2::new(hot, field);

        // A dark emitter still costs a shadow map and a screenful of raymarch,
        // so take it out of the frame entirely rather than scaling it to zero.
        let lit = match peak > 0.0 {
            true => Visibility::Inherited,
            false => Visibility::Hidden,
        };
        for entity in emitter.light.iter().chain(emitter.volume.iter()) {
            if let Ok(mut visibility) = visible.get_mut(*entity) {
                visibility.set_if_neq(lit);
            }
        }

        // Gobo: shows in the beam volume only. A light cookie would also throw the
        // pattern onto whatever the beam lands on, which is not wanted here.
        let gobo = emitter.gobo.and_then(|c| fixture.slot(ty, c)).and_then(|s| s.media.as_ref());
        let gobo_slot = emitter.gobo.and_then(|c| fixture.slot_index(ty, c));
        if emitter.gobo_slot != gobo_slot {
            emitter.gobo_slot = gobo_slot;
            let texture = gobo.and_then(|name| ty.images.get(name)).cloned();

            // Tints the lens face, without the alpha that an albedo map carries.
            mats.get_mut(&emitter.lens).unwrap().emissive_texture = texture.clone();

            if let Some((_, material, _)) = &emitter.beam {
                let mut material = beams.get_mut(material).unwrap();
                material.beam.gobo_spin.z = texture.is_some() as u32 as f32;
                material.gobo = texture.unwrap_or_else(|| blank.0.clone());
            }
        }

        {
            let mut lens = mats.get_mut(&emitter.lens).unwrap();
            let face = match glow {
                // Luminance of a diffuse panel is its flux over its own area
                // and hemisphere; of a lens, the intensity it focuses over it.
                Some(flux) => flux * level / (PI * emitter.area),
                None => peak / emitter.area,
            };
            let face = LENS_KNEE * (1.0 + face / LENS_KNEE).ln();
            lens.emissive = LinearRgba::rgb(rgb.x * face, rgb.y * face, rgb.z * face);
        }

        if let Some(light) = emitter.light {
            let mut light = lights.get_mut(light).unwrap();
            light.color = Color::linear_rgb(rgb.x, rgb.y, rgb.z);
            // Bevy spreads a spot light's lumens over the whole sphere and then
            // masks the cone, so scale back up by that sphere to hand it the
            // on-axis intensity the fixture really makes.
            light.intensity = peak * 2.0 * std::f32::consts::TAU;
            light.outer_angle = emitter.cone.outer().clamp(0.01, TAU_4 - 0.01);
            light.inner_angle = emitter.cone.inner().min(light.outer_angle);
        }

        if let Some((mesh, material, near)) = emitter.beam.clone() {
            // The indexing channel carries two functions: an absolute angle, and
            // above half a continuous rate in degrees per second.
            let spin = match emitter.gobo_pos {
                Some(c) => {
                    let channel = &mode.channels[c];
                    let physical = channel.physical(fixture.values[c]);
                    if channel.function(fixture.values[c]).attribute.ends_with("Gobo1PosRotate") {
                        emitter.spin += physical * time.delta_secs();
                        emitter.spin
                    } else {
                        physical
                    }
                }
                None => 0.0,
            }
            .to_radians();
            let mut material = beams.get_mut(&material).unwrap();
            material.beam.color = rgb.extend(peak);
            (material.beam.gobo_spin.x, material.beam.gobo_spin.y) = (spin.cos(), spin.sin());
            drop(material);

            // The mesh has to reach the outer angle, past where the cone has
            // faded out, or the profile gets clipped by its own geometry.
            let outer = emitter.cone.outer().to_degrees() * 2.0;
            if (outer - emitter.built).abs() > 0.01 {
                *meshes.get_mut(&mesh).unwrap() = cone_mesh(near, outer);
                emitter.built = outer;
            }
        }
    }
}

/// Scattering coefficient of the air, per metre: what fraction of a beam each
/// metre of air turns back out of it. Haze is what makes a beam visible at all,
/// and 0 is clean air where they vanish.
///
/// Koschmieder ties it to how far you can see: `sigma = 3.9 / visual range`.
///
/// | air                        | 1/m    | visual range |
/// |----------------------------|--------|--------------|
/// | indoors, nothing in it     | 1e-5   | 400 km       |
/// | clear desert night         | 5e-4   | 8 km         |
/// | playa with the dust up     | 5e-3   | 800 m        |
/// | a hazer holding a room     | 2e-2   | 200 m        |
/// | dust storm, heavy club fog | 2e-1   | 20 m         |
#[derive(Resource)]
pub struct Haze(pub f32);

impl Default for Haze {
    fn default() -> Self {
        Self(5e-4)
    }
}

/// The beam volume is described in world space, so it trails transform propagation.
pub(super) fn project_beams(
    emitters: Query<(&Emitter, &GlobalTransform)>,
    haze: Res<Haze>,
    mut beams: ResMut<Assets<BeamMaterial>>,
) {
    for (emitter, global) in &emitters {
        let Some((_, material, near)) = &emitter.beam else {
            continue;
        };
        let Some(mut material) = beams.get_mut(material) else {
            continue;
        };

        let (_, rotation, origin) = global.to_scale_rotation_translation();
        let (axis, right, up) = (rotation * Vec3::NEG_Y, rotation * Vec3::X, rotation * Vec3::Z);

        // Sit the apex behind the lens, so the cone has opened to the lens radius
        // by the time it gets there.
        let tan = emitter.cone.outer().tan().max(1e-4);
        let lens = (near / tan).max(1e-3);

        material.beam.apex = (origin - axis * lens).extend(emitter.cone.cos_inner);
        material.beam.axis = axis.extend(emitter.cone.cos_outer);
        material.beam.right = right.extend(lens);
        material.beam.up = up.extend(lens + BEAM_RANGE);
        material.beam.gobo_spin.w = haze.0;
    }
}

/// Gobos arrive without a mip chain, which leaves the beam aliasing against full
/// resolution pixels. Build one as soon as each image lands.
pub(super) fn mip_gobos(
    library: Res<GdtfLibrary>,
    mut events: MessageReader<AssetEvent<Image>>,
    mut images: ResMut<Assets<Image>>,
) {
    for event in events.read() {
        let AssetEvent::Added { id } = event else { continue };
        let ours = library.types.values().any(|ty| ty.images.values().any(|h| h.id() == *id));
        if !ours {
            continue;
        }
        if let Some(mut image) = images.get_mut(*id) {
            super::beam::generate_mipmaps(&mut image);
        }
    }
}

/// Area of an emitting face: a lens is a disc, a glow panel is flat.
fn face_area(model: Option<&file::Model>, glow: bool) -> f32 {
    model
        .map(|m| match glow {
            true => m.size.x * m.size.y,
            false => PI * (m.size.x / 2.0).powi(2),
        })
        .filter(|a| *a > 1e-6)
        .unwrap_or(1e-3)
}

/// A cone spanning `BEAM_RANGE`, with its tip end (radius `near`) at +y.
fn cone_mesh(near: f32, angle: f32) -> Mesh {
    let spread = BEAM_RANGE * (angle.clamp(0.5, 175.0).to_radians() / 2.0).tan();
    Mesh::from(ConicalFrustum { height: BEAM_RANGE, radius_top: near, radius_bottom: near + spread })
}

/// Convert a GDTF transform into bevy's coordinate system.
fn convert(m: Mat4) -> Mat4 {
    let c = Mat4::from_quat(TO_BEVY);
    c * m * c.inverse()
}

/// Size and orient a part mesh against its `<Model>` box.
///
/// Part meshes are raw-unit: real size comes from the box. They also ship in
/// either glTF's y-up or GDTF's own z-up (Chauvet exports y-up, ADJ ships
/// ~113-units-per-metre z-up). Whichever axis mapping is right yields three
/// near-equal scale factors, so the spread between them picks it, which holds
/// whatever units the mesh is in.
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
    // One uniform factor, never a per-axis stretch. A part mesh can carry
    // detail outside the box its model declares, and fitting each axis to the
    // box then shears it; the middle ratio is the one such an outlier moves
    // least. A zero-sized axis means the model doesn't declare it.
    let mut ratios = [f32::MAX; 3];
    let mut declared = 0;
    for axis in 0..3 {
        if target[axis] > 0.0 {
            ratios[declared] = target[axis] / measured[axis];
            declared += 1;
        }
    }
    ratios.sort_by(f32::total_cmp);
    Transform {
        rotation: match zup_wins {
            true => Quat::from_rotation_x(-TAU_4),
            false => Quat::IDENTITY,
        },
        scale: Vec3::splat(match declared {
            0 => 1.0,
            n => ratios[n / 2],
        }),
        ..default()
    }
}

/// Extents of a glTF the size it will render at, node transforms included.
/// Exporters disagree: the Chauvet parts are SketchUp inches under a 0.0254
/// scale on every node, so their raw mesh extents are 39 times the real thing.
fn mesh_extents(
    gltf: &Gltf,
    nodes: &Assets<GltfNode>,
    gltf_meshes: &Assets<GltfMesh>,
    meshes: &Assets<Mesh>,
) -> Option<Vec3> {
    let mut nested: HashSet<AssetId<GltfNode>> = HashSet::new();
    for node in gltf.nodes.iter().filter_map(|handle| nodes.get(handle)) {
        nested.extend(node.children.iter().map(Handle::id));
    }
    let (mut min, mut max) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for root in gltf.nodes.iter().filter(|handle| !nested.contains(&handle.id())) {
        node_extents(root, Affine3A::IDENTITY, nodes, gltf_meshes, meshes, &mut min, &mut max);
    }
    (min.x <= max.x).then(|| max - min)
}

fn node_extents(
    handle: &Handle<GltfNode>,
    parent: Affine3A,
    nodes: &Assets<GltfNode>,
    gltf_meshes: &Assets<GltfMesh>,
    meshes: &Assets<Mesh>,
    min: &mut Vec3,
    max: &mut Vec3,
) {
    let Some(node) = nodes.get(handle) else { return };
    let world = parent * node.transform.compute_affine();
    let mesh = node.mesh.as_ref().and_then(|handle| gltf_meshes.get(handle));
    for primitive in mesh.iter().flat_map(|mesh| &mesh.primitives) {
        let Some(aabb) = meshes.get(&primitive.mesh).and_then(Mesh::compute_aabb) else {
            continue;
        };
        for corner in corners(&aabb) {
            let point = world.transform_point3(corner);
            *min = min.min(point);
            *max = max.max(point);
        }
    }
    for child in &node.children {
        node_extents(child, world, nodes, gltf_meshes, meshes, min, max);
    }
}

/// The eight corners of a box.
pub fn corners(aabb: &Aabb) -> impl Iterator<Item = Vec3> {
    let (center, half) = (Vec3::from(aabb.center), Vec3::from(aabb.half_extents));
    (0..8u32).map(move |corner| {
        let sign = |bit| match corner & bit == 0 {
            true => -1.0,
            false => 1.0,
        };
        center + Vec3::new(sign(1), sign(2), sign(4)) * half
    })
}
