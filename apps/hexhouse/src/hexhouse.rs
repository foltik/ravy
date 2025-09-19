use bevy::core_pipeline::bloom::Bloom;
use bevy::core_pipeline::tonemapping::Tonemapping;
use lib::lights::fixture::{SaberSpot, StealthBeam};
use lib::prelude::*;

// TODO:
//
// # Easy
// - [ ] port lighting code from berlinrat
//
// # Hard
// - [ ] way to dynamically add inspector tab
// - [ ] midi device internal state + floating egui emulator wind
// - [ ] proper DmxDevice system + DmxChannel component (!! debug non-working DMX)
// - [ ] ping E131 and add a status panel
//
// # Cosmetic
// - [ ] better factor moving_head device registration
// - [ ] dmx debugger tab
//
// # Notes
//
// RoomBlock scale: 10.91 4.57 5.2931
//
// Blender | Bevy
// --------+-----
// X       | X
// Y       | Z
// Z       | -Y
//
// MovingHead conventions:
// ---------------------
// Pointing: -Y -> -Z
// Head Rotation: Y -> Z
// Yoke Rotation: Z -> -Y
//

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
        .add_systems(Startup, spawn)
        .add_systems(Update, (setup_scene, render_lights, hotkeys))
        .run();
}

#[derive(Component, Clone)]
struct HexHouse;
fn spawn(mut cmds: Commands, assets: Res<AssetServer>) -> Result {
    GltfSceneBuilder::new()
        .insert(HexHouse)
        .spawn("HexHouse.glb", &mut cmds, &assets);

    cmds.insert_resource(E131::new("10.16.4.1")?);
    cmds.insert_resource(Preset::Wave);
    Ok(())
}

#[derive(Resource, Clone, Copy)]
enum Preset {
    Zero,
    Down,
    Wave,
    Circle,
    Disco,
}

#[derive(Component)]
struct DiscoBall;

#[derive(Component)]
struct DmxChannel(usize);

#[derive(Component)]
struct HexHouseSetup;
fn setup_scene(
    mut cmds: Commands,
    scene: Query<(Entity, &GltfScene), (With<HexHouse>, Without<HexHouseSetup>)>,
    mut camera: Query<(Entity, &mut Camera)>,
) {
    let Ok((scene_ent, scene)) = scene.single() else {
        return;
    };

    let (cam_ent, mut cam) = camera.single_mut().unwrap();
    cam.hdr = true;
    cam.clear_color = ClearColorConfig::Custom(Color::BLACK);
    cmds.entity(cam_ent).insert((Tonemapping::TonyMcMapface, Bloom::NATURAL));

    for (name, entity) in scene.nodes().filter(|(name, _)| name.starts_with("Mover Ch.")) {
        let (_, channel) = name.rsplit_once(".").unwrap();
        let channel: usize = channel.parse().unwrap();
        debug!("Moving Head @ {channel}: {entity}");

        cmds.entity(entity).insert((StealthBeam::default(), DmxChannel(channel)));
    }

    for (name, entity) in scene.nodes().filter(|(name, _)| name.starts_with("Spot Ch.")) {
        let (_, channel) = name.rsplit_once(".").unwrap();
        let channel: usize = channel.parse().unwrap();
        debug!("Spot @ {channel}: {entity}");

        cmds.entity(entity).insert((SaberSpot::default(), DmxChannel(channel)));
    }

    for (_, entity) in scene.nodes().filter(|(name, _)| *name == "DiscoBall") {
        debug!("Disco Ball: {entity}");

        cmds.entity(entity).insert(DiscoBall);
    }

    cmds.entity(scene_ent).insert(HexHouseSetup);
}

fn render_lights(
    beams: Query<(&mut StealthBeam, &Transform, &DmxChannel)>,
    spots: Query<(&mut SaberSpot, &DmxChannel)>,
    disco_ball: Query<&Transform, With<DiscoBall>>,
    preset: Res<Preset>,
    time: Res<Time>,

    mut e131: ResMut<E131>,
) {
    let Ok(disco) = disco_ball.single() else {
        return;
    };

    let t = time.elapsed_secs();
    let hue = (t / 8.0).fract();

    let mut dmx = [0u8; 256];

    for (mut beam, transform, channel) in beams {
        beam.color = Rgb::hsv(hue, 1.0, 1.0).into();

        match *preset {
            Preset::Zero => {
                beam.pitch = 0.0;
                beam.yaw = 0.0;
            }
            Preset::Down => {
                beam.pitch = 0.5;
                beam.yaw = 0.0;
            }
            Preset::Wave => {
                beam.pitch = t.tri(8.0) * 0.75;
                beam.yaw = 0.0;
            }
            Preset::Circle => {
                beam.pitch = 0.33;
                beam.yaw = t.fsin(8.0);
            }
            Preset::Disco => {
                // TODO: refactor this into some sort of look_at() method

                let target_pos = disco.translation;
                let fixture_pos = transform.translation;
                let fixture_rot = transform.rotation;

                // Direction to target in world space
                let world_dir = (target_pos - fixture_pos).normalize();
                // Transform to local space (where +X is forward)
                let local_dir = fixture_rot.inverse() * world_dir;

                // For beam pointing at +X locally:
                // After Ry(yaw) * Rz(pitch), +X becomes:
                // x = cos(yaw) * cos(pitch)
                // y = sin(pitch)
                // z = sin(yaw) * cos(pitch)

                // Solve for angles:
                let mut pitch_deg = local_dir.y.asin().clamp(-PI / 2.0, PI / 2.0).to_degrees();
                let mut yaw_deg = (-local_dir.z).atan2(local_dir.x).to_degrees();

                // Map pitch from [-180, 180] to [0, 180]
                if pitch_deg < 0.0 {
                    pitch_deg = -pitch_deg; // Negate pitch
                    yaw_deg += 180.0; // Flip yaw to compensate
                }
                pitch_deg = pitch_deg.clamp(0.0, 180.0);

                // Map yaw from [-180, 180] to [-540, 0]
                // First normalize to [-180, 180]
                yaw_deg = ((yaw_deg + 180.0).rem_euclid(360.0)) - 180.0;
                // Then map to [-540, 0]
                if yaw_deg > 0.0 {
                    yaw_deg -= 360.0;
                }
                yaw_deg = yaw_deg.clamp(-540.0, 0.0);

                // Normalize to [0, 1]
                let yaw_n = (-yaw_deg / 540.0).clamp(0.0, 1.0); // 0° → 1.0, -540° → 0.0
                let pitch_n = (pitch_deg / 180.0).clamp(0.0, 1.0); // 0° → 0.0, 180° → 1.0

                beam.yaw = yaw_n;
                beam.pitch = pitch_n;
            }
        }

        beam.encode(&mut dmx[channel.0..]);
    }

    for (mut spot, channel) in spots {
        spot.color = Rgb::hsv(hue, 1.0, 1.0).into();
        spot.encode(&mut dmx[channel.0..]);
    }

    e131.send(&dmx);
}

fn hotkeys(keys: Res<ButtonInput<KeyCode>>, mut preset: ResMut<Preset>) {
    if keys.just_pressed(KeyCode::KeyZ) {
        *preset = Preset::Zero;
    }
    if keys.just_pressed(KeyCode::KeyX) {
        *preset = Preset::Down;
    }
    if keys.just_pressed(KeyCode::KeyC) {
        *preset = Preset::Wave;
    }
    if keys.just_pressed(KeyCode::KeyV) {
        *preset = Preset::Circle;
    }
    if keys.just_pressed(KeyCode::KeyB) {
        *preset = Preset::Disco;
    }
}
