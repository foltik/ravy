#![allow(unused)]

use std::collections::HashMap;
use std::sync::Arc;

use bevy::animation::RepeatAnimation;
use bevy::asset::AssetPath;
use bevy::ecs::archetype::Archetypes;
use bevy::ecs::component::Components;
use bevy::gltf::GltfNode;
use bevy::prelude::*;
use bevy::scene::SceneInstanceReady;

pub struct GltfScenePlugin;

impl Plugin for GltfScenePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreUpdate, (reload_gltfs, load_gltfs_pre))
            .add_systems(PostUpdate, animate_gltfs);
    }
}

type MatchFn = Arc<dyn Fn(&str) -> bool + Send + Sync + 'static>;
type InsertFn = Arc<dyn Fn(&mut EntityCommands) + Send + Sync + 'static>;
type InsertMatchingFn = Arc<dyn Fn(&mut EntityCommands) + Send + Sync + 'static>;
type CameraFn = Arc<dyn Fn(&mut Camera) + Send + Sync + 'static>;

#[derive(Default, Clone)]
pub struct GltfSceneBuilder {
    pub insert_fns: Vec<InsertFn>,
    pub insert_on_fns: HashMap<String, InsertFn>,
    pub insert_on_matching_fns: Vec<(MatchFn, InsertMatchingFn)>,
    pub camera_fn: Option<CameraFn>,
    pub replace_materials: HashMap<String, Handle<StandardMaterial>>,
}

#[derive(Component, Clone)]
struct GltfSceneLoader {
    handle: Handle<Gltf>,
    builder: GltfSceneBuilder,
}

#[derive(Component)]
pub struct GltfScene {
    /// Map from glTF animation names to their index in the AnimationGraph
    animations: HashMap<String, AnimationNodeIndex>,
    /// Pointer to the AnimationPlayer
    animation_player: Option<Entity>,
    /// List of deferred animation playback operations.
    ///
    /// We accumulate here and drain in a separate system to avoid requiring the
    /// user to pass in a reference to the AnimationPlayer.
    animation_ops: Vec<(AnimationOp, AnimationNodeIndex)>,
}

impl GltfSceneBuilder {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn camera(mut self, camera_fn: impl Fn(&mut Camera) + Send + Sync + 'static) -> Self {
        self.camera_fn = Some(Arc::new(camera_fn));
        self
    }

    /// Insert a component to the root scene entity.
    pub fn insert<B: Bundle + Clone>(mut self, bundle: B) -> Self {
        self.insert_fns.push(Arc::new(move |cmds| {
            cmds.insert(bundle.clone());
        }));
        self
    }

    /// Insert a component to a named entity in the scene.
    pub fn insert_on<B: Bundle + Clone>(mut self, entity: impl Into<String>, bundle: B) -> Self {
        self.insert_on_fns.insert(
            entity.into(),
            Arc::new(move |cmds| {
                cmds.insert(bundle.clone());
            }),
        );
        self
    }

    /// Insert a component to all entities in the scene whose name matches the given predicate.
    pub fn insert_on_matching<B: Bundle + Clone>(
        mut self,
        func: impl Fn(&str) -> bool + Send + Sync + 'static,
        bundle: B,
    ) -> Self {
        self.insert_on_matching_fns.push((
            Arc::new(func),
            Arc::new(move |cmds| {
                cmds.insert(bundle.clone());
            }),
        ));
        self
    }

    pub fn replace_material(mut self, entity: impl Into<String>, material: Handle<StandardMaterial>) -> Self {
        self.replace_materials.insert(entity.into(), material);
        self
    }

    pub fn spawn<'a>(
        self,
        path: impl Into<AssetPath<'a>>,
        cmds: &mut Commands,
        assets: &AssetServer,
    ) -> Entity {
        let handle = assets.load::<Gltf>(path);
        let loader = GltfSceneLoader { handle, builder: self };
        cmds.spawn(loader).id()
    }
}

