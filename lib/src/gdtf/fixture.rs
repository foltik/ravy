//! Assembling GDTF fixtures in the world and driving them from DMX.
//!
//! A fixture type is parsed once into the [`GdtfLibrary`]; every instance is an
//! entity carrying [`GdtfFixture`], whose part meshes, motors and emitters are
//! spawned underneath it. Instances with a DMX address follow the [`Universe`]
//! the rig is being driven with, so the sim sees exactly what the wire does.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use bevy::asset::io::memory::Dir;
use bevy::camera::RenderTarget;
use bevy::camera::primitives::{Aabb, MeshAabb};
use bevy::window::WindowRef;
use bevy::gltf::{GltfMesh, GltfNode};
use bevy::light::NotShadowCaster;
use bevy::math::Affine3A;
use bevy::world_serialization::WorldInstanceReady;

use super::beam::{self, Volumetrics};
use super::file::{self, Gdtf, Geometry, Kind};
use super::motion;
use super::photometry::{self, Led, Photometry, Profile};
use super::outcast::{self, Ring, Strobe};
use crate::prelude::*;

/// GDTF is z-up with the beam along -z; bevy is y-up.
pub const TO_BEVY: Quat =
    Quat::from_xyzw(-std::f32::consts::FRAC_1_SQRT_2, 0.0, 0.0, std::f32::consts::FRAC_1_SQRT_2);

/// Length of the visible beam cone, in meters.
pub const BEAM_RANGE: f32 = 12.0;

/// Where the lens face's luminance starts compressing, in cd/m^2. Looking into
/// one of these really is millions of nits, and handing that to bloom smears it
/// over the whole frame. Compressing logarithmically above the knee keeps the
/// glow growing with the dimmer all the way down instead of pinning at the top.
const LENS_KNEE: f32 = 2.0e3;

const TAU_4: f32 = std::f32::consts::FRAC_PI_2;
const PI: f32 = std::f32::consts::PI;
const DEG: f32 = PI / 180.0;

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
    pub(super) types: HashMap<String, GdtfType>,
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
    /// Outcast ring macro state. Left alone for every other fixture.
    ring: Ring,
    /// Outcast shutter state, indexed by channel. Empty for every other fixture.
    strobe: Vec<Strobe>,
    /// How far each wheel has turned, in degrees, indexed by channel.
    spin: Vec<f32>,
    /// Mode the parts under this fixture were built for.
    built: Option<usize>,
}

/// A device driven over DMX with a GDTF standing in for it in the sim.
pub trait GdtfDevice: DmxDevice {
    /// Type key in the [`GdtfLibrary`].
    const KIND: &'static str;
    /// DMX mode in the `.gdtf`, which is the personality the fixture is set to.
    const MODE: &'static str;

    /// Where the fixture rests with nothing to do, indexed by [`Axle`]: pan and
    /// tilt in degrees off the middle of their travel, zoom as a fraction of
    /// its own. 0 is mechanically centred, which is home unless the fixture is
    /// mounted somewhere that makes another rest angle the useful one.
    const HOME: [f32; 3] = [0.0; 3];

    /// Total pan and tilt travel in degrees, signed by which way the axle turns
    /// as the DMX value rises. Anything aiming a head in angles needs this: the
    /// travel is not always the round number, and not every fixture pans the
    /// same way. It is what the `.gdtf` declares for the channel, which
    /// [`declared_travel`] checks against.
    const TRAVEL: [f32; 2] = [540.0, 270.0];

    /// Pan, tilt and zoom travel.
    fn motion() -> [motion::Params; 3] {
        [motion::Params::pan(), motion::Params::tilt(), motion::Params::zoom()]
    }
}

