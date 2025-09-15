use bevy::core_pipeline::bloom::Bloom;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::pbr::{FogVolume, VolumetricFog, VolumetricLight};
use lib::prelude::*;

const GROW_GAIN: f32 = 1.0; // RMS factor
const GROW_FACTOR: f32 = 2.0; // Max grow scale

const WAVE_HZ: f32 = 0.05; // Base frequency
const WAVE_GAIN: f32 = 1.00; // RMS factor
const WAVE_LENGTH: f32 = 0.5; // Distance between crests in meters
const WAVE_M: f32 = 0.05; // Maximum forward offset in meters

const BOB_M: f32 = 1.0; // Bob amplitude in meters

/*
 *
# Ideas
- jungle temple spinning with giant light and fog https://bevy.org/examples/3d-rendering/fog/
- monkeys swinging from vines
- MOSH spinning 3d text and animals vibrate surrounding speakers https://github.com/FrankenApps/meshtext
- elephant that shoots a laser out the trunk
- loop of walking down a dense path with lots of foliage
- change animal for every dj

# Todo
- fullscreen post-processing shaders (edge detection, glitch, melt)
- reusable components and systems for common animations
- incorporate Midi, Dmx
- scene switching system
- integrate bevy_inspector_egui

*/

/// ----------------------------------------------------------

/// Example app.
#[derive(argh::FromArgs)]
struct Args {
    /// enable debug logging
    #[argh(switch, short = 'v')]
    debug: bool,
    /// enable trace logging
    #[argh(switch, short = 'V')]
    trace: bool,
}

fn main() {
    let args: Args = argh::from_env();
    App::new()
        .add_plugins(RavyPlugin { module: module_path!(), debug: args.debug, trace: args.trace })
        .add_systems(Startup, (setup, setup_lights))
        .add_systems(PreUpdate, cache_transforms)
        .add_systems(Update, (grow, wave, bob))
        .add_systems(Update, hotkeys)
        .run();
}

fn setup(mut cmds: Commands, assets: Res<AssetServer>) {
    info!("Hello, world!");

    GltfSceneBuilder::new()
        .insert_on(
            "Camera",
            (
                Tonemapping::TonyMcMapface,
                Bloom::NATURAL,
                // VolumetricFog { step_count: 256, jitter: 0.1, ..Default::default() },
                // DistanceFog {
                //     color: Color::srgb(0.25, 0.25, 0.25),
                //     falloff: FogFalloff::ExponentialSquared { density: 0.005 },
                //     ..default()
                // },
            ),
        )
        .insert_on("Icosphere", Bob::default())
        .insert_on_matching(|name| name.starts_with("Speakers.0"), (Grow::default(), Wave::default()))
        .insert_on_matching(
            |name| name.starts_with("Point") || name.starts_with("Spot"),
            Bob::default(),
            // (VolumetricLight, Bob::default()),
        )
        .camera(|cam| {
            cam.hdr = true;
            cam.clear_color = ClearColorConfig::Custom(Color::BLACK);
        })
        .spawn("Jungle.glb", &mut cmds, &assets);

    cmds.spawn((
        Transform::from_xyz(0.0, 6.9, 0.0).looking_at(Vec3::ZERO, Vec3::Y),
        SpotLight {
            intensity: 50000.0, // lumens
            color: GREEN.into(),
            shadows_enabled: true,
            inner_angle: 0.35,
            outer_angle: 0.85,
            ..default()
        },
        // VolumetricLight,
    ));

    // cmds.spawn((FogVolume::default(), Transform::from_scale(Vec3::splat(30.0))));
}

