//! The fullscreen shader patterns: each one a quad running its shader behind
//! its own camera into the feed, all fed from the show here. Phases are
//! integrated on the CPU so a speed knob or a beat surge never pops the
//! picture.

use bevy::camera::visibility::RenderLayers;
use bevy::camera::{Projection, RenderTarget, ScalingMode};
use bevy::light::NotShadowCaster;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;
use lib::prelude::*;

use super::{
    Beats, LAYER_BOXES, LAYER_KALEIDO, LAYER_RINGS, LAYER_SPIRAL, LAYER_WORMHOLE, PX, Pattern,
    PatternCam, Screen, to_linear,
};
use crate::logic::State;

/// They all wear the same uniforms; only the shader differs.
macro_rules! flat_material {
    ($name:ident, $path:literal) => {
        #[derive(Asset, TypePath, AsBindGroup, Clone)]
        pub struct $name {
            #[uniform(0)]
            pub clock: Vec4,
            #[uniform(1)]
            pub color0: LinearRgba,
            #[uniform(2)]
            pub color1: LinearRgba,
            #[uniform(3)]
            pub params: Vec4,
        }

        impl Material for $name {
            fn fragment_shader() -> ShaderRef {
                $path.into()
            }
            fn alpha_mode(&self) -> AlphaMode {
                AlphaMode::Opaque
            }
        }
    };
}

flat_material!(SpiralMaterial, "ledwall/spiral.wgsl");
flat_material!(WormholeMaterial, "ledwall/wormhole.wgsl");
flat_material!(KaleidoMaterial, "ledwall/kaleido.wgsl");
flat_material!(RingsMaterial, "ledwall/rings.wgsl");
flat_material!(BoxesMaterial, "ledwall/boxes.wgsl");

#[derive(Resource, Clone)]
pub struct SpiralParams {
    /// How hard the spokes wind into the tunnel.
    pub swirl: f32,
    /// Spin, radians per second.
    pub speed: f32,
    /// Spoke count; rounded to even so the colour alternation closes.
    pub spokes: f32,
    /// How far in from the rim the tunnel fades up.
    pub cutoff: f32,
    /// Standing brightness.
    pub amount: f32,
    /// Brightness the beat adds, dying off linearly like the milstrike decay.
    pub pulse: f32,
    /// Spoke width, fraction of the gap-to-gap period.
    pub width: f32,
    /// Beats per hit: the pulse fires every this many beats.
    pub every: f32,
}

impl Default for SpiralParams {
    fn default() -> Self {
        Self {
            swirl: 1.0,
            speed: 6.0,
            spokes: 6.0,
            cutoff: 0.7,
            amount: 1.0,
            pulse: 0.75,
            width: 0.5,
            every: 1.0,
        }
    }
}

#[derive(Resource, Clone)]
pub struct WormholeParams {
    /// Flight speed, phase per second.
    pub speed: f32,
    /// Extra flight speed at the kick, decaying over the beat.
    pub surge: f32,
    /// Tube thickness.
    pub warp: f32,
    /// Thickness the kick adds, like resolve's kick breathing.
    pub pulse: f32,
    /// Beats per hit: surge and pulse fire every this many beats.
    pub every: f32,
}

impl Default for WormholeParams {
    fn default() -> Self {
        Self { speed: 0.6, surge: 2.0, warp: 0.65, pulse: 0.05, every: 4.0 }
    }
}

#[derive(Resource, Clone)]
pub struct KaleidoParams {
    /// Flight speed over the field.
    pub speed: f32,
    /// Extra flight speed at the kick, decaying over the beat.
    pub surge: f32,
    /// Scale of the fold field on the plane.
    pub zoom: f32,
    /// Centre breathing amount.
    pub breathe: f32,
    /// Fold line sharpness: higher is thinner lines and more black.
    pub sharp: f32,
    /// Brightness falloff into the distance.
    pub fade: f32,
    /// Fold count: fewer is sparser and bolder.
    pub iters: f32,
    /// Beats per hit: the surge fires every this many beats.
    pub every: f32,
}

impl Default for KaleidoParams {
    fn default() -> Self {
        Self {
            speed: 0.4,
            surge: 2.0,
            zoom: 0.4,
            breathe: 0.04,
            sharp: 6.0,
            fade: 0.5,
            iters: 5.0,
            every: 4.0,
        }
    }
}

#[derive(Resource, Clone)]
pub struct RingsParams {
    /// How far a ring reaches before it dies (the frame is 1 tall).
    pub reach: f32,
    /// Ring thickness.
    pub width: f32,
    /// Beats a ring lives.
    pub life: f32,
    /// The core flash on each beat.
    pub glow: f32,
    /// Launch ease-out exponent: higher bolts off the centre harder.
    pub snap: f32,
    /// Per-ring wobble amount.
    pub wob: f32,
    /// How hard the kick blows the whole field up.
    pub pump: f32,
    /// Beats per hit: launches, jerks and pumps every this many beats.
    pub every: f32,
}

