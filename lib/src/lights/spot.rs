use bevy::gltf::GltfMaterialName;

use crate::prelude::*;

#[rustfmt::skip]
#[bevy_trait_query::queryable]
pub trait SpotDevice: DmxDevice {
    fn name(&self) -> &'static str;
    fn intensity(&self) -> f32;
    fn range(&self) -> f32;
    /// Beam angle angle in degrees.
    fn beam_angle(&self) -> f32;

    /// 3d model in .glb format.
    fn model(&self) -> &'static [u8];
    fn model_path(&self) -> &'static str;

    fn color(&self) -> Rgbw;
}

#[derive(Component)]
pub struct SpotSetup;

pub fn setup_pre(
    mut cmds: Commands,
    fixtures: Query<(Entity, One<&dyn SpotDevice>), Without<SpotSetup>>,
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

        cmds.entity(entity).insert(SpotSetup);

        let handle = assets.load::<Gltf>(format!("memory://{}", device.model_path()));

        // XXX: don't construct this manually, make GltfSceneLoader fields private again
        let loader = GltfSceneLoader { handle, builder: Default::default() };
        cmds.entity(entity).insert((Name::new(device.name()), loader));
    }
}

#[derive(Component)]
pub struct Spot {
    material: Handle<StandardMaterial>,
    light: Entity,
}

pub fn setup_post(
    mut cmds: Commands,
    fixtures: Query<(Entity, One<&dyn SpotDevice>, &GltfScene), (With<SpotSetup>, Without<Spot>)>,
    children: Query<&Children>,
    names: Query<&Name>,

    gltf_materials: Query<&GltfMaterialName>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (entity, device, _scene) in fixtures {
        let mut material = None;
        let mut light = None;

        for child in children.iter_descendants(entity) {
            if names.get(child).is_ok_and(|name| name.starts_with("Head")) {
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
                        Transform::from_rotation(Quat::from_rotation_x(-TAU / 4.0)),
                    ))
                    .id();

                cmds.entity(child).add_child(spot);
                light = Some(spot);
            }

            if gltf_materials.get(child).is_ok_and(|name| name.0.starts_with("Beam")) {
                let beam = materials.add(StandardMaterial {
                    base_color: Color::srgba(1.0, 1.0, 1.0, 0.04),
                    alpha_mode: AlphaMode::Blend,
                    emissive: Color::linear_rgba(1.0, 1.0, 1.0, 1.0).into(),
                    ..Default::default()
                });
                material = Some(beam.clone());

                cmds.entity(child).insert(MeshMaterial3d(beam));
            }
        }

        cmds.entity(entity)
            .insert(Spot { material: material.unwrap(), light: light.unwrap() });
    }
}

pub fn update(
    fixtures: Query<(&Spot, One<&dyn SpotDevice>)>,
    mut lights: Query<&mut SpotLight>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (fixture, device) in fixtures {
        let color = Rgb::from(device.color());
        let Rgb(r, g, b) = color;

        let mut light = lights.get_mut(fixture.light).unwrap();
        light.color = Color::linear_rgb(r, g, b);
        light.intensity = device.intensity() * color.luminance();

        let material = materials.get_mut(&fixture.material).unwrap();
        let s = device.intensity() * 0.0001 * color.luminance();
        material.emissive = Color::linear_rgba(s * r, s * g, s * b, 0.15).into();
    }
}