/// The pan and tilt travel a parsed type declares, in the same signed degrees
/// as [`GdtfDevice::TRAVEL`], for whichever of the two it has a channel for.
pub fn declared_travel(ty: &GdtfType) -> [Option<f32>; 2] {
    ["Pan", "Tilt"].map(|attribute| {
        let channel = ty.mode().channels.iter().find(|c| c.attribute == attribute)?;
        let (from, to) = channel.functions.first()?.physical;
        Some(to - from)
    })
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
            ring: Ring::default(),
            strobe: vec![],
            spin: vec![],
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

    /// The wheel a channel is picking from, the two slots the gate is showing,
    /// and where the edge between them lies across the aperture in radii: +1
    /// for all of the first, -1 for all of the second.
    ///
    /// Read off where the wheel has really turned to rather than the wire, so a
    /// move sweeps the slots between here and there past the gate the way the
    /// fixture does, and a half step parks it square on the edge between two.
    fn showing<'a>(&self, ty: &'a GdtfType, channel: usize) -> Option<(&'a str, [usize; 2], f32)> {
        let chart = &ty.mode().channels[channel];
        let wheel = chart.function(self.values[channel]).wheel.as_deref()?;
        let count = ty.gdtf.wheels.get(wheel)?.len();
        if count == 0 {
            return None;
        }

        let pitch = 360.0 / count as f32;
        let angle = self.spin.get(channel).copied().unwrap_or(0.0);
        let place = (angle / pitch).rem_euclid(count as f32);

        // Round to the slot squarest in the gate rather than truncating toward
        // the one behind it: a wheel parked on a slot lands a hair either side
        // of a whole number of pitches, which a floor reads as the wrong slot.
        let nearest = place.round();
        let past = (place - nearest) * pitch;
        let slot = nearest as usize % count;
        let next = match past < 0.0 {
            true => (slot + count - 1) % count,
            false => (slot + 1) % count,
        };
        // The gate only has two slots in it near the edge between them, and that
        // edge is half a pitch from the middle of either.
        let edge = (2.0 * (pitch * 0.5 - past.abs()) / HANDOVER).clamp(0.0, 1.0);
        Some((wheel, [slot, next], edge))
    }

    /// The filters a colour wheel is showing, one per side of the split: the
    /// hue each passes at a luminance of 1, and how much of the source it lets
    /// through.
    fn filters(&self, ty: &GdtfType, channel: usize) -> Option<([(Vec3, f32); 2], f32)> {
        let (wheel, slots, edge) = self.showing(ty, channel)?;
        let slots = spread(slots, edge);
        let wheel = ty.gdtf.wheels.get(wheel)?;
        let pick = |i: usize| wheel.get(i).and_then(|s| s.color).map_or((Vec3::ONE, 1.0), filter);
        Some(([pick(slots[0]), pick(slots[1])], edge))
    }
}

/// Degrees of wheel travel over which the gate shows two slots at once, centred
/// on the edge between them. Slots are wider than the aperture, so a turning
/// wheel shows a single one for most of its pitch and both only across this.
const HANDOVER: f32 = 25.0;

/// How fast a wheel gets from one slot to the next, in degrees per second. The
/// GDTF times nothing but the spin ranges, and a real wheel is quick between
/// two positions rather than instant: at 400 an adjacent slot takes 100ms and a
/// whole turn most of a second.
pub const WHEEL_SPEED: f32 = 400.0;

/// Which way the gate is cut, as an angle in degrees about the beam axis: the
/// direction a wheel's slots travel across the aperture, and so the angle a
/// split beam is divided at. Not something the GDTF carries; measured off the
/// hydros by eye.
const SPLIT: f32 = 105.0;

/// How far a prism throws each copy off the beam axis, in degrees: the step
/// from one copy to the next on a linear prism, and the radius of the ring on
/// a circular one. The GDTF describes its facets with rotations that claim a
/// 27 degree spread, and a linear prism whose facets are not in a line, so
/// these are set by eye like [`SPLIT`].
const PRISM_STEP: f32 = 2.2;
const PRISM_RING: f32 = 2.6;

