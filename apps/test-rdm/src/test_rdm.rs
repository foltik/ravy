use std::sync::Mutex;
use std::sync::mpsc::{Receiver, Sender, channel};

use lib::lights::fixture::{ColoradoSolo, HydroSpot, OutcastBeamwash};
use lib::prelude::*;
use lib::rdm::{RdmWidget, Uid, pid, pid_name};

/// Which fixture driver, if any, claims this device's stable RDM ids.
fn identify(mfr: u16, model: u16) -> Option<&'static str> {
    fn m<T: RdmDevice>(mfr: u16, model: u16) -> Option<&'static str> {
        (T::MANUFACTURER == mfr && T::MODEL == model)
            .then(|| std::any::type_name::<T>().rsplit("::").next().unwrap())
    }
    m::<OutcastBeamwash>(mfr, model).or_else(|| m::<ColoradoSolo>(mfr, model)).or_else(|| m::<HydroSpot>(mfr, model))
}

/// RDM browser/editor for fixtures behind the ENTTEC DMX USB PRO: discovers
/// the bus, dumps every attribute of every responder, and reconfigures DMX
/// address / personality / identify / device label.
#[derive(argh::FromArgs)]
struct Args {
    /// serial port path (default: auto-detect the first FTDI port)
    #[argh(option)]
    port: Option<String>,
    /// enable debug logging
    #[argh(switch, short = 'v')]
    debug: bool,
    /// enable trace logging
    #[argh(switch, short = 'V')]
    trace: bool,
}

fn main() -> Result {
    let args: Args = argh::from_env();

    // Open before starting the app so a missing widget fails fast.
    let widget = match &args.port {
        Some(path) => RdmWidget::open(path)?,
        None => RdmWidget::new()?,
    };

    let (cmd_tx, cmd_rx) = channel::<Cmd>();
    let (ev_tx, ev_rx) = channel::<Ev>();
    std::thread::spawn(move || worker(widget, cmd_rx, ev_tx));
    cmd_tx.send(Cmd::Discover).unwrap();

    App::new()
        .add_plugins(RavyPlugin { module: module_path!(), debug: args.debug, trace: args.trace })
        .add_systems(Startup, setup)
        .add_systems(Update, poll)
        .add_systems(EguiPrimaryContextPass, draw_ui)
        .insert_resource(Rdm { tx: cmd_tx, rx: Mutex::new(ev_rx) })
        .insert_resource(State::default())
        .run();
    Ok(())
}

fn setup(mut commands: Commands) {
    // Needed to draw the UI
    commands.spawn(Camera2d);
}

///////////////////////// WORKER /////////////////////////

enum Cmd {
    Discover,
    Refresh(Uid),
    Identify(Uid, bool),
    SetAddress(Uid, u16),
    SetPersonality(Uid, u8),
    SetLabel(Uid, String),
}

enum Ev {
    Log(String),
    Widget(String),
    Discovering(bool),
    Device(Box<Device>),
}

/// Everything we could learn about one responder.
#[derive(Default)]
struct Device {
    uid: String,
    raw_uid: Option<Uid>,

    manufacturer: String,
    model: String,
    software: String,
    label: Option<String>,

    // DEVICE_INFO
    protocol: String,
    model_id: u16,
    category: u16,
    footprint: u16,
    address: u16,
    personality: u8,
    personality_count: u8,
    subdevices: u16,
    sensor_count: u8,

    /// (n, name, footprint)
    personalities: Vec<(u8, String, u16)>,
    /// (channel, description)
    slots: Vec<(u16, String)>,
    /// (name, value)
    sensors: Vec<(String, String)>,
    /// (pid, name, value) for every supported PID
    params: Vec<(u16, String, String)>,

    identify: bool,
}

