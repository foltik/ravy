use bevy::gltf::GltfMaterialName;

use crate::prelude::*;
use crate::sim::motor::{Motor, MotorDynamics};

#[rustfmt::skip]
#[bevy_trait_query::queryable]
pub trait MovingHeadDevice: DmxDevice {
    fn name(&self) -> &'static str;
    fn intensity(&self) -> f32;
    fn range(&self) -> f32;
    /// Beam angle angle in degrees.
    fn beam_angle(&self) -> f32;

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
            cmds.entity(child).try_despawn();
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
    head: Entity,
    yoke: Entity,
    material: Handle<StandardMaterial>,
    light: Entity,
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
    for (entity, device, _scene) in fixtures {
        let mut head = None;
        let mut yoke = None;
        let mut material = None;
        let mut light = None;

        for child in children.iter_descendants(entity) {
            if names.get(child).is_ok_and(|name| name.starts_with("Head")) {
                head = Some(child);

                cmds.entity(child)
                    .insert((Motor::new(device.pitch_axis(), device.pitch_dynamics()),));

                let spot = cmds
                    .spawn((
                        SpotLight {
                            color: Color::WHITE,
                            intensity: device.intensity(),
                            inner_angle: 0.0,
                            outer_angle: device.beam_angle().to_radians(),
                            range: device.range(),
                            radius: 0.0,
                            shadows_enabled: false,
                            ..Default::default()
                        },
                        Transform::from_rotation(Quat::from_rotation_y(-TAU / 4.0)),
                    ))
                    .id();
                cmds.entity(child).add_child(spot);
                light = Some(spot);
            }

            if names.get(child).is_ok_and(|name| name.starts_with("Yoke")) {
                yoke = Some(child);

                cmds.entity(child).insert(Motor::new(device.yaw_axis(), device.yaw_dynamics()));
            }

            if gltf_materials.get(child).is_ok_and(|name| name.0.starts_with("Beam")) {
                let beam = materials.add(StandardMaterial {
                    base_color: Color::BLACK,
                    alpha_mode: AlphaMode::Blend,
                    emissive: Color::BLACK.into(),
                    ..Default::default()
                });
                material = Some(beam.clone());

                cmds.entity(child).insert(MeshMaterial3d(beam));
            }
        }

        cmds.entity(entity).insert(MovingHead {
            head: head.unwrap(),
            yoke: yoke.unwrap(),
            material: material.unwrap(),
            light: light.unwrap(),
        });
    }
}

pub fn update(
    fixtures: Query<(&MovingHead, One<&dyn MovingHeadDevice>)>,
    mut motors: Query<&mut Motor>,
    mut lights: Query<&mut SpotLight>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (fixture, device) in fixtures {
        motors.get_mut(fixture.head).unwrap().rotate(device.pitch());
        motors.get_mut(fixture.yoke).unwrap().rotate(device.yaw());

        let color = Rgb::from(device.color());
        let Rgb(r, g, b) = color;

        let mut light = lights.get_mut(fixture.light).unwrap();
        light.color = Color::linear_rgb(r, g, b);
        light.intensity = device.intensity() * color.luminance();

        let material = materials.get_mut(&fixture.material).unwrap();
        let s_emit = device.intensity() * 0.0001 * color.luminance();
        let s_alpha = 0.08 * color.luminance();
        material.base_color.set_alpha(s_alpha);
        material.emissive = Color::linear_rgba(s_emit * r, s_emit * g, s_emit * b, 1.0).into();
    }
}
