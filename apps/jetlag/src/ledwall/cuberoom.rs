//! The funky_beat cube from milstrike7: the glb's CubeAction tumble on
//! black, the room's walls hidden, the camera snapping between the blend's
//! presets every bar, the cube left flat for the fx pass's edge extraction.
//! No lights exist, so nothing can spill into any other view.

use bevy::animation::AnimationPlayer;
use bevy::animation::graph::{AnimationGraph, AnimationGraphHandle, AnimationNodeIndex};
use bevy::app::Propagate;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{Projection, RenderTarget};
use bevy::gltf::{Gltf, GltfAssetLabel, GltfLoaderSettings, GltfMaterialName};
use bevy::light::NotShadowCaster;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;
use bevy::world_serialization::WorldAssetRoot;
use lib::prelude::*;

use super::{Beats, FxMaterial, LAYER_SCENE, Pattern, PatternCam, Screen, to_linear};
use crate::logic::State;

/// How many camera presets the bar cycles through; funky_beat used
/// Camera.000..004 of the nine in the blend.
const SHOTS: usize = 5;

/// The cube's surface, unshaded.
#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct CubeRoomMaterial {
    #[uniform(0)]
    pub color0: LinearRgba,
    #[uniform(1)]
    pub color1: LinearRgba,
    /// x: wire width, fraction of the cube's half extent.
    #[uniform(2)]
    pub params: Vec4,
}

impl Material for CubeRoomMaterial {
    fn fragment_shader() -> ShaderRef {
        "ledwall/cuberoom.wgsl".into()
    }
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Opaque
    }
}

/// The scene's pieces, collected by [`prepare`] once the glb has spawned.
#[derive(Resource)]
pub struct CubeRoom {
    gltf: Handle<Gltf>,
    root: Entity,
    cam: Entity,
    /// Camera presets the bar cycles through.
    shots: Vec<Transform>,
    /// The animated cube node, wearing the AnimationPlayer.
    cube: Option<Entity>,
    anim: Option<AnimationNodeIndex>,
    cube_mat: Handle<CubeRoomMaterial>,
    ready: bool,
}

#[derive(Resource, Clone)]
pub struct CubeParams {
    /// Base tumble speed, animation seconds per second.
    pub spin: f32,
    /// Extra tumble speed at the kick, decaying over the beat.
    pub surge: f32,
    /// Cube growth on the kick, on its 0.5 base scale.
    pub pump: f32,
    /// Mega-freakout burst as the camera jumps, dead within the beat.
    pub mega: f32,
    /// Wireframe width in the second colour, fraction of the half extent.
    pub wire: f32,
}

impl Default for CubeParams {
    fn default() -> Self {
        Self { spin: 0.5, surge: 6.0, pump: 0.05, mega: 0.35, wire: 0.1 }
    }
}

pub(super) fn setup(
    mut cmds: Commands,
    assets: Res<AssetServer>,
    mut materials: ResMut<Assets<CubeRoomMaterial>>,
    screen: Res<Screen>,
) {
    // The blend's cameras are wanted as transforms only (loaded live they
    // grab the primary window), and its lights are unused: the materials are
    // unlit. Every load of the file must carry the same settings: the asset
    // is keyed by path, so the first load's settings win.
    let settings = |s: &mut GltfLoaderSettings| {
        s.load_cameras = false;
        s.load_lights = false;
    };
    let gltf = assets.load_builder().with_settings(settings).load("ledwall/cuberoom.glb");
    let scene = assets
        .load_builder()
        .with_settings(settings)
        .load(GltfAssetLabel::Scene(0).from_asset("ledwall/cuberoom.glb"));
    // Everything the scene spawns, now or on a respawn, inherits the layer
    // and stays out of the beam pass; one-shot patching missed late spawns.
    let root = cmds
        .spawn((
            WorldAssetRoot(scene),
            Visibility::Hidden,
            Propagate(RenderLayers::layer(LAYER_SCENE)),
            Propagate(NotShadowCaster),
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
            // The blend's camera lens.
            Projection::Perspective(PerspectiveProjection {
                fov: 0.3996,
                ..default()
            }),
            RenderLayers::layer(LAYER_SCENE),
            PatternCam(Pattern::CubeRoom),
        ))
        .id();
    let cube_mat = materials.add(CubeRoomMaterial {
        color0: LinearRgba::WHITE,
        color1: LinearRgba::BLACK,
        params: Vec4::ZERO,
    });
    cmds.insert_resource(CubeRoom {
        gltf,
        root,
        cam,
        shots: vec![],
        cube: None,
        anim: None,
        cube_mat,
        ready: false,
    });
}