/// The worker thread owns the serial port: every RDM transaction blocks for
/// up to hundreds of ms, so none of it can run on the render thread.
fn worker(mut w: RdmWidget, rx: Receiver<Cmd>, tx: Sender<Ev>) {
    let log = |s: String| _ = tx.send(Ev::Log(s));

    if let Some(p) = w.params() {
        let serial = w.serial().unwrap_or_default();
        log(format!("widget fw {} serial {serial}, break {} mab {} rate {} pps", p.firmware, p.break_time, p.mab_time, p.rate));
        if p.firmware.starts_with("1.") {
            log("DMX-only 1.x firmware: RDM needs 2.x (see rdm.py flash)".into());
        }
        _ = tx.send(Ev::Widget(format!("ENTTEC DMX USB PRO — fw {} — serial {serial}", p.firmware)));
    } else {
        log("widget did not answer parameter query".into());
    }

    while let Ok(cmd) = rx.recv() {
        match cmd {
            Cmd::Discover => {
                _ = tx.send(Ev::Discovering(true));
                log("discovering...".into());
                let t0 = std::time::Instant::now();
                let uids = w.discover(&mut |uid| log(format!("  found {uid}")));
                log(format!("discovery: {} device(s) in {:.1}s", uids.len(), t0.elapsed().as_secs_f32()));
                for uid in uids {
                    log(format!("enumerating {uid}..."));
                    _ = tx.send(Ev::Device(Box::new(enumerate(&mut w, uid))));
                }
                _ = tx.send(Ev::Discovering(false));
            }
            Cmd::Refresh(uid) => _ = tx.send(Ev::Device(Box::new(enumerate(&mut w, uid)))),
            Cmd::Identify(uid, on) => {
                let ok = w.set(uid, pid::IDENTIFY_DEVICE, &[on as u8]);
                log(format!("{uid} identify {}: {}", if on { "on" } else { "off" }, if ok { "OK" } else { "FAILED" }));
            }
            Cmd::SetAddress(uid, addr) => {
                let ok = w.set(uid, pid::DMX_START_ADDRESS, &addr.to_be_bytes());
                log(format!("{uid} set address {addr}: {}", if ok { "OK" } else { "FAILED" }));
                _ = tx.send(Ev::Device(Box::new(enumerate(&mut w, uid))));
            }
            Cmd::SetPersonality(uid, n) => {
                let ok = w.set(uid, pid::DMX_PERSONALITY, &[n]);
                log(format!("{uid} set personality {n}: {}", if ok { "OK" } else { "FAILED" }));
                _ = tx.send(Ev::Device(Box::new(enumerate(&mut w, uid))));
            }
            Cmd::SetLabel(uid, label) => {
                let ok = w.set(uid, pid::DEVICE_LABEL, label.as_bytes());
                log(format!("{uid} set label {label:?}: {}", if ok { "OK" } else { "FAILED" }));
                _ = tx.send(Ev::Device(Box::new(enumerate(&mut w, uid))));
            }
        }
    }
}

fn ascii(pd: &[u8]) -> String {
    pd.iter().map(|&b| if b == 0 { ' ' } else { b as char }).collect::<String>().trim().to_string()
}

fn u16be(pd: &[u8], i: usize) -> u16 {
    ((pd[i] as u16) << 8) | pd[i + 1] as u16
}

