use bevy::core_pipeline::bloom::Bloom;
use bevy::core_pipeline::tonemapping::Tonemapping;
use lib::prelude::*;

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

#[derive(Component, Clone)]
struct Bounce;

fn main() {
    let args: Args = argh::from_env();
    App::new()
        .add_plugins(RavyPlugin { module: module_path!(), debug: args.debug, trace: args.trace })
        .add_systems(Startup, setup)
        .add_systems(Update, bounce)
        .run();
}

fn setup(mut cmds: Commands, assets: Res<AssetServer>) {
    info!("Hello, world!");

    GltfSceneBuilder::new()
        .insert_on("Camera", (Tonemapping::TonyMcMapface, Bloom::NATURAL))
        .insert_on_matching(|name| name.starts_with("Speakers.0"), Bounce)
        .camera(|cam| {
            cam.hdr = true;
            cam.clear_color = ClearColorConfig::Custom(Color::BLACK);
        })
        .spawn("Jungle.glb", &mut cmds, &assets);
}

fn bounce(mut speakers: Query<&mut Transform, With<Bounce>>, time: Res<Time>) {
    for mut speaker in &mut speakers {
        let elapsed = time.elapsed().as_secs_f32();

        let fr = (elapsed * 10.0).sin().abs();
        let bouncy = fr.powf(4.0);
        let scale = 1.0 + bouncy * 0.1;

        speaker.scale = vec3(scale, scale, scale);
    }
}
