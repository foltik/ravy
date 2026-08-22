//! The frame chain: pattern camera -> feed image -> fx camera -> frame image.

use bevy::app::HierarchyPropagatePlugin;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{Projection, RenderTarget, ScalingMode};
use bevy::image::ImageSampler;
use bevy::light::NotShadowCaster;
use bevy::pbr::ExtendedMaterial;
use bevy::render::render_resource::{
    AsBindGroup, Extent3d, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages,
};
use bevy::shader::ShaderRef;
use lib::prelude::*;

use super::{Fx, FxMaterial, LedWall, LedWallExtension, LedWallMaterial};
use crate::logic::State;

/// The wall, in pixels. Its physical 1.5x1.0m face is authored in the blend.
pub const COLS: u32 = 3;
pub const ROWS: u32 = 2;
pub const MODULE_PX: u32 = 128;
pub const MODULE_M: f32 = 0.5;
pub const PX: UVec2 = UVec2::new(COLS * MODULE_PX, ROWS * MODULE_PX);

/// The isolines pattern quad and its camera.
pub(super) const LAYER_PATTERN: usize = 30;
/// The upscaled copy shown in the output window.
pub(super) const LAYER_OUTPUT: usize = 31;
/// The cuberoom scene and its camera.
pub(super) const LAYER_SCENE: usize = 32;
/// The fx quad and its camera.
pub(super) const LAYER_FX: usize = 33;
/// The fullscreen shader patterns, one lane each.
pub(super) const LAYER_SPIRAL: usize = 34;
pub(super) const LAYER_WORMHOLE: usize = 35;
pub(super) const LAYER_KALEIDO: usize = 36;
pub(super) const LAYER_RINGS: usize = 37;
pub(super) const LAYER_BOXES: usize = 38;
/// The spinning ball scene and its camera.
pub(super) const LAYER_BALL: usize = 39;

/// The wall's one frame, shared by the sim mesh and the output window.
#[derive(Resource)]
pub struct Screen {
    /// The selected pattern's raw output, input to the fx pass.
    pub feed: Handle<Image>,
    /// The finished frame: anything that wants the wall's pixels reads this.
    #[allow(unused)]
    pub frame: Handle<Image>,
    pub material: Handle<ScreenMaterial>,
    pub fx: Handle<FxMaterial>,
    /// What the model's `Screen` mesh wears: the frame behind an LED grid.
    pub wall: Handle<LedWallMaterial>,
    /// Which pattern's camera feeds the wall.
    pub pattern: Pattern,
    /// The popped-out output window and its camera, while open.
    pub output: Option<(Entity, Entity)>,
}

/// Content the wall can show, each behind its own camera into the feed.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Pattern {
    Isolines,
    CubeRoom,
    Spiral,
    Ball,
    Wormhole,
    Kaleido,
    Rings,
    Boxes,
}

/// Which pattern's camera this is; [`select`] activates the chosen one.
#[derive(Component)]
pub struct PatternCam(pub Pattern);

/// Beats since the show started, unwrapped from the 64-beat phase: shader
/// and animation maths would pop on the wrap otherwise.
#[derive(Resource, Default)]
pub struct Beats(pub f32);

/// The isolines shader, fed from the show every frame.
#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct ScreenMaterial {
    /// x: seconds since start, y: beats since start (unwrapped), z: bpm,
    /// w: unused (brightness is applied in the fx pass).
    #[uniform(0)]
    pub clock: Vec4,
    /// The palette's mover colour.
    #[uniform(1)]
    pub color0: LinearRgba,
    /// The palette's par colour.
    #[uniform(2)]
    pub color1: LinearRgba,
    /// x: drift units/s, y: kick per bar, z: kick ease exponent, w: horizon
    /// sway speed.
    #[uniform(3)]
    pub motion: Vec4,
    /// x: camera height, y: distance fade, z: isoline count, w: baseline
    /// line thickness.
    #[uniform(4)]
    pub look: Vec4,
    /// x: thickness the kick adds, riding its velocity envelope, y: horizon
    /// sway range, z: fraction of the bar the kick takes to coast in.
    #[uniform(5)]
    pub accent: Vec4,
}