/// Fetch everything the device will answer.
fn enumerate(w: &mut RdmWidget, uid: Uid) -> Device {
    let mut d = Device { uid: uid.to_string(), raw_uid: Some(uid), ..Default::default() };

    d.manufacturer = w.get(uid, pid::MANUFACTURER_LABEL, &[]).map(|pd| ascii(&pd)).unwrap_or_default();
    d.model = w.get(uid, pid::DEVICE_MODEL_DESC, &[]).map(|pd| ascii(&pd)).unwrap_or_default();
    d.software = w.get(uid, pid::SOFTWARE_VERSION_LABEL, &[]).map(|pd| ascii(&pd)).unwrap_or_default();
    d.label = w.get(uid, pid::DEVICE_LABEL, &[]).map(|pd| ascii(&pd));
    d.identify = w.get(uid, pid::IDENTIFY_DEVICE, &[]).map(|pd| pd.first() == Some(&1)).unwrap_or(false);

    if let Some(info) = w.get(uid, pid::DEVICE_INFO, &[])
        && info.len() >= 19
    {
        d.protocol = format!("{}.{}", info[0], info[1]);
        d.model_id = u16be(&info, 2);
        d.category = u16be(&info, 4);
        d.footprint = u16be(&info, 10);
        d.personality = info[12];
        d.personality_count = info[13];
        d.address = u16be(&info, 14);
        d.subdevices = u16be(&info, 16);
        d.sensor_count = info[18];
    }

    for n in 1..=d.personality_count {
        if let Some(pd) = w.get(uid, pid::DMX_PERSONALITY_DESC, &[n])
            && pd.len() >= 3
        {
            d.personalities.push((n, ascii(&pd[3..]), u16be(&pd, 1)));
        }
    }

    if let Some(pd) = w.get(uid, pid::SLOT_INFO, &[]) {
        for c in pd.chunks_exact(5) {
            let (off, ty, id) = (u16be(c, 0), c[2], u16be(c, 3));
            let kind = if ty == 0 { slot_id_name(id) } else { format!("secondary({ty:#04x})") };
            d.slots.push((off + 1, kind));
        }
    }

    for n in 0..d.sensor_count {
        let name = w
            .get(uid, pid::SENSOR_DEFINITION, &[n])
            .filter(|pd| pd.len() > 13)
            .map(|pd| ascii(&pd[13..]))
            .unwrap_or_else(|| format!("sensor {n}"));
        let value = w
            .get(uid, pid::SENSOR_VALUE, &[n])
            .filter(|pd| pd.len() >= 9)
            .map(|pd| {
                let v = i16::from_be_bytes([pd[1], pd[2]]);
                let (lo, hi) = (i16::from_be_bytes([pd[3], pd[4]]), i16::from_be_bytes([pd[5], pd[6]]));
                format!("{v} (range seen {lo}..{hi})")
            })
            .unwrap_or_else(|| "(no answer)".into());
        d.sensors.push((name, value));
    }

    // Every supported PID, decoded when we know how, raw hex otherwise.
    if let Some(pd) = w.get(uid, pid::SUPPORTED_PARAMETERS, &[]) {
        let pids: Vec<u16> = pd.chunks_exact(2).map(|c| u16be(c, 0)).collect();
        for p in pids {
            let name = match pid_name(p) {
                Some(name) => name.to_string(),
                // Manufacturer-specific PID: ask the device for its name.
                None if p >= 0x8000 => w
                    .get(uid, pid::PARAMETER_DESC, &p.to_be_bytes())
                    .filter(|pd| pd.len() > 20)
                    .map(|pd| ascii(&pd[20..]))
                    .unwrap_or_else(|| "(manufacturer specific)".into()),
                None => String::new(),
            };
            let value = match p {
                // Covered by dedicated sections above, or SET-only.
                pid::DMX_PERSONALITY_DESC
                | pid::SLOT_DESCRIPTION
                | pid::SENSOR_DEFINITION
                | pid::SENSOR_VALUE
                | pid::PARAMETER_DESC
                | pid::FACTORY_DEFAULTS
                | pid::RESET_DEVICE
                | pid::PERFORM_SELFTEST => "-".into(),
                _ => match w.get(uid, p, &[]) {
                    Some(pd) => decode_pid(p, &pd),
                    None => "(no answer)".into(),
                },
            };
            d.params.push((p, name, value));
        }
    }

    d
}

/// Human-readable value for a GET response, for the common PIDs.
fn decode_pid(p: u16, pd: &[u8]) -> String {
    match p {
        pid::DEVICE_INFO => format!("{} bytes", pd.len()),
        pid::DMX_START_ADDRESS if pd.len() >= 2 => u16be(pd, 0).to_string(),
        pid::DMX_PERSONALITY if pd.len() >= 2 => format!("{} of {}", pd[0], pd[1]),
        pid::IDENTIFY_DEVICE | pid::PAN_INVERT | pid::TILT_INVERT | pid::PAN_TILT_SWAP | pid::DISPLAY_INVERT
            if !pd.is_empty() =>
        {
            if pd[0] == 1 { "on" } else { "off" }.into()
        }
        pid::DEVICE_HOURS | pid::LAMP_HOURS | pid::LAMP_STRIKES | pid::DEVICE_POWER_CYCLES if pd.len() >= 4 => {
            u32::from_be_bytes([pd[0], pd[1], pd[2], pd[3]]).to_string()
        }
        pid::PRODUCT_DETAIL_ID_LIST => {
            pd.chunks_exact(2).map(|c| format!("{:#06x}", u16be(c, 0))).collect::<Vec<_>>().join(" ")
        }
        pid::MANUFACTURER_LABEL
        | pid::DEVICE_MODEL_DESC
        | pid::DEVICE_LABEL
        | pid::SOFTWARE_VERSION_LABEL
        | pid::BOOT_SOFTWARE_LABEL => ascii(pd),
        _ if pd.is_empty() => "(empty)".into(),
        _ => pd.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" "),
    }
}

