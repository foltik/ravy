use bevy::gltf::GltfMaterialName;

use crate::prelude::*;
use crate::sim::motor::{Motor, MotorDynamics};

#[rustfmt::skip]
#[bevy_trait_query::queryable]
pub trait MovingHeadDevice: DmxDevice {
    fn name(&self) -> &'static str;
    fn watts(&self) -> f32;

    /// 3d model in .glb format.
    fn model(&self) -> &'static [u8];
    fn model_path(&self) -> &'static str;

    fn pitch_axis(&self) -> Axis { Axis::Z }
    fn pitch_dynamics(&self) -> MotorDynamics;
    fn yaw_axis(&self) -> Axis { Axis::Y }
    fn yaw_dynamics(&self) -> MotorDynamics;

    fn pitch(&self) -> f32;
    fn yaw(&self) -> f32;
    fn color(&self) -> Rgbw;
}

#[derive(Component)]
pub struct MovingHeadSetup;

pub fn setup_pre(
    mut cmds: Commands,
    fixtures: Query<(Entity, One<&dyn MovingHeadDevice>), Without<MovingHeadSetup>>,
    children: Query<&Children>,
    assets: Res<AssetServer>,
) {
    for (entity, device) in fixtures {
        cmds.entity(entity).remove::<GltfSceneLoader>();
        cmds.entity(entity).remove::<GltfScene>();
        cmds.entity(entity).remove::<SceneRoot>();
        for child in children.iter_descendants(entity) {
            cmds.entity(child).despawn();
        }

        cmds.entity(entity).insert(MovingHeadSetup);

        let handle = assets.load::<Gltf>(format!("memory://{}", device.model_path()));

        // XXX: don't construct this manually, make GltfSceneLoader fields private again
        let loader = GltfSceneLoader { handle, builder: Default::default() };
        cmds.entity(entity).insert((Name::new(device.name()), loader));
    }
}

#[derive(Component)]
pub struct MovingHead {
    pitch_motor: Entity,
    yaw_motor: Entity,
    material: Handle<StandardMaterial>,
}

pub fn setup_post(
    mut cmds: Commands,
    fixtures: Query<
        (Entity, One<&dyn MovingHeadDevice>, &GltfScene),
        (With<MovingHeadSetup>, Without<MovingHead>),
    >,
    children: Query<&Children>,
    names: Query<&Name>,

    gltf_materials: Query<&GltfMaterialName>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (entity, fixture, _scene) in fixtures {
        let mut pitch_motor = None;
        let mut yaw_motor = None;
        let mut material = None;

        for child in children.iter_descendants(entity) {
            if names.get(child).is_ok_and(|name| name.starts_with("Head")) {
                let id = cmds
                    .entity(child)
                    .insert(Motor::new(fixture.pitch_axis(), fixture.pitch_dynamics()))
                    .id();
                pitch_motor = Some(id);
            }

            if names.get(child).is_ok_and(|name| name.starts_with("Yoke")) {
                let id = cmds
                    .entity(child)
                    .insert(Motor::new(fixture.yaw_axis(), fixture.yaw_dynamics()))
                    .id();
                yaw_motor = Some(id);
            }

            if gltf_materials.get(child).is_ok_and(|name| name.0.starts_with("Beam")) {
                let beam = materials.add(StandardMaterial {
                    base_color: Color::srgba(1.0, 1.0, 1.0, 0.15),
                    alpha_mode: AlphaMode::Blend,
                    emissive: Color::linear_rgba(1.0, 1.0, 1.0, 1.0).into(),
                    // perceptual_roughness: 0.2,
                    ..Default::default()
                });

                cmds.entity(child).insert(MeshMaterial3d(beam.clone()));

                material = Some(beam);
            }
        }

        cmds.entity(entity).insert(MovingHead {
            pitch_motor: pitch_motor.unwrap(),
            yaw_motor: yaw_motor.unwrap(),
            material: material.unwrap(),
        });
    }
}

pub fn update(
    fixtures: Query<(&MovingHead, One<&dyn MovingHeadDevice>)>,
    mut motors: Query<&mut Motor>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (fixture, device) in fixtures {
        motors.get_mut(fixture.pitch_motor).unwrap().rotate(device.pitch());
        motors.get_mut(fixture.yaw_motor).unwrap().rotate(device.yaw());

        let Rgbw(r, g, b, w) = device.color();
        let s = 10.0; // emissive strength
        materials.get_mut(&fixture.material).unwrap().emissive =
            Color::linear_rgba(s * (r + w).min(1.0), s * (g + w).min(1.0), s * (b + w).min(1.0), 0.15).into();
    }
}