/// Both slots when the gate is really straddling them, and the near one twice
/// over when it is not. Only the mechanism that is mid-move differs across the
/// split; everything else in the path applies to the whole beam.
fn spread<T: Copy>(pair: [T; 2], edge: f32) -> [T; 2] {
    match edge < 1.0 {
        true => pair,
        false => [pair[0]; 2],
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
    /// Gobo slot currently applied, to avoid restamping it every frame.
    gobo_slot: Option<usize>,
    /// Prism selection channel.
    prism: Option<usize>,
    /// Prism indexing channel, which turns the copies about the beam.
    prism_pos: Option<usize>,
    /// Luminous flux in lumens the GDTF claims, used where nothing measured
    /// this fixture.
    flux: f32,
    /// Field angle to use when the mode has no zoom channel.
    angle: f32,
    /// BeamAngle / FieldAngle: where the cone falls off from hot to soft.
    hotspot: f32,
    /// A surface that lights up but casts no beam.
    pub glow: bool,
    /// Segment of the Outcast's ring this cell is. None for anything else.
    segment: Option<usize>,
    /// Fraction of the fixture's measured output this cell makes. The lab lit
    /// every cell of an array at once, so one of seven throws a seventh.
    share: f32,
    /// Area of the emitting face in square metres, which sets its luminance.
    area: f32,
    /// Current beam profile, shared with the volume shader.
    cone: Profile,
    /// What the emitter is putting out right now, in candela, across the whole
    /// aperture. The two sides of a split mixed by how much each covers.
    pub peak: f32,
    /// Beam and field angle it is putting it out over, in degrees.
    pub angles: Vec2,
    /// Linear sRGB it is putting out, at a luminance of one, mixed the same way.
    tint: Vec3,
    /// What each side of the split is showing.
    sides: [Side; 2],
    /// Where the edge between them lies across the aperture, in radii. +1 when
    /// nothing is mid-move, which is the whole beam on the first side.
    split: f32,
    /// Where the gobo has been turned to, in radians.
    gobo_spin: f32,
    /// Copies the prism is making, negative for a linear one and 0 for none.
    copies: f32,
    /// xy: where the first copy sits across the aperture, in radii. zw: how to
    /// get from one copy to the next.
    facets: Vec4,
    /// How far through its cycle a shake is, in turns. Carried rather than
    /// taken from the clock, so changing the rate doesn't jump the gobo.
    shake: f32,
    /// The emitter's own rest rotation, which a shake swings the beam off.
    rest: Quat,
    lens: Handle<StandardMaterial>,
    /// Radius of the emitting face, which is where the cone starts. None for a
    /// glow surface, which projects nothing.
    aperture: Option<f32>,
}

impl Emitter {
    /// A hand-measured emitter with no GDTF behind it: a fixed cone throwing
    /// `flux` lumens over the given full beam/field angles from a lens of
    /// `aperture` radius, driven by [`Emitter::set`] instead of the universe.
    pub fn manual(beam: f32, field: f32, flux: f32, aperture: f32) -> Self {
        Self {
            fixture: Entity::PLACEHOLDER,
            dimmer: None,
            shutter: None,
            zoom: None,
            rgbw: [None; 4],
            leds: [Led { rgb: Vec3::ONE, weight: 1.0 }; 4],
            additive: false,
            source: Vec3::ONE,
            wheels: Vec::new(),
            gobo: None,
            gobo_pos: None,
            gobo_slot: None,
            prism: None,
            prism_pos: None,
            flux,
            angle: field,
            hotspot: (beam / field.max(1e-3)).clamp(0.0, 1.0),
            glow: false,
            segment: None,
            share: 1.0,
            area: PI * aperture * aperture,
            cone: Profile::new(beam, field),
            peak: 0.0,
            angles: Vec2::new(beam, field),
            tint: Vec3::ZERO,
            sides: [Side::default(); 2],
            split: 1.0,
            gobo_spin: 0.0,
            copies: 0.0,
            facets: Vec4::ZERO,
            shake: 0.0,
            rest: Quat::IDENTITY,
            lens: Handle::default(),
            aperture: Some(aperture),
        }
    }

    /// Drive a manual emitter: linear sRGB as the show mixed it, level and all.
    pub fn set(&mut self, rgb: Vec3) {
        let lum = photometry::luminance(rgb);
        self.tint = match lum > 1e-6 {
            true => rgb / lum,
            false => Vec3::ONE,
        };
        self.peak = lum * self.flux / self.cone.solid_angle();
        self.sides = [Side { tint: self.tint, peak: self.peak, layer: 0.0 }; 2];
    }
}

/// One side of what the gate is showing.
#[derive(Clone, Copy, Default)]
struct Side {
    /// Linear sRGB through this slot, at a luminance of one.
    tint: Vec3,
    /// Peak luminous intensity through it, in candela.
    peak: f32,
    /// Layer of the packed gobo array, 0 for an open gate.
    layer: f32,
}

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

/// Copy each patched fixture's slice of the universe into its channel values,
/// then run whatever the values alone drive. An unaddressed fixture is written
/// to directly, so it is only skipped by the copy.
pub(super) fn drive(
    universe: Option<Res<Universe>>,
    time: Res<Time>,
    library: Res<GdtfLibrary>,
    mut fixtures: Query<&mut GdtfFixture>,
) {
    for mut fixture in &mut fixtures {
        if !fixture.is_built() {
            continue;
        }
        let Some(ty) = library.get(&fixture.kind) else { continue };
        let mode = ty.mode();
        if let (Some(universe), true) = (&universe, fixture.address != 0) {
            let base = fixture.address - 1;
            let values: Vec<u32> = mode
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

        // Where every wheel has actually turned to. A slot chart names a target
        // the wheel takes time to reach, and a spin range names a rate: either
        // way the position is ours to carry, and what the gate shows follows
        // from it rather than straight from the wire.
        let dt = time.delta_secs();
        fixture.spin.resize(mode.channels.len(), 0.0);
        for (i, chart) in mode.channels.iter().enumerate() {
            let value = fixture.values.get(i).copied().unwrap_or(0);
            let function = chart.function(value);
            let Some(wheel) = function.wheel.as_deref() else { continue };
            let count = ty.gdtf.wheels.get(wheel).map_or(0, Vec::len);
            if count == 0 {
                continue;
            }

            let angle = fixture.spin[i];
            let next = if function.attribute.ends_with("WheelSpin")
                || function.attribute.ends_with("PosRotate")
            {
                angle + chart.rate(value) * dt
            } else if let Some((_, slot, offset)) = chart.slot(value) {
                // Whichever way round is shorter, at the speed the wheel moves.
                let pitch = 360.0 / count as f32;
                let short = ((slot as f32 + offset) * pitch - angle + 180.0).rem_euclid(360.0);
                angle + (short - 180.0).clamp(-WHEEL_SPEED * dt, WHEEL_SPEED * dt)
            } else {
                angle
            };
            fixture.spin[i] = next;
        }

        // The one fixture whose firmware we replay: its ring macros animate the
        // 12 segments, and its shutter really blinks.
        if !outcast::matches(&ty.gdtf) {
            continue;
        }
        let find = |attribute: &str| mode.channels.iter().position(|c| c.attribute == attribute);
        let value = |channel: Option<usize>| {
            channel.and_then(|c| fixture.values.get(c).copied()).unwrap_or(0)
        };
        let (pattern, speed) = (value(find("Effects2")), value(find("Effects2Rate")));
        fixture.ring.step(pattern, speed, dt);

        fixture.strobe.resize_with(mode.channels.len(), Strobe::default);
        for (i, _) in mode.channels.iter().enumerate().filter(|(_, c)| c.attribute == "Shutter1") {
            let value = fixture.values.get(i).copied().unwrap_or(0);
            fixture.strobe[i].step(value, dt);
        }
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
            body: body.0.clone(),
            share: 1.0 / cells.max(1) as f32,
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
    /// Every part that is not a lens, since GDTF part meshes carry whatever
    /// material their exporter felt like.
    body: Handle<StandardMaterial>,
    /// Fraction of the fixture's measured output one beam cell makes.
    share: f32,
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
                self.emitter(entity, beam, model, chain, lens.clone(), local.rotation);
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
            // Meshless beams still have a size: stand in with a disc. Flat, and
            // facing down the beam: given the thickness the model claims, its
            // rim and back face end up buried in the head that houses it and
            // fight with it. It also sits across the aperture the beam starts
            // at, so like the rest of the fixture it must not block that beam's
            // own light.
            let disc = self.meshes.add(Circle::new(model.size.x / 2.0));
            let facing = Transform::from_rotation(Quat::from_rotation_x(TAU_4));
            let part = self
                .cmds
                .spawn((Mesh3d(disc), MeshMaterial3d(lens.clone()), facing, NotShadowCaster))
                .id();
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
        rest: Quat,
    ) {
        let mode = self.ty.mode();
        let gdtf = &self.ty.gdtf;
        let resolve = |attribute: &str| mode.resolve(gdtf, chain, attribute);

        // FieldAngle is the full cone; BeamAngle is just the hot core.
        let field = match beam.field > 0.0 {
            true => beam.field,
            false => beam.angle,
        };

        // Prefer the lens size over BeamRadius, which is the far-field radius.
        let aperture = (!beam.glow).then(|| {
            model
                .and_then(|m| gdtf.models.get(m))
                .map(|m| m.size.x / 2.0)
                .filter(|r| *r > 0.0)
                .unwrap_or(beam.radius)
        });

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
            gobo_slot: None,
            prism: resolve("Prism1"),
            prism_pos: resolve("Prism1Pos"),
            flux: beam.flux,
            angle: field,
            hotspot: match field > 0.0 {
                true => (beam.angle / field).clamp(0.0, 1.0),
                false => 0.7,
            },
            glow: beam.glow,
            segment: outcast::segment(gdtf, chain),
            share: match beam.glow {
                true => 1.0,
                false => self.share,
            },
            area: face_area(model.and_then(|m| gdtf.models.get(m)), beam.glow),
            cone: Profile::new(beam.angle, field),
            peak: 0.0,
            angles: Vec2::ZERO,
            tint: Vec3::ZERO,
            sides: [Side::default(); 2],
            split: 1.0,
            gobo_spin: 0.0,
            copies: 0.0,
            facets: Vec4::ZERO,
            shake: 0.0,
            rest,
            lens,
            aperture,
        });
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

/// What a filter passes outside the band it is named for. A slot's GDTF colour
/// is the chromaticity of white light through it, not the curve that made it,
/// and converting one straight to rgb pins whole channels at zero. Two filters
/// in series then multiply out to whatever the gamut clip happened to leave.
/// Real dichroics have skirts that overlap, and this is the floor standing in
/// for them.
const LEAKAGE: f32 = 0.12;

/// A wheel slot's transmission per channel, peak-normalised so that only its
/// hue is carried here, and how much of the source it lets through overall
/// (its CIE Y, as a fraction).
fn filter(cie: Vec3) -> (Vec3, f32) {
    let Vec3 { x, y, z: transmission } = cie;
    if y <= 0.0 {
        return (Vec3::ONE, 1.0);
    }
    let hue = photometry::xy_to_rgb(x, y);
    let band = hue / hue.max_element().max(1e-6);
    (band.max(Vec3::splat(LEAKAGE)), (transmission / 100.0).clamp(0.0, 1.0))
}

pub(super) fn apply(
    library: Res<GdtfLibrary>,
    time: Res<Time>,
    fixtures: Query<&GdtfFixture>,
    // A geometry is an axle or a beam, never both, but only the filter says so.
    mut motors: Query<(&mut Motor, &mut Transform), Without<Emitter>>,
    mut emitters: Query<(&mut Emitter, &mut Transform)>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    gobos: Res<Volumetrics>,
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

    for (mut emitter, mut transform) in &mut emitters {
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
        let level = ty.photometry.output(weight);
        rgb = match weight > 1e-6 {
            true => rgb / weight,
            false => Vec3::ONE,
        };

        // Colour wheels tint and dim. The slot's chromaticity is the hue it
        // passes; its CIE Y is the whole of what passing it costs, so shape the
        // hue with a peak-normalised transmission and leave luminance to Y.
        //
        // A wheel caught between two slots throws both at once, so from here the
        // beam is two of everything. Two wheels mid-move would really cut the
        // aperture up twice over; the one that has turned furthest takes it.
        let mut tint = [rgb; 2];
        let mut pass = [level; 2];
        let mut split = 1.0_f32;
        for &channel in &emitter.wheels {
            let Some((filters, edge)) = fixture.filters(ty, channel) else { continue };
            for (side, (band, through)) in filters.iter().enumerate() {
                let shaped = tint[side] * *band;
                tint[side] = shaped / photometry::luminance(shaped).max(1e-6);
                pass[side] *= through;
            }
            split = split.min(edge);
        }

        // The Outcast's own shutter timing, which really blinks. Every other
        // fixture falls back to the GDTF's function, whose strobe ranges all
        // read as nominally open.
        let open = emitter.shutter.is_none_or(|c| {
            fixture.strobe.get(c).and_then(Strobe::open).unwrap_or_else(|| {
                let channel = &mode.channels[c];
                let function = channel.function(fixture.values[c]);
                !function.attribute.ends_with("Shutter1")
                    || channel.physical(fixture.values[c]) >= 0.5
            })
        });
        let mut whole = fixture.level(ty, emitter.dimmer, 1.0) * if open { 1.0 } else { 0.0 };

        // The Outcast's ring macro blacks out the segments its frame leaves unlit.
        if emitter.segment.and_then(|s| fixture.ring.lit(s)) == Some(false) {
            whole = 0.0;
        }
        pass = pass.map(|p| p * whole);

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
        let source = match glow {
            Some(flux) => flux / emitter.cone.solid_angle(),
            None => full * emitter.share,
        };
        emitter.split = split;
        emitter.sides = [0, 1].map(|side| Side {
            tint: tint[side],
            peak: source * pass[side],
            layer: 0.0,
        });

        // Neither the spot light nor the lens face can be split, so they take the
        // two sides mixed by how much of the aperture each covers.
        let far = ((1.0 - split) * 0.5).clamp(0.0, 1.0);
        let level = pass[0] * (1.0 - far) + pass[1] * far;
        let peak = emitter.sides[0].peak * (1.0 - far) + emitter.sides[1].peak * far;
        let mixed = emitter.sides[0].tint * (1.0 - far) + emitter.sides[1].tint * far;
        let rgb = mixed / photometry::luminance(mixed).max(1e-6);
        emitter.peak = peak;
        emitter.angles = Vec2::new(hot, field);
        emitter.tint = rgb;

        // Gobo. A turning wheel hands one over to the next the same way a colour
        // wheel does, so the two sides can be showing different patterns.
        let showing = emitter.gobo.and_then(|c| fixture.showing(ty, c));
        let mut near = None;
        if let Some((wheel, slots, edge)) = showing {
            let media = |slot: usize| {
                let name = ty.gdtf.wheels.get(wheel)?.get(slot)?.media.as_ref()?;
                ty.images.get(name).cloned()
            };
            let images = spread(slots, edge).map(media);
            for (side, image) in images.iter().enumerate() {
                emitter.sides[side].layer = gobos.layer(image.as_ref());
            }
            emitter.split = emitter.split.min(edge);
            near = Some((slots[0], images[0].clone()));
        }

        // The lens face carries whichever pattern is squarest in the gate.
        let gobo_slot = near.as_ref().map(|(slot, _)| *slot);
        if emitter.gobo_slot != gobo_slot {
            emitter.gobo_slot = gobo_slot;
            // Tints the lens face, without the alpha that an albedo map carries.
            mats.get_mut(&emitter.lens).unwrap().emissive_texture =
                near.and_then(|(_, image)| image);
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

        // A shaking wheel rattles its slot across the gate rather than turning
        // it, which swings the beam a little way off where it is pointing. The
        // head itself does not move, so this is the emitter's own rotation and
        // not the axles'.
        let rate = emitter.gobo.map_or(0.0, |c| shake(&mode.channels[c], fixture.values[c]));
        emitter.shake = (emitter.shake + rate * time.delta_secs()).fract();
        let swing = match rate > 0.0 {
            true => SHAKE_SWING * wave(emitter.shake, rate) * DEG,
            false => Vec2::ZERO,
        };
        let rotation =
            emitter.rest * Quat::from_rotation_x(swing.x) * Quat::from_rotation_z(swing.y);
        if transform.rotation != rotation {
            transform.rotation = rotation;
        }

        emitter.gobo_spin = indexed(fixture, ty, emitter.gobo_pos).to_radians();

        // Prism: every facet throws a whole copy of the beam a fixed angle off
        // the axis. Across the aperture that deviation is the same offset at
        // any depth, so it is carried in radii, which also means zooming wide
        // draws the copies together the way the fixture does.
        let selected = emitter.prism.and_then(|c| prism_slot(ty, c, fixture.values[c]));
        let (mut copies, mut facets) = (0.0, Vec4::ZERO);
        if let Some(slot) = selected {
            let spin = indexed(fixture, ty, emitter.prism_pos).to_radians();
            let n = slot.facets as f32;
            let tan = emitter.cone.outer().tan().max(1e-4);
            let radii = |degrees: f32| degrees.to_radians().tan() / tan;
            // Nothing structural in the GDTF says which shape a prism throws.
            match slot.name.to_ascii_lowercase().contains("linear") {
                true => {
                    let step = Vec2::from_angle(SPLIT.to_radians() + spin) * radii(PRISM_STEP);
                    let first = -step * (n - 1.0) / 2.0;
                    (copies, facets) = (-n, first.extend(step.x).extend(step.y));
                }
                false => {
                    let first = Vec2::from_angle(spin) * radii(PRISM_RING);
                    let turn = Vec2::from_angle(std::f32::consts::TAU / n);
                    (copies, facets) = (n, first.extend(turn.x).extend(turn.y));
                }
            }
        }
        emitter.copies = copies;
        emitter.facets = facets;
    }
}

/// Where an indexing channel has turned to, in degrees. These carry an
/// absolute angle low down and a continuous rate above it, which [`drive`] has
/// already integrated.
fn indexed(fixture: &GdtfFixture, ty: &GdtfType, channel: Option<usize>) -> f32 {
    let Some(c) = channel else { return 0.0 };
    let channel = &ty.mode().channels[c];
    match channel.function(fixture.values[c]).attribute.ends_with("PosRotate") {
        true => fixture.spin.get(c).copied().unwrap_or(0.0),
        false => channel.physical(fixture.values[c]),
    }
}

/// The prism a channel has selected, if it is on one. Open picks no slot, and
/// neither does the macro half of the chart.
fn prism_slot(ty: &GdtfType, channel: usize, value: u32) -> Option<&file::Slot> {
    let (wheel, slot, _) = ty.mode().channels[channel].slot(value)?;
    ty.gdtf.wheels.get(wheel)?.get(slot).filter(|slot| slot.facets > 1)
}

/// How far a wheel shake swings the beam, in degrees: across the tilt axle,
/// then across the pan axle. The throw runs on a slant, mostly up and down.
/// The gate shakes the gobo, not the yoke, so the beam only twitches.
const SHAKE_SWING: Vec2 = Vec2::new(1.2, -0.7);

/// How fast a shake runs at either end of its chart range, in hertz.
const SHAKE_RATE: [f32; 2] = [0.85, 4.8];

/// How long a shake rests at the end of a throw, in seconds, and the most of
/// the throw that rest may take. It is a fixed wait rather than a share of the
/// cycle, so a slow shake spends nearly all of its time crossing and only a
/// fast one reads as a flick and a pause.
const SHAKE_DWELL: [f32; 2] = [0.06, 0.6];

/// Where a shake has swung to, -1..1, `phase` cycles into one running at
/// `rate` hertz. The gate drives the gobo across at a steady rate, holds it
/// against the stop, then drives it back.
fn wave(phase: f32, rate: f32) -> f32 {
    let [hold, most] = SHAKE_DWELL;
    let dwell = (hold * rate * 2.0).min(most);
    let phase = phase.rem_euclid(1.0) * 2.0;
    let across = (phase.fract() / (1.0 - dwell)).min(1.0);
    let swung = 2.0 * across - 1.0;
    match phase < 1.0 {
        true => swung,
        false => -swung,
    }
}

/// Hertz the wheel on `channel` is shaking at, and 0 where it is not shaking.
/// A shake runs across one chart entry, so where the value sits in that entry
/// is the whole of what it says.
fn shake(channel: &file::Channel, value: u32) -> f32 {
    if !channel.function(value).attribute.contains("Shake") {
        return 0.0;
    }
    let [slow, fast] = SHAKE_RATE;
    slow + channel.entry(value) * (fast - slow)
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

/// How far a beam bleeds past the edge of its cone, standing in for the light
/// haze scatters more than once.
///
/// The march follows only a photon's first bounce, which leaves a shaft with a
/// harder edge than any real one in fog and no halo around it at all. Tracing
/// the rest would cost far more than the whole pass does; this is the shape
/// those bounces leave, without them.
#[derive(Resource, Clone, Copy)]
pub struct Glow {
    /// How far past the rim it reaches, in cone radii.
    pub width: f32,
    /// What it is worth at the rim, as a fraction of the beam on axis.
    pub level: f32,
}

impl Default for Glow {
    fn default() -> Self {
        Self { width: 0.3, level: 0.08 }
    }
}

/// Gather the frame's lit cones and hand them to the beam pass. Beams are in
/// world space, so this trails transform propagation.
pub(super) fn project_beams(
    emitters: Query<(&Emitter, &GlobalTransform)>,
    cameras: Query<(&Camera, &GlobalTransform, Option<&RenderTarget>), With<Camera3d>>,
    haze: Res<Haze>,
    glow: Res<Glow>,
    mut volumetrics: ResMut<Volumetrics>,
) {
    // The venue view is whichever camera draws the primary window; offscreen
    // passes and popout windows carry their own RenderTarget.
    let Some((camera, view)) = cameras.iter().find_map(|(camera, view, target)| {
        matches!(target, None | Some(RenderTarget::Window(WindowRef::Primary)))
            .then_some((camera, view))
    }) else {
        return;
    };
    let Some(size) = camera.physical_viewport_size() else { return };
    let clip = camera.clip_from_view() * view.affine().inverse();
    let cut = Vec2::from_angle(SPLIT.to_radians());

    let mut list = std::mem::take(&mut volumetrics.list);
    list.clear();
    for (emitter, global) in &emitters {
        let Some(near) = emitter.aperture else { continue };
        if emitter.peak <= 0.0 || list.len() >= beam::MAX_BEAMS {
            continue;
        }

        let (_, rotation, origin) = global.to_scale_rotation_translation();
        let (axis, right, up) = (rotation * Vec3::NEG_Y, rotation * Vec3::X, rotation * Vec3::Z);

        // Sit the apex behind the lens, so the cone has opened to the lens radius
        // by the time it gets there.
        let tan = emitter.cone.outer().tan().max(1e-4);
        let lens = (near / tan).max(1e-3);

        let [near, far] = emitter.sides;
        list.push(beam::Beam {
            color: near.tint.extend(near.peak),
            color2: far.tint.extend(far.peak),
            apex: (origin - axis * lens).extend(emitter.cone.cos_inner),
            axis: axis.extend(emitter.cone.cos_outer),
            right: right.extend(lens),
            up: up.extend(lens + BEAM_RANGE),
            gobo: Vec4::new(
                emitter.gobo_spin.cos(),
                emitter.gobo_spin.sin(),
                near.layer,
                emitter.copies,
            ),
            split: cut.extend(emitter.split).extend(far.layer),
            prism: emitter.facets,
        });
    }

    volumetrics.list = list;
    let air = Vec4::new(haze.0, glow.width, glow.level, 0.0);
    beam::publish(&mut volumetrics, clip, size, air);
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