/// E1.20 table C-2 slot ID definitions (subset).
fn slot_id_name(id: u16) -> String {
    match id {
        0x0001 => "intensity".into(),
        0x0002 => "intensity master".into(),
        0x0051 => "strobe".into(),
        0x0201 => "color wheel".into(),
        0x0202 => "red".into(),
        0x0203 => "green".into(),
        0x0204 => "blue".into(),
        0x0205 => "amber".into(),
        0x0206 => "white".into(),
        0x0401 => "zoom".into(),
        0x0501 => "macro/program".into(),
        0x0502 => "macro speed".into(),
        0xFFFF => "undefined".into(),
        _ => format!("{id:#06x}"),
    }
}

///////////////////////// STATE /////////////////////////

#[derive(Resource)]
struct Rdm {
    tx: Sender<Cmd>,
    rx: Mutex<Receiver<Ev>>,
}

/// Per-device UI edit state alongside the fetched attributes.
struct DeviceUi {
    d: Device,
    addr_edit: u16,
    pers_edit: u8,
    label_edit: String,
}

#[derive(Resource, Default)]
struct State {
    widget: String,
    discovering: bool,
    devices: Vec<DeviceUi>,
    log: Vec<String>,
}

fn poll(rdm: Res<Rdm>, mut s: ResMut<State>) {
    while let Ok(ev) = rdm.rx.lock().unwrap().try_recv() {
        match ev {
            Ev::Log(line) => {
                info!("{line}");
                s.log.push(line);
            }
            Ev::Widget(w) => s.widget = w,
            Ev::Discovering(b) => {
                s.discovering = b;
                if b {
                    s.devices.clear();
                }
            }
            Ev::Device(d) => {
                let ui = DeviceUi {
                    addr_edit: d.address,
                    pers_edit: d.personality,
                    label_edit: d.label.clone().unwrap_or_default(),
                    d: *d,
                };
                match s.devices.iter_mut().find(|x| x.d.uid == ui.d.uid) {
                    Some(x) => *x = ui,
                    None => s.devices.push(ui),
                }
            }
        }
    }
}

///////////////////////// UI /////////////////////////