pub fn setup_lights(
    mut cmds: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    // 10 m^3 cube ⇒ edge length cbrt(10)
    let edge = 20.0;
    let half = edge * 0.5;

    // Place cube so it sits above the origin: y ∈ [0, edge]
    // and centered on X/Z around 0.
    let y_min = 0.0;
    let y_max = edge;
    let x_min = -half;
    let x_max = half;
    let z_min = -half;
    let z_max = half;

    let bulb_mesh = meshes.add(Mesh::from(Sphere { radius: 0.2 }));

    // Simple unlit emissive-ish white so the bulbs are visible even if
    // exposure/intensity varies. (You can tweak color later.)
    let bulb_mat = mats.add(StandardMaterial {
        base_color: Color::srgb(0.0, 1.0, 0.0),
        // Make it pop without affecting PBR too much:
        emissive: Color::srgb(0.0, 100.0, 0.0).into(),
        unlit: true,
        // alpha_mode: AlphaMode::Opaque,
        ..default()
    });

    // Choose a spacing that respects the bulb radius to avoid overlaps.
    // 0.5 m centers gives some breathing room (diameter = 0.4 m).
    let step = 5.0_f32;

    // Build integer ranges that fit neatly within the box
    let mut xs = vec![];
    let mut ys = vec![];
    let mut zs = vec![];

    {
        let mut v = x_min + step * 0.5;
        while v <= x_max - step * 0.5 {
            xs.push(v);
            v += step;
        }

        let mut v = y_min + step * 0.5;
        while v <= y_max - step * 0.5 {
            ys.push(v);
            v += step;
        }

        let mut v = z_min + step * 0.5;
        while v <= z_max - step * 0.5 {
            zs.push(v);
            v += step;
        }
    }

    for &x in &xs {
        for &y in &ys {
            for &z in &zs {
                cmds.spawn((
                    // Visual bulb
                    Mesh3d(bulb_mesh.clone()),
                    MeshMaterial3d(bulb_mat.clone()),
                    Transform::from_translation(Vec3::new(x, y, z)),
                    // Actual light
                    PointLight {
                        intensity: 8_000.0, // tune to taste
                        color: Color::WHITE,
                        range: 10.0, // small local influence
                        radius: 0.0,
                        shadows_enabled: false,
                        ..default()
                    },
                    // Your component
                    Bob::default(),
                ));
            }
        }
    }
}

fn hotkeys(mut q: Query<&mut GltfScene>, keys: Res<ButtonInput<KeyCode>>) {
    let Ok(mut scene) = q.single_mut() else {
        return;
    };

    if keys.just_pressed(KeyCode::Space) {
        scene.start("CameraMove1");
    }
}

// -------------------------------------------------------------------------------

#[derive(Component, Clone, Default)]
struct Grow;

/// Grow pulsing with RMS
fn grow(mut q: Query<(&mut Transform, &OrigTransform), With<Grow>>, audio: Res<Audio>) {
    let ds = (GROW_FACTOR - 1.0) + GROW_GAIN * audio.rms();
    for (mut xform, orig) in &mut q {
        xform.scale = orig.scale * Vec3::splat(ds);
    }
}

// -------------------------------------------------------------------------------

#[derive(Component, Clone, Default)]
struct Wave {
    phase: f32,
}

fn wave(mut q: Query<(&mut Transform, &OrigTransform, &mut Wave)>, time: Res<Time>, audio: Res<Audio>) {
    let dt = time.delta_secs();
    let hz = WAVE_HZ + WAVE_GAIN * audio.rms();

    for (mut xform, orig, mut wave) in &mut q {
        wave.phase = (wave.phase + hz * dt).fract();

        let phase_offset = orig.0.translation.y / WAVE_LENGTH;
        let phi = (wave.phase - phase_offset) * std::f32::consts::TAU;
        let dz = phi.sin() * 0.5 + 0.5;

        xform.translation.z = orig.0.translation.z + dz * WAVE_M;
    }
}

// -------------------------------------------------------------------------------

#[derive(Component, Clone, Default)]
struct Bob;

fn bob(mut q: Query<(&mut Transform, &OrigTransform), With<Bob>>, time: Res<Time>) {
    let t = time.elapsed_secs();
    let dy = t.sin() * 0.5;
    for (mut xform, orig) in &mut q {
        xform.translation.y = orig.translation.y + dy * BOB_M;
    }
}

// -------------------------------------------------------------------------------

#[derive(Component)]
struct OrigTransform(Transform);

impl std::ops::Deref for OrigTransform {
    type Target = Transform;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

fn cache_transforms(mut cmds: Commands, q_added: Query<(Entity, &Transform), Added<Transform>>) {
    for (e, t) in &q_added {
        cmds.entity(e).insert(OrigTransform(*t));
    }
}
