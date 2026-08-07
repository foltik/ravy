use lib::gdtf::GdtfSystems;
use lib::midi::device::launch_control_xl::LaunchControlXL;
use lib::midi::device::launchpad_x::LaunchpadX;
use lib::prelude::*;

mod blackout;
mod dmx;
mod home;
mod lights;
mod logic;
mod mirror;
mod rig;
mod sim;
mod status;
mod ui;

/// Kosmic: layered looks (energy x movement x texture x palette) for the ENTTEC rig.
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
        // rig::load reads the resources setup makes, and Haze the gdtf plugin does.
        .add_systems(Startup, (setup, sim::setup, rig::load).chain())
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
            )
                .chain()
                // The sim reads the frame this chain writes, debug overrides included.
                .before(GdtfSystems),
        )
        .add_systems(
            Update,
            (
                sim::patch,
                sim::readdress,
                sim::movers,
                sim::blackout_volumes.after(sim::movers),
                sim::outline,
                sim::room_light,
                sim::orbit,
            ),
        )
        .add_systems(EguiPrimaryContextPass, (mirror::draw, status::draw, dmx::draw));

    app.insert_resource(dmx::Dmx::open(args.port.as_deref()));
    app.run();
}

fn setup(mut cmds: Commands) {
    cmds.insert_resource(Universe::default());
    cmds.init_resource::<dmx::Patch>();
    cmds.init_resource::<blackout::Blackout>();
    cmds.init_resource::<home::Home>();
    cmds.init_resource::<rig::Trim>();
    cmds.init_resource::<sim::Room>();
    cmds.init_resource::<sim::Movers>();
    cmds.init_resource::<sim::Highlight>();
    cmds.insert_resource(logic::State::new());
    cmds.insert_resource(lights::Lights::default());
    cmds.insert_resource(lights::Wheels::default());

    let ctrl = Midi::new("Launch Control XL", LaunchControlXL);
    let pad = Midi::new("Launchpad X LPX MIDI", LaunchpadX::default());
    cmds.insert_resource(mirror::Pad::new(pad));
    cmds.insert_resource(mirror::Ctrl::new(ctrl));
}