/// The isolines pattern's dials, mirrored into [`ScreenMaterial`] every frame.
#[derive(Resource, Clone)]
pub struct ScreenParams {
    /// Baseline scroll speed, floor units per second.
    pub drift: f32,
    /// Scroll travel each bar's kick adds.
    pub kick: f32,
    /// Kick ease-out exponent: higher snaps harder and coasts longer.
    pub snap: f32,
    /// Horizon sway speed, radians per second.
    pub wave: f32,
    /// Camera height over the floor.
    pub cam: f32,
    /// Brightness falloff into the distance.
    pub fade: f32,
    /// Isoline count across the field.
    pub lines: f32,
    /// Baseline line thickness.
    pub thick: f32,
    /// Thickness the kick adds while it's fast, fading as it coasts.
    pub pump: f32,
    /// Horizon sway range: how far the look-up/down wave swings.
    pub sway: f32,
    /// Fraction of the bar the kick takes to settle back to the drift.
    pub settle: f32,
}

impl Default for ScreenParams {
    fn default() -> Self {
        Self {
            drift: 0.08,
            kick: 1.0,
            snap: 4.0,
            wave: 0.18,
            cam: 2.0,
            fade: 0.45,
            lines: 14.0,
            thick: 0.72,
            pump: 0.18,
            sway: 0.3,
            settle: 0.75,
        }
    }
}

impl ScreenParams {
    fn motion(&self) -> Vec4 {
        Vec4::new(self.drift, self.kick, self.snap, self.wave)
    }
    fn look(&self) -> Vec4 {
        Vec4::new(self.cam, self.fade, self.lines, self.thick)
    }
    fn accent(&self) -> Vec4 {
        Vec4::new(self.pump, self.sway, self.settle.clamp(0.05, 1.0), 0.0)
    }
}

impl Material for ScreenMaterial {
    fn fragment_shader() -> ShaderRef {
        "ledwall/isolines.wgsl".into()
    }
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Opaque
    }
}

pub struct LedwallPlugin;

impl Plugin for LedwallPlugin {
    fn build(&self, app: &mut App) {
        // The cuberoom scene root wears Propagate for these; everything the
        // spawner puts under it inherits them.
        app.add_plugins(HierarchyPropagatePlugin::<RenderLayers>::new(PostUpdate))
            .add_plugins(HierarchyPropagatePlugin::<NotShadowCaster>::new(PostUpdate))
            .add_plugins(MaterialPlugin::<ScreenMaterial>::default())
            .add_plugins(MaterialPlugin::<FxMaterial>::default())
            .add_plugins(MaterialPlugin::<super::CubeRoomMaterial>::default())
            .add_plugins((
                MaterialPlugin::<super::SpiralMaterial>::default(),
                MaterialPlugin::<super::WormholeMaterial>::default(),
                MaterialPlugin::<super::KaleidoMaterial>::default(),
                MaterialPlugin::<super::RingsMaterial>::default(),
                MaterialPlugin::<super::BoxesMaterial>::default(),
                MaterialPlugin::<super::BallMaterial>::default(),
            ))
            .add_plugins(MaterialPlugin::<LedWallMaterial>::default())
            .register_type::<LedWall>()
            .init_resource::<Beats>()
            .init_resource::<ScreenParams>()
            .init_resource::<Fx>()
            .init_resource::<super::CubeParams>()
            .init_resource::<super::SpiralParams>()
            .init_resource::<super::WormholeParams>()
            .init_resource::<super::KaleidoParams>()
            .init_resource::<super::RingsParams>()
            .init_resource::<super::BoxesParams>()
            .init_resource::<super::BallParams>()
            .add_systems(
                Startup,
                (setup, super::cuberoom::setup, super::flat::setup, super::ball::setup)
                    .chain(),
            )
            .add_systems(
                Update,
                (
                    beats,
                    (
                        drive,
                        super::fx::drive,
                        super::cuberoom::animate,
                        super::flat::drive,
                        super::ball::animate,
                    ),
                )
                    .chain(),
            )
            .add_systems(
                Update,
                (
                    select,
                    super::cuberoom::prepare,
                    super::wall::tune,
                    super::output::fullscreen,
                    super::output::closed,
                ),
            );
    }
}

