use lib::midi::device::launch_control_xl::{self, LaunchControlXL};
use lib::prelude::*;

/// Tool for exploring a DMX universe and testing channel functions.
#[derive(argh::FromArgs)]
struct Args {
    /// enable debug logging
    #[argh(switch, short = 'v')]
    debug: bool,
    /// enable trace logging
    #[argh(switch, short = 'V')]
    trace: bool,
}

fn main() -> Result {
    let args: Args = argh::from_env();
    App::new()
        .add_plugins(RavyPlugin { module: module_path!(), debug: args.debug, trace: args.trace })
        .add_systems(Startup, setup)
        .add_systems(Update, (on_ctrl, render_ctrl, tick, render_lights).chain())
        .add_systems(EguiPrimaryContextPass, draw_ui)
        .insert_resource(Midi::new("Launch Control XL", LaunchControlXL::default()))
        .insert_resource(E131::new("10.16.4.1")?)
        .insert_resource(State::default().tap_mut(|s| {
            s.dmx.resize(256, 0);
            s.device.resize(1, 0);
        }))
        .run();
    Ok(())
}

fn setup(mut commands: Commands) {
    // Needed to draw the UI
    commands.spawn(Camera2d);
}

///////////////////////// STATE /////////////////////////

#[derive(Resource, Default)]
pub struct State {
    /// Time since the last `tick()` in seconds
    pub dt: f32,
    /// Total time elapsed since startup in seconds
    pub t: f32,
    /// Pattern period
    pub pd: f32,
    /// Pattern phase
    pub fr: f32,
    /// Whether to reset everything but the device channels to zeros
    pub persist: bool,
    /// Shift buttons
    pub shift: [bool; 2],

    /// Test device
    pub device: Vec<u8>,
    /// Test device start channel
    pub device_channel: usize,
    /// Test device type
    pub device_ty: DeviceType,

    /// DMX channel values
    pub dmx: Vec<u8>,
}

#[derive(Default, PartialEq)]
pub enum DeviceType {
    #[default]
    Manual,
    Spot,   // (6  /  6ch?) Saber Spot RGB
    Beam,   // (11 / 16ch?) Eliminator Stealth RGB
    Strobe, // (5  /  5ch?) Blizzard Max-L
}

///////////////////////// TICK /////////////////////////

pub fn tick(mut s: ResMut<State>, t: Res<Time>) {
    let pd = s.pd.lerp(10.0..0.25);
    s.fr += t.delta_secs() / pd;
    s.fr = s.fr.fract();
}

///////////////////////// LIGHTS /////////////////////////

pub fn render_lights(mut s: ResMut<State>, mut e131: ResMut<E131>) {
    if !s.persist {
        s.dmx.fill(0);
    }

    let rgbw = match s.t.floor() as u64 % 4 {
        0 => (255, 0, 0, 0),
        1 => (0, 255, 0, 0),
        2 => (0, 0, 255, 0),
        3 => (0, 0, 0, 255),
        _ => unreachable!(),
    };
    let rgb = match s.t.floor() as u64 % 3 {
        0 => (255, 0, 0),
        1 => (0, 255, 0),
        2 => (0, 0, 255),
        _ => unreachable!(),
    };

    match s.device_ty {
        DeviceType::Manual => {}
        DeviceType::Spot => {
            s.device[0] = rgbw.0;
            s.device[1] = rgbw.1;
            s.device[2] = rgbw.2;
            s.device[3] = rgbw.3;
            s.device[4] = 255;
        }
        DeviceType::Beam => {
            match s.t.floor() as u64 % 8 {
                0..4 => {
                    s.device[0] = s.t.tri(4.0).byte();
                    s.device[1] = 0;
                    s.device[2] = 0;
                    s.device[3] = 0;
                }
                4..8 => {
                    s.device[0] = 0;
                    s.device[1] = 0;
                    s.device[2] = s.t.tri(4.0).byte();
                    s.device[3] = 0;
                }
                _ => unreachable!(),
            }
            s.device[4] = rgbw.0;
            s.device[5] = rgbw.1;
            s.device[6] = rgbw.2;
            s.device[7] = rgbw.3;
            s.device[8] = 0;
            s.device[9] = 255; // dimmer
            s.device[10] = 255; // shutter
        }
        DeviceType::Strobe => {
            s.device[0] = 255;
            s.device[1] = 0;
            s.device[2] = rgb.0;
            s.device[3] = rgb.1;
            s.device[4] = rgb.2;
            s.device[5] = 0;
        }
    };

    let device = s.device.clone();
    let device_channels = s.device_channel..(s.device_channel + s.device.len());
    s.dmx[device_channels].copy_from_slice(&device);

    e131.send(&s.device);
}

///////////////////////// CTRL INPUT /////////////////////////

