use bevy::core_pipeline::bloom::Bloom;
use bevy::core_pipeline::tonemapping::Tonemapping;
use lib::prelude::*;

const GROW_GAIN: f32 = 1.0; // RMS factor
const GROW_FACTOR: f32 = 2.0; // Max grow scale

const WAVE_HZ: f32 = 0.05; // Base frequency
const WAVE_GAIN: f32 = 0.20; // RMS factor
const WAVE_LENGTH: f32 = 0.5; // Distance between crests in meters
const WAVE_M: f32 = 0.05; // Maximum forward offset in meters

const BOB_M: f32 = 1.0; // Bob amplitude in meters

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
        .add_systems(Startup, setup)
        .add_systems(PreUpdate, cache_transforms)
        .add_systems(Update, (grow, wave, bob))
        .run();
}

fn setup(mut cmds: Commands, assets: Res<AssetServer>) {
    info!("Hello, world!");

    GltfSceneBuilder::new()
        .insert_on("Camera", (Tonemapping::TonyMcMapface, Bloom::NATURAL))
        .insert_on("Icosphere", Bob::default())
        .insert_on_matching(|name| name.starts_with("Speakers.0"), (Grow::default(), Wave::default()))
        .camera(|cam| {
            cam.hdr = true;
            cam.clear_color = ClearColorConfig::Custom(Color::BLACK);
        })
        .spawn("Jungle.glb", &mut cmds, &assets);
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
