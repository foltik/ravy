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

fn main() {
    let args: Args = argh::from_env();
    App::new()
        .add_plugins(RavyPlugin { module: module_path!(), debug: args.debug, trace: args.trace })
        .add_systems(Startup, setup)
        .add_systems(Update, animate)
        .run();
}

fn setup(mut cmds: Commands, assets: Res<AssetServer>) {
    info!("Hello, world!");

    GltfSceneBuilder::new()
        .insert_on("Camera", (Tonemapping::TonyMcMapface, Bloom::NATURAL))
        .camera(|cam| {
            cam.hdr = true;
            cam.clear_color = ClearColorConfig::Custom(Color::BLACK);
        })
        .spawn("Default.glb", &mut cmds, &assets);

    cmds.spawn((
        Text::new("Bounce:  Space\nRotate:  R\nQuit:    Q"),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Percent(80.0),
            left: Val::Percent(44.0),
            ..Default::default()
        },
    ));
}

fn animate(keys: Res<ButtonInput<KeyCode>>, mut scene: Query<&mut GltfScene>) {
    if let Ok(mut scene) = scene.single_mut() {
        if keys.just_pressed(KeyCode::Space) {
            scene.start("Bounce");
        }

        if keys.just_pressed(KeyCode::KeyR) {
            scene.toggle("Rotate");
        }
    }
}