/// One 384x256 offscreen image; nearest keeps display crisp, linear lets the
/// fx pass shift by fractions of a pixel.
fn target(label: &'static str, sampler: ImageSampler) -> Image {
    let mut image = Image {
        texture_descriptor: TextureDescriptor {
            label: Some(label),
            size: Extent3d { width: PX.x, height: PX.y, depth_or_array_layers: 1 },
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8UnormSrgb,
            mip_level_count: 1,
            sample_count: 1,
            usage: TextureUsages::RENDER_ATTACHMENT
                | TextureUsages::TEXTURE_BINDING
                | TextureUsages::COPY_DST,
            view_formats: &[],
        },
        sampler,
        ..default()
    };
    image.resize(Extent3d { width: PX.x, height: PX.y, depth_or_array_layers: 1 });
    image
}

fn setup(
    mut cmds: Commands,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut screen_materials: ResMut<Assets<ScreenMaterial>>,
    mut fx_materials: ResMut<Assets<FxMaterial>>,
    mut wall_materials: ResMut<Assets<LedWallMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let feed = images.add(target("screen feed", ImageSampler::linear()));
    let frame = images.add(target("screen frame", ImageSampler::nearest()));
    let quad = meshes.add(Plane3d::new(Vec3::Z, Vec2::new(PX.x as f32 / 2.0, PX.y as f32 / 2.0)));

    // The isolines pass: an ortho camera staring at a quad running the shader.
    let params = ScreenParams::default();
    let material = screen_materials.add(ScreenMaterial {
        clock: Vec4::ZERO,
        color0: LinearRgba::WHITE,
        color1: LinearRgba::WHITE,
        motion: params.motion(),
        look: params.look(),
        accent: params.accent(),
    });
    // The offscreen quads are hundreds of metres wide and sit at the world
    // origin; NotShadowCaster keeps them out of the beam pass's acceleration
    // structure, where they would wall off the venue despite being on hidden
    // render layers.
    cmds.spawn((
        Mesh3d(quad.clone()),
        MeshMaterial3d(material.clone()),
        RenderLayers::layer(LAYER_PATTERN),
        NotShadowCaster,
    ));
    cmds.spawn((
        Camera3d::default(),
        Camera {
            clear_color: ClearColorConfig::Custom(Color::BLACK),
            order: -3,
            ..default()
        },
        RenderTarget::Image(feed.clone().into()),
        Projection::from(OrthographicProjection {
            scaling_mode: ScalingMode::Fixed { width: PX.x as f32, height: PX.y as f32 },
            ..OrthographicProjection::default_3d()
        }),
        Transform::from_xyz(0.0, 0.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
        RenderLayers::layer(LAYER_PATTERN),
        PatternCam(Pattern::Isolines),
    ));

    // The fx pass: the milstrike filter chain over the feed, into the frame.
    let fx = fx_materials.add(FxMaterial {
        clock: Vec4::ZERO,
        warp: Vec4::ZERO,
        grade: Vec4::new(0.0, 0.0, 0.0, 1.0),
        feed: feed.clone(),
    });
    cmds.spawn((
        Mesh3d(quad.clone()),
        MeshMaterial3d(fx.clone()),
        RenderLayers::layer(LAYER_FX),
        NotShadowCaster,
    ));
    cmds.spawn((
        Camera3d::default(),
        Camera {
            clear_color: ClearColorConfig::Custom(Color::BLACK),
            order: -2,
            ..default()
        },
        RenderTarget::Image(frame.clone().into()),
        Projection::from(OrthographicProjection {
            scaling_mode: ScalingMode::Fixed { width: PX.x as f32, height: PX.y as f32 },
            ..OrthographicProjection::default_3d()
        }),
        Transform::from_xyz(0.0, 0.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
        RenderLayers::layer(LAYER_FX),
    ));

    // What the model's `Screen` mesh wears, applied by `sim::patch`.
    let wall = wall_materials.add(ExtendedMaterial {
        base: StandardMaterial {
            base_color: Color::BLACK,
            perceptual_roughness: 1.0,
            ..default()
        },
        extension: LedWallExtension::new(&LedWall::default(), frame.clone()),
    });

    // The upscaled copy for the output window, waiting on `popout`'s camera:
    // the render target on an unlit quad.
    cmds.spawn((
        Mesh3d(quad),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::WHITE,
            base_color_texture: Some(frame.clone()),
            unlit: true,
            ..default()
        })),
        RenderLayers::layer(LAYER_OUTPUT),
        NotShadowCaster,
    ));

    cmds.insert_resource(Screen {
        feed,
        frame,
        material,
        fx,
        wall,
        pattern: Pattern::Isolines,
        output: None,
    });
}

