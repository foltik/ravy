use bevy::core_pipeline::bloom::Bloom;
use bevy::core_pipeline::tonemapping::Tonemapping;
use lib::lights::fixture::{SaberSpot, StealthBeam};
use lib::midi::device::launch_control_xl::LaunchControlXL;
use lib::midi::device::launchpad_x::{self, LaunchpadX};
use lib::prelude::*;

mod lights;
mod logic;

// TODO:
//
// # Now
// - [ ] port lighting code from berlinrat
// - [ ] make some cosine palettes
// - [ ] Synesthesia OSC connector (color, scene switching)
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
        .add_systems(Startup, setup)
        .add_systems(PreUpdate, setup_scene)
        // .add_systems(Update, render_lights)
        .add_systems(
            Update,
            (
                logic::on_pad,
                logic::on_ctrl,
                logic::tick,
                logic::render_lights,
                logic::render_pad,
            )
                .chain(),
        )
        .run();
}

fn setup(mut cmds: Commands, assets: Res<AssetServer>) -> Result {
    // Resources
    cmds.insert_resource(logic::State::new());
    cmds.insert_resource(E131::new("10.16.4.1")?);

    // Control surfaces
    let ctrl = Midi::new("Launch Control XL", LaunchControlXL::default());
    let mut pad = Midi::new("Launchpad X LPX MIDI", LaunchpadX::default());
    {
        use launchpad_x::types::*;
        use launchpad_x::*;
        pad.send(Output::Pressure(Pressure::Off, PressureCurve::Medium));
        pad.send(Output::Brightness(0.0));
    }
    cmds.insert_resource(pad);
    cmds.insert_resource(ctrl);

    // Visualizer
    GltfSceneBuilder::new()
        .insert(HexHouse)
        .spawn("HexHouse.glb", &mut cmds, &assets);

    Ok(())
}

#[derive(Component, Clone)]
struct HexHouse;
#[derive(Component)]
struct HexHouseSetup;

#[derive(Component)]
struct FixtureChannel(usize);
#[derive(Component)]
struct FixtureIndex {
    i: usize,
    row: usize,
    col: usize,
}

#[derive(Component)]
struct DiscoBall;

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
        debug!("Mover: {entity}");
        let (chan, idx) = parse_fixture(name);
        cmds.entity(entity).insert((StealthBeam::default(), chan, idx));
    }

    for (name, entity) in scene.nodes().filter(|(name, _)| name.starts_with("Spot Ch.")) {
        debug!("Spot: {entity}");
        let (chan, idx) = parse_fixture(name);
        cmds.entity(entity).insert((SaberSpot::default(), chan, idx));
    }

    for (_, entity) in scene.nodes().filter(|(name, _)| *name == "DiscoBall") {
        debug!("Disco: {entity}");
        cmds.entity(entity).insert(DiscoBall);
    }

    cmds.entity(scene_ent).insert(HexHouseSetup);
}

fn parse_fixture(name: &str) -> (FixtureChannel, FixtureIndex) {
    let (_, parts) = name.split_once(' ').unwrap();
    let parts = parts.split(' ').collect::<Vec<_>>();

    let (_, chan) = parts[0].split_once('.').unwrap();
    let chan = FixtureChannel(chan.parse().unwrap());

    let (_, i) = parts[1].split_once('.').unwrap();
    let i: usize = i.parse().unwrap();
    let (_, row) = parts[2].split_once('.').unwrap();
    let row: usize = row.parse().unwrap();
    let (_, col) = parts[3].split_once('.').unwrap();
    let col: usize = col.parse().unwrap();
    let idx = FixtureIndex { i, row, col };

    (chan, idx)
}

// fn transfer_lights<'a>(
//     time: Res<Time>,

//     mut e131: ResMut<E131>,
// ) {
//     // let Ok(disco) = disco_ball.single() else {
//     //     return;
//     // };

//     // let l = Lights { beams, spots, disco_ball };

//     // let t = time.elapsed_secs();
//     // let hue = (t / 8.0).fract();

//     // let mut dmx = [0u8; 256];

//     // for (mut beam, transform, channel, idx) in beams {
//     //     beam.color = Rgb::hsv(hue, 1.0, 1.0).into();

//     //     match *preset {
//     //         Preset::Zero => {
//     //             beam.pitch = 0.0;
//     //             beam.yaw = 0.0;
//     //         }
//     //         Preset::Down => {
//     //             beam.pitch = 0.5;
//     //             beam.yaw = 0.0;
//     //         }
//     //         Preset::Wave => {
//     //             beam.pitch = t.tri(8.0) * 0.75;
//     //             beam.yaw = 0.0;
//     //         }
//     //         Preset::Circle => {
//     //             beam.pitch = 0.33;
//     //             beam.yaw = t.fsin(8.0);
//     //         }
//     //         Preset::Disco => {
//     //             // TODO: refactor this into some sort of look_at() method

//     //             let target_pos = disco.translation;
//     //             let fixture_pos = transform.translation;
//     //             let fixture_rot = transform.rotation;

//     //             // Direction to target in world space
//     //             let world_dir = (target_pos - fixture_pos).normalize();
//     //             // Transform to local space (where +X is forward)
//     //             let local_dir = fixture_rot.inverse() * world_dir;

//     //             // For beam pointing at +X locally:
//     //             // After Ry(yaw) * Rz(pitch), +X becomes:
//     //             // x = cos(yaw) * cos(pitch)
//     //             // y = sin(pitch)
//     //             // z = sin(yaw) * cos(pitch)

//     //             // Solve for angles:
//     //             let mut pitch_deg = local_dir.y.asin().clamp(-PI / 2.0, PI / 2.0).to_degrees();
//     //             let mut yaw_deg = (-local_dir.z).atan2(local_dir.x).to_degrees();

//     //             // Map pitch from [-180, 180] to [0, 180]
//     //             if pitch_deg < 0.0 {
//     //                 pitch_deg = -pitch_deg; // Negate pitch
//     //                 yaw_deg += 180.0; // Flip yaw to compensate
//     //             }
//     //             pitch_deg = pitch_deg.clamp(0.0, 180.0);

//     //             // Map yaw from [-180, 180] to [-540, 0]
//     //             // First normalize to [-180, 180]
//     //             yaw_deg = ((yaw_deg + 180.0).rem_euclid(360.0)) - 180.0;
//     //             // Then map to [-540, 0]
//     //             if yaw_deg > 0.0 {
//     //                 yaw_deg -= 360.0;
//     //             }
//     //             yaw_deg = yaw_deg.clamp(-540.0, 0.0);

//     //             // Normalize to [0, 1]
//     //             let yaw_n = (-yaw_deg / 540.0).clamp(0.0, 1.0); // 0° → 1.0, -540° → 0.0
//     //             let pitch_n = (pitch_deg / 180.0).clamp(0.0, 1.0); // 0° → 0.0, 180° → 1.0

//     //             beam.yaw = yaw_n;
//     //             beam.pitch = pitch_n;
//     //         }
//     //     }

//     //     beam.encode(&mut dmx[channel.0..]);
//     // }

//     // for (mut spot, channel) in spots {
//     //     spot.color = Rgb::hsv(hue, 1.0, 1.0).into();
//     //     spot.encode(&mut dmx[channel.0..]);
//     // }

//     // e131.send(&dmx);
// }