/// Dress whatever the world spawner produced: strip stray cameras, hide the
/// Room, swap the Cube onto our material. Runs idempotently forever so a
/// respawned scene is re-dressed; the one-time collection of camera presets
/// and the tumble start gate on `ready`.
pub(super) fn prepare(
    mut cuberoom: ResMut<CubeRoom>,
    mut cmds: Commands,
    gltfs: Res<Assets<Gltf>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    children: Query<&Children>,
    names: Query<&Name>,
    transforms: Query<&Transform>,
    material_names: Query<(&GltfMaterialName, Has<MeshMaterial3d<StandardMaterial>>)>,
    cameras: Query<(), With<Camera>>,
    mut players: Query<&mut AnimationPlayer>,
) {
    let Some(gltf) = gltfs.get(&cuberoom.gltf) else { return };

    let mut shots: Vec<(String, Transform)> = vec![];
    let mut cube = None;
    for entity in children.iter_descendants(cuberoom.root) {
        // A camera that slipped through would fight the sim for the window.
        if cameras.contains(entity) {
            cmds.entity(entity).remove::<(Camera3d, Camera)>();
        }
        if let Ok((name, standard)) = material_names.get(entity)
            && standard
        {
            match name.0.as_str() {
                // The walls are gone: just the cube tumbling on black. The
                // material removal also stops this branch re-running.
                "Room" => {
                    cmds.entity(entity)
                        .remove::<MeshMaterial3d<StandardMaterial>>()
                        .insert(Visibility::Hidden);
                }
                "Cube" => {
                    cmds.entity(entity)
                        .remove::<MeshMaterial3d<StandardMaterial>>()
                        .insert(MeshMaterial3d(cuberoom.cube_mat.clone()));
                }
                _ => {}
            }
        }
        if cuberoom.ready {
            continue;
        }
        if let Ok(name) = names.get(entity)
            && name.as_str().starts_with("Camera.")
            && let Ok(transform) = transforms.get(entity)
        {
            shots.push((name.to_string(), *transform));
        }
        if players.contains(entity) {
            cube = Some(entity);
        }
    }

    if cuberoom.ready {
        return;
    }
    let Some(cube) = cube else { return };
    if shots.is_empty() {
        return;
    }
    shots.sort_by(|a, b| a.0.cmp(&b.0));
    cuberoom.shots = shots.into_iter().take(SHOTS).map(|(_, t)| t).collect();
    cuberoom.cube = Some(cube);

    if let Some(clip) = gltf.named_animations.get("CubeAction") {
        let (graph, index) = AnimationGraph::from_clip(clip.clone());
        cmds.entity(cube).insert(AnimationGraphHandle(graphs.add(graph)));
        if let Ok(mut player) = players.get_mut(cube) {
            player.play(index).repeat();
        }
        cuberoom.anim = Some(index);
    }

    cmds.entity(cuberoom.root).insert(Visibility::Visible);
    cuberoom.ready = true;
}

/// The show: a new shot every bar, the tumble surging on the kick and
/// coasting off it, the cube solid in the palette's first colour with the
/// second on its wireframe.
pub(super) fn animate(
    cuberoom: Res<CubeRoom>,
    params: Res<CubeParams>,
    screen: Res<Screen>,
    s: Res<State>,
    beats: Res<Beats>,
    mut materials: ResMut<Assets<CubeRoomMaterial>>,
    mut fx_materials: ResMut<Assets<FxMaterial>>,
    mut players: Query<&mut AnimationPlayer>,
    mut transforms: Query<&mut Transform>,
) {
    if !cuberoom.ready {
        return;
    }
    let bp = beats.0.fract();
    let rush = (1.0 - bp).powi(3);

    let shot = (beats.0 / 4.0) as usize % cuberoom.shots.len();
    if let Ok(mut transform) = transforms.get_mut(cuberoom.cam) {
        *transform = cuberoom.shots[shot];
    }

    if let Some(cube) = cuberoom.cube {
        if let (Some(index), Ok(mut player)) = (cuberoom.anim, players.get_mut(cube))
            && let Some(anim) = player.animation_mut(index)
        {
            anim.set_speed(params.spin * (1.0 + params.surge * rush));
        }
        if let Ok(mut transform) = transforms.get_mut(cube) {
            transform.scale = Vec3::splat(0.5 + params.pump * rush);
        }
    }

    if let Some(mut material) = materials.get_mut(&cuberoom.cube_mat) {
        let beam = to_linear(s.palette.beam_color(&s));
        let par = to_linear(s.palette.par_color(&s));
        material.color0 = beam;
        // A one-colour palette wears a white wireframe instead of none.
        material.color1 = match par == beam {
            true => LinearRgba::WHITE,
            false => par,
        };
        material.params.x = params.wire;
    }

    // milstrike hit the frame on every camera jump; a mega burst as the bar
    // swaps the shot, dead within its first beat. Runs after the fx drive,
    // stacking on whatever the preset holds.
    if screen.pattern == Pattern::CubeRoom
        && let Some(mut material) = fx_materials.get_mut(&screen.fx)
    {
        let jump = beats.0 - (beats.0 / 4.0).floor() * 4.0;
        material.warp.w =
            (material.warp.w + params.mega * (1.0 - jump).max(0.0).powi(6)).min(2.0);
    }
}