fn draw_ui(mut ctxs: EguiContexts, mut s: ResMut<State>, rdm: Res<Rdm>) -> Result {
    let ctx = ctxs.ctx_mut()?;

    egui::SidePanel::left("bus").min_width(340.0).show(ctx, |ui| {
        ui.heading("RDM bus");
        ui.label(&s.widget);
        ui.horizontal(|ui| {
            if ui.add_enabled(!s.discovering, egui::Button::new("Rescan")).clicked() {
                _ = rdm.tx.send(Cmd::Discover);
            }
            if s.discovering {
                ui.spinner();
                ui.label("discovering...");
            }
        });
        ui.separator();
        egui::ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
            for line in &s.log {
                ui.monospace(line);
            }
        });
    });

    egui::CentralPanel::default().show(ctx, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| {
            if s.devices.is_empty() && !s.discovering {
                ui.label("No RDM devices found. Check fixture power/cabling and hit Rescan.");
            }
            for dev in &mut s.devices {
                let Some(uid) = dev.d.raw_uid else { continue };
                let title = format!("{} — {} {}", dev.d.uid, dev.d.manufacturer, dev.d.model);
                egui::CollapsingHeader::new(title).default_open(true).show(ui, |ui| {
                    // Reconfiguration controls
                    ui.horizontal(|ui| {
                        if ui.button("⟳ refresh").clicked() {
                            _ = rdm.tx.send(Cmd::Refresh(uid));
                        }
                        ui.separator();
                        let mut identify = dev.d.identify;
                        if ui.checkbox(&mut identify, "identify").changed() {
                            dev.d.identify = identify;
                            _ = rdm.tx.send(Cmd::Identify(uid, identify));
                        }
                        ui.separator();
                        ui.label("address");
                        ui.add(egui::DragValue::new(&mut dev.addr_edit).range(1..=512));
                        if ui.button("set").clicked() {
                            _ = rdm.tx.send(Cmd::SetAddress(uid, dev.addr_edit));
                        }
                        ui.separator();
                        ui.label("personality");
                        let current = dev
                            .d
                            .personalities
                            .iter()
                            .find(|(n, ..)| *n == dev.pers_edit)
                            .map(|(n, name, ch)| format!("{n}: {name} ({ch}ch)"))
                            .unwrap_or_else(|| dev.pers_edit.to_string());
                        egui::ComboBox::from_id_salt(&dev.d.uid).selected_text(current).show_ui(ui, |ui| {
                            for (n, name, ch) in &dev.d.personalities {
                                ui.selectable_value(&mut dev.pers_edit, *n, format!("{n}: {name} ({ch}ch)"));
                            }
                        });
                        if ui.button("set").clicked() {
                            _ = rdm.tx.send(Cmd::SetPersonality(uid, dev.pers_edit));
                        }
                    });
                    if dev.d.label.is_some() {
                        ui.horizontal(|ui| {
                            ui.label("label");
                            ui.text_edit_singleline(&mut dev.label_edit);
                            if ui.button("set").clicked() {
                                _ = rdm.tx.send(Cmd::SetLabel(uid, dev.label_edit.clone()));
                            }
                        });
                    }
                    ui.separator();

                    egui::Grid::new(format!("info-{}", dev.d.uid)).striped(true).show(ui, |ui| {
                        let mut row = |k: &str, v: String| {
                            ui.label(k);
                            ui.label(v);
                            ui.end_row();
                        };
                        row("software", dev.d.software.clone());
                        row("RDM protocol", dev.d.protocol.clone());
                        row("model id", format!("{:#06x}", dev.d.model_id));
                        if let Some(name) = identify(uid.manufacturer(), dev.d.model_id) {
                            row("driver", name.to_string());
                        }
                        row("category", format!("{:#06x}", dev.d.category));
                        row("DMX address", dev.d.address.to_string());
                        row(
                            "footprint",
                            format!(
                                "{} channels ({}-{})",
                                dev.d.footprint,
                                dev.d.address,
                                dev.d.address + dev.d.footprint.max(1) - 1
                            ),
                        );
                        row("personality", format!("{} of {}", dev.d.personality, dev.d.personality_count));
                        row("subdevices", dev.d.subdevices.to_string());
                        row("sensors", dev.d.sensor_count.to_string());
                    });

                    if !dev.d.personalities.is_empty() {
                        ui.collapsing("personalities", |ui| {
                            for (n, name, ch) in &dev.d.personalities {
                                let active = if *n == dev.d.personality { "  <-- active" } else { "" };
                                ui.monospace(format!("{n}: {name:<16} {ch:3}ch{active}"));
                            }
                        });
                    }
                    if !dev.d.slots.is_empty() {
                        ui.collapsing("slots", |ui| {
                            for (ch, kind) in &dev.d.slots {
                                ui.monospace(format!("ch {ch:3}: {kind}"));
                            }
                        });
                    }
                    if !dev.d.sensors.is_empty() {
                        ui.collapsing("sensors", |ui| {
                            for (name, value) in &dev.d.sensors {
                                ui.monospace(format!("{name}: {value}"));
                            }
                        });
                    }
                    ui.collapsing("all supported parameters", |ui| {
                        egui::Grid::new(format!("pids-{}", dev.d.uid)).striped(true).show(ui, |ui| {
                            for (p, name, value) in &dev.d.params {
                                ui.monospace(format!("{p:#06x}"));
                                ui.monospace(name);
                                ui.monospace(value);
                                ui.end_row();
                            }
                        });
                    });
                });
            }
        });
    });

    Ok(())
}