pub fn on_ctrl(mut s: ResMut<State>, mut ctrl: ResMut<Midi<LaunchControlXL>>) {
    use launch_control_xl::*;

    for input in ctrl.recv() {
        debug!("ctrl: {input:?}");

        let device_len = s.device.len();
        let shift = if s.shift[1] {
            16
        } else if s.shift[0] {
            4
        } else {
            1
        };

        match input {
            Input::SendSelect(false, true) => s.device.resize(device_len + shift, 0),
            Input::SendSelect(true, true) => s.device.resize(device_len.saturating_sub(shift), 0),

            Input::TrackSelect(false, true) => s.device_channel = s.device_channel.saturating_sub(shift),
            Input::TrackSelect(true, true) => {
                s.device_channel = (s.device_channel + shift).min(s.dmx.len() - s.device.len())
            }

            Input::SendA(i, fr) => {
                let i = i as usize;
                if i < s.device.len() {
                    s.device[i] = ((fr + 1.0) / 2.0).byte();
                }
            }
            Input::SendB(i, fr) => {
                let i = i as usize + 8;
                if i < s.device.len() {
                    s.device[i] = ((fr + 1.0) / 2.0).byte();
                }
            }
            Input::Pan(i, fr) => {
                let i = i as usize + 16;
                if i < s.device.len() {
                    s.device[i] = ((fr + 1.0) / 2.0).byte();
                }
            }

            Input::Device(true) => s.device_ty = DeviceType::Manual,
            Input::Mute(true) => {
                s.device.clear();
                s.device.resize(5, 0);
                s.device_ty = DeviceType::Spot;
            }
            Input::Solo(true) => {
                s.device.clear();
                s.device.resize(11, 0);
                s.device_ty = DeviceType::Beam;
            }
            Input::Record(true) => {
                s.device.clear();
                s.device.resize(6, 0);
                s.device_ty = DeviceType::Strobe;
            }

            Input::Slider(0, fr) => s.pd = (fr + 1.0) / 2.0,

            Input::Focus(0, true) => s.persist = !s.persist,

            Input::Control(0, b) => s.shift[0] = b,
            Input::Control(1, b) => s.shift[1] = b,

            _ => {}
        }
    }
}

///////////////////////// CTRL OUTPUT /////////////////////////

pub fn render_ctrl(s: Res<State>, mut ctrl: ResMut<Midi<LaunchControlXL>>) {
    use launch_control_xl::types::*;
    use launch_control_xl::*;

    ctrl.send(Output::Batch(vec![
        (
            Led::Device,
            Color::Amber,
            if s.device_ty == DeviceType::Manual {
                Brightness::High
            } else {
                Brightness::Off
            },
        ),
        (
            Led::Mute,
            Color::Amber,
            if s.device_ty == DeviceType::Spot { Brightness::High } else { Brightness::Off },
        ),
        (
            Led::Solo,
            Color::Amber,
            if s.device_ty == DeviceType::Beam { Brightness::High } else { Brightness::Off },
        ),
        (
            Led::Record,
            Color::Amber,
            if s.device_ty == DeviceType::Strobe {
                Brightness::High
            } else {
                Brightness::Off
            },
        ),
        (
            Led::Focus(0),
            Color::Red,
            if s.persist { Brightness::High } else { Brightness::Off },
        ),
        (
            Led::Control(0),
            if s.shift[0] { Color::Green } else { Color::Amber },
            Brightness::High,
        ),
        (
            Led::Control(1),
            if s.shift[1] { Color::Green } else { Color::Amber },
            Brightness::High,
        ),
    ]));
}

pub fn draw_ui(mut ctxs: EguiContexts, s: Res<State>) -> Result {
    let ctx = ctxs.ctx_mut()?;

    egui::CentralPanel::default().show(ctx, |ui| {
        let size = ui.available_size();
        let (_resp, painter) = ui.allocate_painter(size, egui::Sense::hover());

        let p = &painter;
        let (w0, h0) = (size.x, size.y);

        // bounds
        let w = w0 * 0.8;
        let h = h0 * 0.8;
        let x0 = (w0 - w) * 0.5;
        let y0 = (h0 - h) * 0.5;

        let n = s.dmx.len();
        ui::text(
            p,
            48.0,
            Rgbw::WHITE,
            x0 + w / 2.0,
            y0 / 2.0,
            format!("n={n} ch={} i={}", s.device.len(), s.device_channel),
        );

        let wmin = w.min(h);
        let woff = (w - wmin) / 2.0;

        let ns = (n as f32).sqrt().ceil() as usize;
        let s2 = (wmin / ns as f32) / 2.0;
        for sy in 0..ns {
            for sx in 0..ns {
                let i = sy * ns + sx;
                let x = x0 + woff + (sx as f32 * s2 * 2.0);
                let y = y0 + (sy as f32 * s2 * 2.0);

                if i < n {
                    let dmx = s.dmx[i];
                    let fr = (dmx as f32).map(0..255, 0..1);

                    // outline
                    let stroke = if i >= s.device_channel && i < s.device_channel + s.device.len() {
                        Rgbw::WHITE
                    } else {
                        Rgbw::BLACK
                    };
                    ui::rect_stroke(p, 4.0, stroke, x + s2, y + s2, s2 * 2.0 - 3.0, s2 * 2.0 - 3.0);
                    // fill
                    let h = fr * s2 * 2.0;
                    ui::rect(
                        p,
                        Rgbw(0.4, 0.4, 0.4, 0.0),
                        x + s2,
                        y + (s2 * 2.0) - (h / 2.0),
                        s2 * 2.0 - 4.0,
                        (h - 4.0).max(0.0),
                    );
                    // channel + value
                    ui::text(p, 12.0, Rgbw::WHITE, x + s2, y + s2 / 2.0, dmx);
                    if i >= s.device_channel && i < s.device_channel + s.device.len() {
                        let i = i - s.device_channel + 1;
                        ui::text(p, 12.0, Rgbw::WHITE, x + s2, y + s2, format!("ch={i}"));
                    }
                }
            }
        }
    });

    Ok(())
}
