#![allow(mixed_script_confusables)]

use lib::gdtf::GdtfSystems;
use lib::midi::device::launch_control_xl::LaunchControlXL;
use lib::midi::device::launchpad_x::LaunchpadX;
use lib::prelude::*;

mod blackout;
mod dmx;
mod e131;
mod home;
mod lights;
mod logic;
mod mirror;
mod rig;
mod sim;
mod status;
mod ui;

/// Homerave: the kosmic rig and the classic rig on one ENTTEC universe, run
/// as one show from the kosmic control scheme.
#[derive(argh::FromArgs)]
struct Args {
    /// ENTTEC serial port path (default: auto-detect, simulate if missing)
    #[argh(option)]
    port: Option<String>,
    /// keep the ENTTEC connected but hold everything on it dark
    #[argh(switch)]
    mute_enttec: bool,
    /// come up in the test pattern (every fixture stepping through red,
    /// green, blue, white, rgbw once a second) until the first grid press
    #[argh(switch)]
    test: bool,
    /// set every RDM fixture on the ENTTEC to the address the patch wants it
    /// at, save the assignments, and exit
    #[argh(switch)]
    readdress: bool,
    /// E1.31 node to send the classic rig's channels to as well (default:
    /// none, the ENTTEC carries the lot)
    #[argh(option)]
    node: Option<String>,
    /// put only these classic fixtures on the node, e.g. BigBeam.2,BigBeam.3,
    /// holding every other channel at zero, and log their bytes once a second
    #[argh(option)]
    solo: Option<String>,
    /// run the show without a window: no sim, no panels, for a box with only
    /// the control surfaces and the wire on it
    #[argh(switch)]
    headless: bool,
    /// enable debug logging
    #[argh(switch, short = 'v')]
    debug: bool,
    /// enable trace logging
    #[argh(switch, short = 'V')]
    trace: bool,
}

fn main() -> Result {
    let args: Args = argh::from_env();
    if args.readdress {
        dmx::readdress(args.port.as_deref())?;
        return Ok(());
    }

    let mut app = App::new();
    app.add_systems(
        Update,
        (
            dmx::poll,
            logic::on_pad,
            logic::on_ctrl,
            logic::tick,
            logic::render_lights,
            dmx::force,
            e131::force,
            dmx::output,
            e131::output,
            logic::render_pad,
            logic::render_ctrl,
        )
            .chain()
            // The sim reads the frame this chain writes, debug overrides included.
            .before(GdtfSystems),
    );

    if args.headless {
        app.add_plugins(RavyHeadlessPlugin { module: module_path!(), debug: args.debug, trace: args.trace })
            // Normally the gdtf plugin's; rig::load still reads them.
            .init_resource::<Haze>()
            .init_resource::<Glow>()
            .add_systems(Startup, (setup, rig::load).chain());
    } else {
        app.add_plugins(RavyPlugin { module: module_path!(), debug: args.debug, trace: args.trace })
            // rig::load reads the resources setup makes, and Haze the gdtf plugin does.
            .add_systems(Startup, (setup, sim::setup, rig::load).chain())
            .add_systems(Update, sim::sync.after(e131::output).before(GdtfSystems))
            .add_systems(
                Update,
                (
                    sim::readdress,
                    sim::movers,
                    sim::blackout_volumes.after(sim::movers),
                    sim::beam_setup,
                    sim::bigbeam_setup,
                    sim::spider_setup,
                    sim::lamp_setup,
                    sim::outline,
                    sim::room_light,
                    sim::orbit,
                ),
            )
            // The ENTTEC panel owns the highlight; the node's fills in behind it.
            .add_systems(
                EguiPrimaryContextPass,
                (mirror::draw, status::draw, (dmx::draw, e131::draw).chain()),
            );
    }

    app.insert_resource(TestPattern(args.test));
    app.insert_resource(dmx::Dmx::open(args.port.as_deref(), args.mute_enttec));
    let solo = args.solo.iter().flat_map(|s| s.split(',')).map(String::from).collect();
    app.insert_resource(e131::Node::open(args.node.as_deref(), solo));
    app.run();
    Ok(())
}

/// Whether `--test` was given.
#[derive(Resource)]
struct TestPattern(bool);

fn setup(mut cmds: Commands, node: Res<e131::Node>, test: Res<TestPattern>) {
    cmds.insert_resource(Universe::default());
    let patch = dmx::Patch::default();
    patch.log();
    node.log();
    cmds.insert_resource(patch);
    cmds.init_resource::<blackout::Blackout>();
    cmds.init_resource::<home::Home>();
    cmds.init_resource::<rig::Trim>();
    cmds.init_resource::<sim::Room>();
    cmds.init_resource::<sim::Movers>();
    cmds.init_resource::<sim::Highlight>();
    cmds.insert_resource(logic::State::new(test.0));
    cmds.insert_resource(lights::Lights::default());
    cmds.insert_resource(lights::Wheels::default());

    let ctrl = Midi::new("Launch Control XL", LaunchControlXL);
    let pad = Midi::new("Launchpad X LPX MIDI", LaunchpadX::default());
    cmds.insert_resource(mirror::Pad::new(pad));
    cmds.insert_resource(mirror::Ctrl::new(ctrl));
}