// Once a glTF is loaded from disk, spawn in the contained scene.
fn load_gltfs_pre(
    mut cmds: Commands,
    mut loaders: Query<(Entity, &GltfSceneLoader), Without<SceneRoot>>,
    gltfs: Res<Assets<Gltf>>,
    mut animation_graphs: ResMut<Assets<AnimationGraph>>,
) {
    for (entity, loader) in loaders.iter() {
        let Some(gltf) = gltfs.get(&loader.handle) else {
            continue;
        };

        assert_eq!(gltf.scenes.len(), 1, "glTF must have exactly one scene");
        cmds.entity(entity)
            .insert(SceneRoot(gltf.scenes[0].clone()))
            .observe(load_gltfs_post);
    }
}

// Once a glTF is loaded and its scene has been spawned, setup a GltfScene for it.
fn load_gltfs_post(
    trigger: Trigger<SceneInstanceReady>,
    mut cmds: Commands,
    children: Query<&Children>,
    names: Query<&Name>,
    mut loaders: Query<&mut GltfSceneLoader>,
    mut cameras: Query<&mut Camera>,
    animation_players: Query<Entity, With<AnimationPlayer>>,
    mut animation_graphs: ResMut<Assets<AnimationGraph>>,
    gltfs: Res<Assets<Gltf>>,
    gltf_nodes: Res<Assets<GltfNode>>,
) {
    let scene = trigger.target();

    // unwrap(): we've ensured these are present in `load_gltfs_pre()`
    let mut loader = loaders.get_mut(scene).unwrap();
    let gltf = gltfs.get(&loader.handle).unwrap();

    // Add components to the root scene entity
    for insert_fn in &loader.builder.insert_fns {
        insert_fn(&mut cmds.entity(scene));
    }

    // Add components to named child entities in the scene
    for (name, insert_fn) in &loader.builder.insert_on_fns {
        let node_handle = gltf.named_nodes.get(name.as_str()).unwrap_or_else(|| {
            let names = gltf.named_nodes.keys().collect::<Vec<_>>();
            panic!("no such entity {name:?}. available: {names:?}")
        });

        // unwrap(): it's guaranteed to be present once `SceneInstanceReady` fires
        let node = gltf_nodes.get(node_handle).unwrap();

        let child = children
            .iter_descendants(scene)
            .find(|&desc| names.get(desc).map_or(false, |n| n.as_str() == name))
            .expect("no child with Name component");

        insert_fn(&mut cmds.entity(child));
    }

    for (match_fn, insert_fn) in &loader.builder.insert_on_matching_fns {
        for (name, node_handle) in &gltf.named_nodes {
            let name: &str = &*name;
            if match_fn(name) {
                // unwrap(): it's guaranteed to be present once `SceneInstanceReady` fires
                let node = gltf_nodes.get(node_handle).unwrap();

                let child = children
                    .iter_descendants(scene)
                    .find(|&desc| names.get(desc).map_or(false, |n| n.as_str() == name))
                    .expect("no child with Name component");

                insert_fn(&mut cmds.entity(child));
            }
        }
    }

    // Modify the camera
    if let Some(camera_fn) = loader.builder.camera_fn.as_ref() {
        for mut camera in cameras.iter_mut() {
            camera_fn(&mut *camera);
        }
    }

    // If the glTF has animations, when spawned as a scene it will have a bunch
    // of AnimationTargets pointing to a single AnimationPlayer.
    let animation_player = children
        .iter_descendants(scene)
        .find_map(|desc| animation_players.get(desc).ok());

    // If this player is present we'll attach an AnimationGraph to it. We can
    // control playback later with the player and the indices of each clip in the graph.
    let mut animations = HashMap::new();
    if let Some(animation_player) = animation_player {
        let mut graph = AnimationGraph::new();
        for (name, clip) in &gltf.named_animations {
            let idx = graph.add_clip(clip.clone(), 1.0, graph.root);
            animations.insert(name.to_string(), idx);
        }
        cmds.entity(animation_player)
            .insert(AnimationGraphHandle(animation_graphs.add(graph)));
    }

    cmds.entity(scene)
        .insert(GltfScene { animations, animation_player, animation_ops: vec![] });
}