impl Default for RingsParams {
    fn default() -> Self {
        Self {
            reach: 1.3,
            width: 0.05,
            life: 3.0,
            glow: 0.8,
            snap: 3.0,
            wob: 0.15,
            pump: 0.2,
            every: 1.0,
        }
    }
}

#[derive(Resource, Clone)]
pub struct BoxesParams {
    /// Zoom, box periods per second.
    pub speed: f32,
    /// Extra zoom at the kick, decaying over the beat.
    pub surge: f32,
    /// Corkscrew per depth unit.
    pub twist: f32,
    /// Boxes per depth unit.
    pub density: f32,
    /// Line thickness, fraction of the gap between boxes.
    pub thick: f32,
    /// Radius the deep end fades out below, before it gets too dense.
    pub cutoff: f32,
    /// Beats per hit: the surge fires every this many beats.
    pub every: f32,
}

impl Default for BoxesParams {
    fn default() -> Self {
        Self {
            speed: 0.5,
            surge: 1.5,
            twist: 0.4,
            density: 3.0,
            thick: 0.1,
            cutoff: 0.15,
            every: 8.0,
        }
    }
}

/// Material handles plus the integrated phases.
#[derive(Resource)]
pub struct Flat {
    spiral: Handle<SpiralMaterial>,
    wormhole: Handle<WormholeMaterial>,
    kaleido: Handle<KaleidoMaterial>,
    rings: Handle<RingsMaterial>,
    boxes: Handle<BoxesMaterial>,
    spiral_t: f32,
    wormhole_t: f32,
    kaleido_t: f32,
    boxes_t: f32,
}

