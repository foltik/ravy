#![allow(mixed_script_confusables)]

use lib::midi::device::launch_control_xl::LaunchControlXL;
use lib::midi::device::launchpad_x::LaunchpadX;
use lib::prelude::*;

mod dmx;
mod ledwall;
mod lights;
mod logic;
mod mirror;
mod rig;
mod sim;
mod status;
mod ui;

/// Jetlag: the classic mslive rig with the kosmic control scheme, on the
/// witch hut stage with an LED wall.
#[derive(argh::FromArgs)]
struct Args {
    /// serial port path (default: auto-detect, simulate if missing)
    #[argh(option)]
    port: Option<String>,
    /// enable debug logging
    #[argh(switch, short = 'v')]
    debug: bool,
    /// enable trace logging
    #[argh(switch, short = 'V')]
    trace: bool,
}

fn main() {
    let args: Args = argh::from_env();

    let mut app = App::new();
    app.add_plugins(RavyPlugin { module: module_path!(), debug: args.debug, trace: args.trace })
        .add_plugins(ledwall::LedwallPlugin);

    app.add_systems(Startup, (setup, sim::setup, rig::load).chain())
        .add_systems(
            Update,
            (
                dmx::poll,
                logic::on_pad,
                logic::on_ctrl,
                logic::tick,
                logic::render_lights,
                dmx::force,
                dmx::output,
                logic::render_pad,
                logic::render_ctrl,
                sim::sync,
            )
                .chain(),
        )
        .add_systems(
            Update,
            (
                sim::patch,
                sim::beam_setup,
                sim::bigbeam_setup,
                sim::spider_setup,
                sim::bar_setup,
                sim::room_light,
                sim::orbit,
                sim::outline,
            ),
        )
        .add_systems(EguiPrimaryContextPass, (mirror::draw, status::draw, dmx::draw));

    app.insert_resource(dmx::Dmx::open(args.port.as_deref()));
    app.run();
}

fn setup(mut cmds: Commands, mut egui: ResMut<EguiGlobalSettings>) {
    // bevy_egui would otherwise attach the UI to whichever camera spawns
    // first, which can be the wall's offscreen render-target camera; the sim
    // camera carries PrimaryEguiContext instead.
    egui.auto_create_primary_context = false;

    cmds.insert_resource(Universe::default());
    cmds.init_resource::<rig::Trim>();
    cmds.init_resource::<sim::Room>();
    cmds.init_resource::<sim::Highlight>();
    cmds.insert_resource(logic::State::new());
    cmds.insert_resource(lights::Lights::default());

    let ctrl = Midi::new("Launch Control XL", LaunchControlXL);
    let pad = Midi::new("Launchpad X LPX MIDI", LaunchpadX::default());
    cmds.insert_resource(mirror::Pad::new(pad));
    cmds.insert_resource(mirror::Ctrl::new(ctrl));
}