fn reload_gltfs(
    mut asset_events: EventReader<AssetEvent<Gltf>>,
    children: Query<&Children>,
    mut cmds: Commands,
    scenes: Query<(Entity, &GltfSceneLoader)>,
) {
    for event in asset_events.read() {
        if let AssetEvent::Modified { id } = event {
            for (entity, loader) in scenes {
                if loader.handle.id() == *id {
                    cmds.entity(entity).despawn();
                    cmds.spawn(loader.clone());
                }
            }
        }
    }
}

impl GltfScene {
    fn animation_idx(&self, name: &str) -> AnimationNodeIndex {
        *self.animations.get(name).unwrap_or_else(|| {
            let names = self.animations.keys().collect::<Vec<_>>();
            panic!("no such animation {name:?}. available: {names:?}")
        })
    }

    pub fn start(&mut self, animation: &str) {
        self.start_at_speed(animation, 1.0)
    }
    pub fn start_at_speed(&mut self, animation: &str, speed: f32) {
        self.animation_ops
            .push((AnimationOp::Start { speed, repeat: false }, self.animation_idx(animation)));
    }

    pub fn repeat(&mut self, animation: &str) {
        self.repeat_at_speed(animation, 1.0)
    }
    pub fn repeat_at_speed(&mut self, animation: &str, speed: f32) {
        self.animation_ops
            .push((AnimationOp::Start { speed, repeat: true }, self.animation_idx(animation)));
    }

    pub fn play(&mut self, animation: &str) {
        self.animation_ops.push((AnimationOp::Play, self.animation_idx(animation)));
    }
    pub fn pause(&mut self, animation: &str) {
        self.animation_ops.push((AnimationOp::Pause, self.animation_idx(animation)));
    }
    pub fn toggle(&mut self, animation: &str) {
        self.animation_ops.push((AnimationOp::Toggle, self.animation_idx(animation)));
    }
    pub fn stop(&mut self, animation: &str) {
        self.animation_ops.push((AnimationOp::Stop, self.animation_idx(animation)));
    }
}

enum AnimationOp {
    Start { speed: f32, repeat: bool },
    Play,
    Pause,
    Toggle,
    Stop,
    SetSpeed(f32),
}

fn animate_gltfs(mut scenes: Query<(Entity, &mut GltfScene)>, mut players: Query<&mut AnimationPlayer>) {
    for (entity, mut scene) in scenes.iter_mut() {
        if let Some(player) = scene.animation_player
            && !scene.animation_ops.is_empty()
        {
            // unwrap(): we've ensured the player is present in `load_gltfs_post()`
            let mut player = players.get_mut(player).unwrap();
            for (op, idx) in scene.animation_ops.drain(..) {
                // unwrap(): we created all these indices ourselves in `load_gltfs_post()`
                match op {
                    AnimationOp::Start { speed, repeat } => {
                        player.start(idx).set_speed(speed).set_repeat(match repeat {
                            true => RepeatAnimation::Forever,
                            false => RepeatAnimation::Never,
                        });
                    }
                    AnimationOp::Stop => {
                        player.stop(idx);
                    }
                    AnimationOp::Play => {
                        player.play(idx);
                    }
                    AnimationOp::Pause => {
                        player.animation_mut(idx).unwrap().pause();
                    }
                    AnimationOp::Toggle => {
                        if let Some(anim) = player.animation_mut(idx) {
                            if anim.is_finished() {
                                player.start(idx);
                            } else if anim.is_paused() {
                                anim.resume();
                            } else {
                                anim.pause();
                            }
                        } else {
                            player.start(idx);
                        }
                    }
                    AnimationOp::SetSpeed(speed) => {
                        player.animation_mut(idx).unwrap().set_speed(speed);
                    }
                }
            }
        }
    }
}
