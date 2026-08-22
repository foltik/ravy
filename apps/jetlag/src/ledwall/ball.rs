//! The uvbounce ball from milstrike7's reality intro, floorless and
//! unshaded: a checkered sphere spinning about its poles, spun up by every
//! beat and coasting between them like cyber_grind's ships, the camera
//! cutting between angles every bar.

use bevy::camera::visibility::RenderLayers;
use bevy::camera::{Projection, RenderTarget};
use bevy::light::NotShadowCaster;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;
use lib::prelude::*;

use super::{Beats, LAYER_BALL, Pattern, PatternCam, Screen, to_linear};
use crate::logic::State;

#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct BallMaterial {
    #[uniform(0)]
    pub color0: LinearRgba,
    #[uniform(1)]
    pub color1: LinearRgba,
    /// x: checker columns around the equator.
    #[uniform(2)]
    pub params: Vec4,
}

impl Material for BallMaterial {
    fn fragment_shader() -> ShaderRef {
        "ledwall/ball.wgsl".into()
    }
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Opaque
    }
}

#[derive(Resource, Clone)]
pub struct BallParams {
    /// Ball diameter, metres-ish in the scene.
    pub size: f32,
    /// Checker columns around the equator.
    pub checks: f32,
    /// Spin floor, radians per second.
    pub spin: f32,
    /// Spin each hit adds.
    pub boost: f32,
    /// Spin shed per second coasting back to the floor.
    pub brake: f32,
    /// Beats per hit: the boost fires every this many beats.
    pub every: f32,
}

impl Default for BallParams {
    fn default() -> Self {
        Self { size: 1.0, checks: 8.0, spin: 3.0, boost: 8.0, brake: 20.0, every: 1.0 }
    }
}

#[derive(Resource)]
pub struct BallScene {
    ball: Entity,
    cam: Entity,
    material: Handle<BallMaterial>,
    vel: f32,
    rot: f32,
    last: f32,
}

/// Camera perches the bar cycles through as (position, up), all aimed at the
/// ball; the tilted ups roll the frame dutch. [`animate`] sways around them.
const SHOTS: [(Vec3, Vec3); 5] = [
    // Close side, dutch.
    (Vec3::new(0.9, 0.5, 4.0), Vec3::new(-0.3, 1.0, 0.0)),
    // Straight down onto the pole.
    (Vec3::new(0.3, 4.2, 1.2), Vec3::Z),
    // Worm's eye, close and rolled the other way.
    (Vec3::new(-1.5, -2.5, 3.5), Vec3::new(0.4, 1.0, 0.0)),
    // Far and small, floating in the dark.
    (Vec3::new(0.0, 0.5, 14.0), Vec3::Y),
    // Hard side sweep, heavy roll.
    (Vec3::new(4.5, 0.3, 5.0), Vec3::new(1.0, 0.6, 0.0)),
];

pub(super) fn setup(
    mut cmds: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<BallMaterial>>,
    screen: Res<Screen>,
) {
    let material = materials.add(BallMaterial {
        color0: LinearRgba::WHITE,
        color1: LinearRgba::BLACK,
        params: Vec4::new(BallParams::default().checks, 0.0, 0.0, 0.0),
    });
    let ball = cmds
        .spawn((
            Mesh3d(meshes.add(Sphere::new(0.5).mesh().uv(48, 24))),
            MeshMaterial3d(material.clone()),
            RenderLayers::layer(LAYER_BALL),
            NotShadowCaster,
        ))
        .id();
    let cam = cmds
        .spawn((
            Camera3d::default(),
            Camera {
                clear_color: ClearColorConfig::Custom(Color::BLACK),
                order: -3,
                is_active: false,
                ..default()
            },
            RenderTarget::Image(screen.feed.clone().into()),
            // The uvbounce blend's narrow lens.
            Projection::Perspective(PerspectiveProjection { fov: 0.3996, ..default() }),
            Transform::from_translation(SHOTS[0].0).looking_at(Vec3::ZERO, SHOTS[0].1),
            RenderLayers::layer(LAYER_BALL),
            PatternCam(Pattern::Ball),
        ))
        .id();
    cmds.insert_resource(BallScene { ball, cam, material, vel: 0.0, rot: 0.0, last: 0.0 });
}

pub(super) fn animate(
    mut scene: ResMut<BallScene>,
    params: Res<BallParams>,
    s: Res<State>,
    beats: Res<Beats>,
    time: Res<Time>,
    mut materials: ResMut<Assets<BallMaterial>>,
    mut transforms: Query<&mut Transform>,
) {
    let dt = time.delta_secs();
    let hits = (beats.0 / params.every.max(1.0)).floor();
    if hits > scene.last {
        scene.vel += params.boost;
    }
    scene.last = hits;
    scene.vel = (scene.vel - params.brake * dt).max(params.spin);
    scene.rot += scene.vel * dt;

    let t = s.t;
    if let Ok(mut transform) = transforms.get_mut(scene.cam) {
        let (perch, up) = SHOTS[(beats.0 / 16.0) as usize % SHOTS.len()];
        // Drift around the perch on incommensurate sines, the aim wandering
        // off the ball's centre, a slow roll through the up vector.
        let sway = Vec3::new(
            (0.31 * t).sin() + 0.5 * (0.83 * t + 1.7).sin(),
            0.8 * (0.47 * t + 4.0).sin(),
            (0.23 * t + 0.9).sin(),
        ) * 0.4;
        let aim = Vec3::new((0.27 * t + 1.1).sin(), (0.37 * t).sin(), 0.0) * 0.5;
        let roll = up + Vec3::X * 0.12 * (0.19 * t + 2.0).sin();
        *transform = Transform::from_translation(perch + sway).looking_at(aim, roll);
    }

    if let Ok(mut transform) = transforms.get_mut(scene.ball) {
        // The mesh's poles sit on +-z; stand them up, spin about them, and
        // precess the whole axis so the spin keeps changing character.
        transform.rotation = Quat::from_rotation_z(0.45 * (0.21 * t).sin())
            * Quat::from_rotation_x(0.35 + 0.3 * (0.34 * t + 1.2).sin())
            * Quat::from_rotation_y(scene.rot)
            * Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
        transform.scale = Vec3::splat(params.size.max(0.05));
    }

    if let Some(mut material) = materials.get_mut(&scene.material) {
        let beam = to_linear(s.palette.beam_color(&s));
        let par = to_linear(s.palette.par_color(&s));
        material.color0 = beam;
        // The par colour folds its white channel into rgb, so it often lands
        // near-white and washes the checker out. Unless it is genuinely its
        // own saturated colour, checker against black.
        let peak = par.red.max(par.green).max(par.blue);
        let sat = (peak - par.red.min(par.green).min(par.blue)) / peak.max(1e-4);
        material.color1 = match par != beam && sat > 0.35 {
            true => par,
            false => LinearRgba::BLACK,
        };
        material.params.x = params.checks.round().max(2.0);
    }
}