/// Unwrap the show's beat phase into a running count. While a manual beat is
/// engaged it takes the clock over: each press on either side is the next
/// beat, and nothing ticks over between presses.
fn beats(
    s: Res<State>,
    time: Res<Time>,
    mut beats: ResMut<Beats>,
    mut wrap: Local<(f32, f32)>,
    mut taps: Local<(f32, f32)>,
) {
    if let Some(beat) = s.beat {
        if (beat.t0, beat.t1) != *taps {
            *taps = (beat.t0, beat.t1);
            beats.0 = beats.0.floor() + 1.0;
        } else {
            // The fraction still runs at the clock's rate for the decays,
            // but parks just short of rolling a beat on its own.
            let bp = beats.0.fract() + (s.bpm / 60.0) * s.phi_mul * time.delta_secs();
            beats.0 = beats.0.floor() + bp.min(0.999);
        }
        // Re-anchor the unwrap so handing back to the clock is seamless.
        *wrap = (s.phi, beats.0 - s.phi);
        return;
    }
    let (last, mut laps) = *wrap;
    if s.phi + 1.0 < last {
        laps += 64.0;
    }
    *wrap = (s.phi, laps);
    beats.0 = laps + s.phi;
}

/// Activate the selected pattern's camera; the others go dark.
fn select(screen: Res<Screen>, mut cams: Query<(&PatternCam, &mut Camera)>) {
    for (cam, mut camera) in &mut cams {
        let active = cam.0 == screen.pattern;
        if camera.is_active != active {
            camera.is_active = active;
        }
    }
}

/// Feed the isolines shader the show.
fn drive(
    screen: Res<Screen>,
    s: Res<State>,
    beats: Res<Beats>,
    params: Res<ScreenParams>,
    mut materials: ResMut<Assets<ScreenMaterial>>,
) {
    let Some(mut material) = materials.get_mut(&screen.material) else { return };
    material.clock = Vec4::new(s.t, beats.0, s.bpm, 1.0);
    material.color0 = to_linear(s.palette.beam_color(&s));
    material.color1 = to_linear(s.palette.par_color(&s));
    material.motion = params.motion();
    material.look = params.look();
    material.accent = params.accent();
}

/// The wall shows the palette's numbers as-is: treated as linear they land
/// brighter on the sRGB frame and every midtone washes toward white.
pub(super) fn to_linear(color: Rgbw) -> LinearRgba {
    let Rgb(r, g, b) = color.into();
    bevy::color::Srgba::rgb(r, g, b).into()
}