/// One pattern's quad and camera on its own layer, into the feed.
fn lane<M: Material>(
    cmds: &mut Commands,
    quad: &Handle<Mesh>,
    material: Handle<M>,
    layer: usize,
    feed: &Handle<Image>,
    pattern: Pattern,
) {
    cmds.spawn((
        Mesh3d(quad.clone()),
        MeshMaterial3d(material),
        RenderLayers::layer(layer),
        NotShadowCaster,
    ));
    cmds.spawn((
        Camera3d::default(),
        Camera {
            clear_color: ClearColorConfig::Custom(Color::BLACK),
            order: -3,
            is_active: false,
            ..default()
        },
        RenderTarget::Image(feed.clone().into()),
        Projection::from(OrthographicProjection {
            scaling_mode: ScalingMode::Fixed { width: PX.x as f32, height: PX.y as f32 },
            ..OrthographicProjection::default_3d()
        }),
        Transform::from_xyz(0.0, 0.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
        RenderLayers::layer(layer),
        PatternCam(pattern),
    ));
}

pub(super) fn setup(
    mut cmds: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    screen: Res<Screen>,
    mut spirals: ResMut<Assets<SpiralMaterial>>,
    mut wormholes: ResMut<Assets<WormholeMaterial>>,
    mut kaleidos: ResMut<Assets<KaleidoMaterial>>,
    mut rings: ResMut<Assets<RingsMaterial>>,
    mut boxes: ResMut<Assets<BoxesMaterial>>,
) {
    let quad =
        meshes.add(Plane3d::new(Vec3::Z, Vec2::new(PX.x as f32 / 2.0, PX.y as f32 / 2.0)));
    let spiral = spirals.add(SpiralMaterial {
        clock: Vec4::ZERO,
        color0: LinearRgba::WHITE,
        color1: LinearRgba::BLACK,
        params: Vec4::ZERO,
    });
    let wormhole = wormholes.add(WormholeMaterial {
        clock: Vec4::ZERO,
        color0: LinearRgba::WHITE,
        color1: LinearRgba::BLACK,
        params: Vec4::ZERO,
    });
    let kaleido = kaleidos.add(KaleidoMaterial {
        clock: Vec4::ZERO,
        color0: LinearRgba::WHITE,
        color1: LinearRgba::BLACK,
        params: Vec4::ZERO,
    });
    let ring = rings.add(RingsMaterial {
        clock: Vec4::ZERO,
        color0: LinearRgba::WHITE,
        color1: LinearRgba::BLACK,
        params: Vec4::ZERO,
    });
    let boxs = boxes.add(BoxesMaterial {
        clock: Vec4::ZERO,
        color0: LinearRgba::WHITE,
        color1: LinearRgba::BLACK,
        params: Vec4::ZERO,
    });

    lane(&mut cmds, &quad, spiral.clone(), LAYER_SPIRAL, &screen.feed, Pattern::Spiral);
    lane(&mut cmds, &quad, wormhole.clone(), LAYER_WORMHOLE, &screen.feed, Pattern::Wormhole);
    lane(&mut cmds, &quad, kaleido.clone(), LAYER_KALEIDO, &screen.feed, Pattern::Kaleido);
    lane(&mut cmds, &quad, ring.clone(), LAYER_RINGS, &screen.feed, Pattern::Rings);
    lane(&mut cmds, &quad, boxs.clone(), LAYER_BOXES, &screen.feed, Pattern::Boxes);

    cmds.insert_resource(Flat {
        spiral,
        wormhole,
        kaleido,
        rings: ring,
        boxes: boxs,
        spiral_t: 0.0,
        wormhole_t: 0.0,
        kaleido_t: 0.0,
        boxes_t: 0.0,
    });
}

/// Advance and feed whichever of these is on the wall.
pub(super) fn drive(
    mut flat: ResMut<Flat>,
    screen: Res<Screen>,
    s: Res<State>,
    beats: Res<Beats>,
    time: Res<Time>,
    params: (
        Res<SpiralParams>,
        Res<WormholeParams>,
        Res<KaleidoParams>,
        Res<RingsParams>,
        Res<BoxesParams>,
    ),
    materials: (
        ResMut<Assets<SpiralMaterial>>,
        ResMut<Assets<WormholeMaterial>>,
        ResMut<Assets<KaleidoMaterial>>,
        ResMut<Assets<RingsMaterial>>,
        ResMut<Assets<BoxesMaterial>>,
    ),
) {
    let (spiral, wormhole, kaleido, rings, boxes) = params;
    let (mut spirals, mut wormholes, mut kaleidos, mut ring_mats, mut boxes_mats) = materials;
    let dt = time.delta_secs();
    // Each pattern divides the beat clock: a hit every `every` beats, the
    // decay stretched across the whole gap, so slow presets get a big kick
    // and a long coast.
    let hit = |every: f32| {
        let bp = (beats.0 / every.max(1.0)).fract();
        (bp, (1.0 - bp).powi(3))
    };
    let color0 = to_linear(s.palette.beam_color(&s));
    let color1 = to_linear(s.palette.par_color(&s));

    match screen.pattern {
        Pattern::Spiral => {
            let (bp, _) = hit(spiral.every);
            flat.spiral_t += dt * spiral.speed;
            let Some(mut m) = spirals.get_mut(&flat.spiral) else { return };
            m.clock = Vec4::new(flat.spiral_t, beats.0, spiral.amount + spiral.pulse * (1.0 - bp), 0.0);
            m.color0 = color0;
            m.color1 = color1;
            m.params = Vec4::new(
                spiral.swirl,
                (spiral.spokes * 0.5).round().max(1.0) * 2.0,
                spiral.cutoff,
                1.0 - spiral.width.clamp(0.05, 0.95),
            );
        }
        Pattern::Wormhole => {
            let (_, rush) = hit(wormhole.every);
            flat.wormhole_t += dt * wormhole.speed * (1.0 + wormhole.surge * rush);
            let Some(mut m) = wormholes.get_mut(&flat.wormhole) else { return };
            m.clock = Vec4::new(flat.wormhole_t, beats.0, 0.0, 0.0);
            m.color0 = color0;
            m.color1 = color1;
            m.params = Vec4::new(wormhole.warp + wormhole.pulse * rush, 0.0, 0.0, 0.0);
        }
        Pattern::Kaleido => {
            let (_, rush) = hit(kaleido.every);
            flat.kaleido_t += dt * kaleido.speed * (1.0 + kaleido.surge * rush);
            let Some(mut m) = kaleidos.get_mut(&flat.kaleido) else { return };
            m.clock = Vec4::new(flat.kaleido_t, beats.0, kaleido.iters.round(), 0.0);
            m.color0 = color0;
            m.color1 = color1;
            m.params = Vec4::new(kaleido.zoom, kaleido.breathe, kaleido.sharp, kaleido.fade);
        }
        Pattern::Rings => {
            let (_, rush) = hit(rings.every);
            let Some(mut m) = ring_mats.get_mut(&flat.rings) else { return };
            m.clock = Vec4::new(
                beats.0 / rings.every.max(1.0),
                rings.snap.max(1.0),
                rings.wob,
                rings.pump * rush,
            );
            m.color0 = color0;
            m.color1 = color1;
            m.params = Vec4::new(rings.reach, rings.width, rings.life.max(0.25), rings.glow);
        }
        Pattern::Boxes => {
            let (_, rush) = hit(boxes.every);
            flat.boxes_t += dt * boxes.speed * (1.0 + boxes.surge * rush);
            let Some(mut m) = boxes_mats.get_mut(&flat.boxes) else { return };
            m.clock = Vec4::new(flat.boxes_t, beats.0, 0.0, 0.0);
            m.color0 = color0;
            m.color1 = color1;
            m.params = Vec4::new(boxes.twist, boxes.density, boxes.thick, boxes.cutoff);
        }
        _ => {}
    }
}
