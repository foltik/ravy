//! The link to the rig: one worker thread owns the ENTTEC widget, since the
//! same serial port carries both the show's DMX frames and RDM traffic.
//!
//! RDM discovery answers what is really out there and at what address. The
//! [`Patch`] then says which discovered device drives which fixture in the
//! scene, and it is the only place addresses come from — the encoder and the
//! sim both read it.

use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, TryRecvError, channel};
use std::time::Duration;

use lib::gdtf::GdtfDevice;
use lib::lights::fixture::{ColoradoSolo, HydroSpot, OutcastBeamwash};
use lib::prelude::*;
use lib::rdm::{RdmWidget, Uid, pid};

use crate::ui::{AMBER, BLUE, DARK, GREEN, RED, knob, lamp};

/// Saved assignments, one `<fixture> <uid>` per line.
const PATCH_FILE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/patch.txt");

///////////////////////// PATCH /////////////////////////

/// Slot order, which is the order the show counts fixtures in.
pub const OUTCASTS: Range<usize> = 0..2;
pub const HYDROS: Range<usize> = 2..4;
pub const PARS: Range<usize> = 4..10;

/// One fixture in the scene, and the device on the wire driving it.
pub struct Slot {
    /// Matches the empty in `Kosmic.glb`, e.g. `ColoradoSolo.0`.
    pub name: String,
    /// RDM manufacturer and model of this fixture type.
    pub ids: (u16, u16),
    pub footprint: usize,
    /// 1-based DMX start address.
    pub address: usize,
    pub uid: Option<Uid>,
}

#[derive(Resource)]
pub struct Patch {
    pub slots: Vec<Slot>,
    /// Assignments from disk, reapplied after every scan.
    saved: HashMap<String, Uid>,
}

impl Default for Patch {
    fn default() -> Self {
        // Addresses as set by hand; a scan replaces them with what's real.
        let mut slots = vec![];
        slots.extend(slots_for::<OutcastBeamwash>(&[1, 38]));
        slots.extend(slots_for::<HydroSpot>(&[75, 97]));
        slots.extend(slots_for::<ColoradoSolo>(&[119, 136, 153, 170, 187, 204]));
        Self { slots, saved: read_saved() }
    }
}

fn slots_for<D: GdtfDevice + RdmDevice + Default>(addresses: &[usize]) -> Vec<Slot> {
    addresses
        .iter()
        .enumerate()
        .map(|(i, &address)| Slot {
            name: format!("{}.{i}", D::KIND),
            ids: (D::MANUFACTURER, D::MODEL),
            footprint: D::default().channels(),
            address,
            uid: None,
        })
        .collect()
}

impl Patch {
    pub fn slot(&self, name: &str) -> Option<&Slot> {
        self.slots.iter().find(|s| s.name == name)
    }

    /// Highest occupied channel, which is all of the frame worth sending.
    pub fn footprint(&self) -> usize {
        self.slots.iter().map(|s| s.address + s.footprint - 1).max().unwrap_or(0).min(512)
    }

    pub fn assign(&mut self, slot: usize, device: &Device) {
        self.unassign(device.uid);
        self.slots[slot].uid = Some(device.uid);
        self.slots[slot].address = device.address;
    }

    /// Drop a device's assignment. The slot keeps its address, so the show
    /// carries on driving whatever was last there.
    pub fn unassign(&mut self, uid: Uid) {
        for slot in &mut self.slots {
            if slot.uid == Some(uid) {
                slot.uid = None;
            }
        }
    }

    /// Bind saved assignments first, then fill what's left with devices of the
    /// right type in ascending address order.
    pub fn auto(&mut self, devices: &[Device]) {
        let Self { slots, saved } = self;
        let mut taken = HashSet::new();

        for slot in slots.iter_mut() {
            slot.uid = None;
            let Some(device) = saved.get(&slot.name).and_then(|uid| devices.iter().find(|d| d.uid == *uid))
            else {
                continue;
            };
            slot.uid = Some(device.uid);
            slot.address = device.address;
            taken.insert(device.uid);
        }

        let mut order: Vec<&Device> = devices.iter().collect();
        order.sort_by_key(|d| d.address);
        for slot in slots.iter_mut().filter(|s| s.uid.is_none()) {
            let Some(device) = order.iter().find(|d| d.ids() == slot.ids && !taken.contains(&d.uid)) else {
                continue;
            };
            slot.uid = Some(device.uid);
            slot.address = device.address;
            taken.insert(device.uid);
        }
    }

    pub fn save(&mut self) -> std::io::Result<()> {
        self.saved =
            self.slots.iter().filter_map(|s| Some((s.name.clone(), s.uid?))).collect();
        let body: String =
            self.slots.iter().filter_map(|s| Some(format!("{} {}\n", s.name, s.uid?))).collect();
        std::fs::write(PATCH_FILE, body)
    }
}

fn read_saved() -> HashMap<String, Uid> {
    let Ok(text) = std::fs::read_to_string(PATCH_FILE) else {
        return HashMap::new();
    };
    text.lines()
        .filter_map(|line| {
            let (name, uid) = line.split_once(char::is_whitespace)?;
            Some((name.to_string(), uid.trim().parse().ok()?))
        })
        .collect()
}

///////////////////////// WIDGET /////////////////////////

/// How often to retry the port while unplugged, and to retransmit an
/// unchanged frame as a liveness check while idle.
const RETRY: Duration = Duration::from_secs(1);

enum Cmd {
    Frame(Vec<u8>),
    Scan,
    Identify(Uid, bool),
}

enum Ev {
    Widget(String),
    Lost,
    Scanning(bool),
    Found(Device),
}

/// An RDM responder on the wire.
pub struct Device {
    pub uid: Uid,
    pub manufacturer: u16,
    pub model: u16,
    /// The fixture's own name for itself, when we don't have a driver for it.
    pub desc: String,
    pub address: usize,
    pub footprint: usize,
    /// Seconds since the traffic light was last re-armed.
    pub blink: f32,
    pub identify: bool,
    /// Set while the panel is driving this fixture by hand.
    pub debug: Option<Debug>,
}

impl Device {
    pub fn ids(&self) -> (u16, u16) {
        (self.manufacturer, self.model)
    }

    /// The fixture struct that drives this model, if we have one.
    pub fn engine(&self) -> Option<&'static str> {
        fn matches<D: GdtfDevice + RdmDevice>(ids: (u16, u16)) -> Option<&'static str> {
            (ids == (D::MANUFACTURER, D::MODEL)).then_some(D::KIND)
        }
        let ids = self.ids();
        matches::<OutcastBeamwash>(ids)
            .or_else(|| matches::<HydroSpot>(ids))
            .or_else(|| matches::<ColoradoSolo>(ids))
    }
}

/// Whichever transport the widget's firmware supports.
enum Port {
    Rdm(RdmWidget),
    Dmx(Enttec),
}

impl Port {
    fn open(path: Option<&str>) -> Result<Self> {
        let rdm = match path {
            Some(path) => RdmWidget::open(path),
            None => RdmWidget::new(),
        };
        match rdm {
            Ok(widget) => Ok(Self::Rdm(widget)),
            Err(e) => {
                let enttec = match path {
                    Some(path) => Enttec::open(path)?,
                    None => Enttec::new()?,
                };
                warn!("No RDM, DMX only: {e}");
                Ok(Self::Dmx(enttec))
            }
        }
    }

    fn rdm(&mut self) -> Option<&mut RdmWidget> {
        match self {
            Self::Rdm(widget) => Some(widget),
            Self::Dmx(_) => None,
        }
    }

    fn send(&mut self, frame: &[u8]) -> bool {
        match self {
            Self::Rdm(widget) => widget.send_dmx(frame).is_ok(),
            Self::Dmx(enttec) => {
                enttec.send(frame);
                enttec.connected()
            }
        }
    }
}

/// The worker owns the port: every RDM transaction blocks for up to hundreds
/// of ms, and a full discovery for seconds. It also owns hotplug: retry the
/// port while it's unplugged, and rescan whenever it shows back up.
fn worker(path: Option<String>, rx: Receiver<Cmd>, tx: Sender<Ev>) {
    loop {
        let Ok(mut port) = Port::open(path.as_deref()) else {
            // Drop stale traffic while unplugged, bail once the app is gone
            loop {
                match rx.try_recv() {
                    Ok(_) => continue,
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => return,
                }
            }
            std::thread::sleep(RETRY);
            continue;
        };

        // Firmware version alone; the panel writes the rest of the name.
        let version = match port.rdm() {
            Some(widget) => widget.params().map(|p| format!("v{}", p.firmware)).unwrap_or_default(),
            None => "no RDM".to_string(),
        };
        _ = tx.send(Ev::Widget(version));
        if let Some(widget) = port.rdm() {
            scan(widget, &tx);
        }

        let mut last: Option<Vec<u8>> = None;
        loop {
            let first = match rx.recv_timeout(RETRY) {
                Ok(cmd) => Some(cmd),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => return,
            };

            // Frames supersede each other, so only the newest is worth the
            // wire. Idle retransmits the last one to notice an unplug.
            let mut frame = None;
            for cmd in first.into_iter().chain(rx.try_iter()) {
                match cmd {
                    Cmd::Frame(f) => frame = Some(f),
                    Cmd::Scan => {
                        if let Some(widget) = port.rdm() {
                            scan(widget, &tx);
                        }
                    }
                    Cmd::Identify(uid, on) => {
                        if let Some(widget) = port.rdm() {
                            widget.set(uid, pid::IDENTIFY_DEVICE, &[on as u8]);
                        }
                    }
                }
            }

            if let Some(frame) = frame.or_else(|| last.take()) {
                if !port.send(&frame) {
                    warn!("Lost DMX widget");
                    _ = tx.send(Ev::Lost);
                    break;
                }
                last = Some(frame);
            }
        }
    }
}

fn scan(widget: &mut RdmWidget, tx: &Sender<Ev>) {
    _ = tx.send(Ev::Scanning(true));
    let uids = widget.discover(&mut |uid| info!("found {uid}"));
    for uid in uids {
        // DEVICE_INFO carries model, footprint and address in one GET.
        let Some(info) = widget.get(uid, pid::DEVICE_INFO, &[]).filter(|pd| pd.len() >= 19) else {
            continue;
        };
        let u16be = |i: usize| ((info[i] as u16) << 8) | info[i + 1] as u16;
        let desc = widget
            .get(uid, pid::DEVICE_MODEL_DESC, &[])
            .map(|pd| pd.iter().map(|&b| if b == 0 { ' ' } else { b as char }).collect::<String>())
            .unwrap_or_default();
        _ = tx.send(Ev::Found(Device {
            uid,
            manufacturer: uid.manufacturer(),
            model: u16be(2),
            desc: desc.trim().to_string(),
            address: u16be(14) as usize,
            footprint: u16be(10) as usize,
            blink: f32::MAX,
            identify: false,
            debug: None,
        }));
    }
    _ = tx.send(Ev::Scanning(false));
}

///////////////////////// RESOURCE /////////////////////////

#[derive(Resource)]
pub struct Dmx {
    tx: Sender<Cmd>,
    rx: Mutex<Receiver<Ev>>,
    pub widget: String,
    pub connected: bool,
    pub scanning: bool,
    pub devices: Vec<Device>,
    /// Last frame handed to the widget, which only wants the changes.
    last: [u8; 512],
    /// Whether the widget has been given anything yet. Until it has, a black
    /// frame is still worth sending: it's what shuts the rig off.
    primed: bool,
}

impl Dmx {
    /// Start the worker, which opens the widget (and discovery) whenever one
    /// is plugged in. Until then the show runs against the sim alone.
    pub fn open(path: Option<&str>) -> Self {
        let path = path.map(String::from);
        let (tx, cmds) = channel();
        let (evs, rx) = channel();
        std::thread::spawn(move || worker(path, cmds, evs));
        Self {
            tx,
            rx: Mutex::new(rx),
            widget: String::new(),
            connected: false,
            scanning: false,
            devices: vec![],
            last: [0; 512],
            primed: false,
        }
    }

    pub fn scan(&self) {
        _ = self.tx.send(Cmd::Scan);
    }
}

pub fn poll(mut dmx: ResMut<Dmx>, mut patch: ResMut<Patch>) {
    let events: Vec<Ev> = dmx.rx.lock().unwrap().try_iter().collect();
    for ev in events {
        match ev {
            Ev::Widget(name) => {
                dmx.widget = name;
                dmx.connected = true;
                // The widget forgets its frame across a replug
                dmx.primed = false;
            }
            Ev::Lost => dmx.connected = false,
            Ev::Found(device) => dmx.devices.push(device),
            Ev::Scanning(true) => {
                dmx.scanning = true;
                dmx.devices.clear();
            }
            Ev::Scanning(false) => {
                dmx.scanning = false;
                patch.auto(&dmx.devices);
            }
        }
    }
}

/// Hand the widget the frame, and blink whichever devices it moved.
pub fn output(mut dmx: ResMut<Dmx>, patch: Res<Patch>, universe: Res<Universe>, time: Res<Time>) {
    let dmx = &mut *dmx;

    let dt = time.delta_secs();
    for device in &mut dmx.devices {
        device.blink += dt;
        let start = device.address.saturating_sub(1).min(512);
        let range = start..(start + device.footprint).min(512);
        // Re-arm only once the lamp has been through a full on/off, so a stream
        // of updates reads as a flicker instead of pinning it solid.
        if universe.0[range.clone()] != dmx.last[range] && device.blink >= BLINK_ON + BLINK_OFF {
            device.blink = 0.0;
        }
    }

    if !dmx.primed || universe.0 != dmx.last {
        dmx.primed = true;
        dmx.last = universe.0;
        _ = dmx.tx.send(Cmd::Frame(universe.0[..patch.footprint()].to_vec()));
    }
}

///////////////////////// DEBUG /////////////////////////

/// A fixture driven straight from the panel, over whatever the show sent it.
pub struct Debug {
    hue: f32,
    light: Light,
}

enum Light {
    Par(ColoradoSolo),
    Wash(OutcastBeamwash),
    Spot(HydroSpot),
}

impl Debug {
    fn new(kind: &str) -> Option<Self> {
        let light = match kind {
            ColoradoSolo::KIND => Light::Par(default()),
            OutcastBeamwash::KIND => Light::Wash(default()),
            HydroSpot::KIND => Light::Spot(default()),
            _ => return None,
        };
        Some(Self { hue: 0.0, light })
    }
}

/// The knobs worth having by hand, per fixture type.
fn knobs(ui: &mut egui::Ui, debug: &mut Debug) {
    let Debug { hue, light } = debug;
    match light {
        Light::Par(l) => {
            knob(ui, "dimmer", &mut l.alpha, 0.0..=1.0);
            knob(ui, "hue", hue, 0.0..=1.0);
            knob(ui, "zoom", &mut l.zoom, 0.0..=1.0);
            l.color = Rgb::hsv(*hue, 1.0, 1.0).with_w();
        }
        Light::Wash(l) => {
            knob(ui, "dimmer", &mut l.ring_alpha, 0.0..=1.0);
            knob(ui, "hue", hue, 0.0..=1.0);
            knob(ui, "pitch", &mut l.pitch, 0.0..=1.0);
            knob(ui, "yaw", &mut l.yaw, 0.0..=1.0);
            knob(ui, "zoom", &mut l.zoom, 0.0..=1.0);
            // Ring and centre move together: one dimmer, one colour.
            l.center_alpha = l.ring_alpha;
            l.ring = Rgb::hsv(*hue, 1.0, 1.0).with_w();
            l.center = l.ring;
        }
        Light::Spot(l) => {
            knob(ui, "dimmer", &mut l.alpha, 0.0..=1.0);
            knob(ui, "color 1", &mut l.color, 0..=255);
            knob(ui, "color 2", &mut l.color2, 0..=255);
            knob(ui, "gobo", &mut l.gobo, 0..=255);
            knob(ui, "prism", &mut l.prism, 0..=255);
            knob(ui, "prism rot", &mut l.prism_rot, 0..=255);
            knob(ui, "frost", &mut l.frost, 0.0..=1.0);
            knob(ui, "frost 2", &mut l.frost2, 0.0..=1.0);
            knob(ui, "zoom", &mut l.zoom, 0.0..=1.0);
            knob(ui, "focus", &mut l.focus, 0.0..=1.0);
        }
    }
}

/// A device the panel has taken over: nothing the show wrote for it leaves,
/// and a debug row puts its own values there instead.
pub fn force(dmx: Res<Dmx>, mut universe: ResMut<Universe>) {
    for device in &dmx.devices {
        if device.debug.is_none() && !device.identify {
            continue;
        }
        let start = device.address.saturating_sub(1).min(universe.0.len());
        let end = (start + device.footprint).min(universe.0.len());
        universe.0[start..end].fill(0);

        match device.debug.as_ref().map(|d| &d.light) {
            Some(Light::Par(l)) => place(l, device.address, &mut universe),
            Some(Light::Wash(l)) => place(l, device.address, &mut universe),
            Some(Light::Spot(l)) => place(l, device.address, &mut universe),
            None => {}
        }
    }
}

/// Pack a fixture into the universe at a 1-based address. One addressed too
/// high for its footprint has nowhere to go, so it sits the frame out rather
/// than taking the universe with it.
pub fn place(light: &impl DmxDevice, address: usize, universe: &mut Universe) {
    let start = address.saturating_sub(1);
    if start + light.channels() <= universe.0.len() {
        light.encode(&mut universe.0[start..]);
    }
}

///////////////////////// DRAW /////////////////////////

/// How long the traffic light stays lit, and the dark gap that follows it.
const BLINK_ON: f32 = 0.05;
const BLINK_OFF: f32 = 0.04;

pub fn draw(mut ctxs: EguiContexts, mut dmx: ResMut<Dmx>, mut patch: ResMut<Patch>) -> Result {
    let ctx = ctxs.ctx_mut()?;
    let dmx = &mut *dmx;
    let patch = patch.as_mut();

    egui::Window::new("DMX").default_width(420.0).default_pos([760.0, 24.0]).show(ctx, |ui| {
        ui.spacing_mut().item_spacing = egui::vec2(8.0, 6.0);

        ui.horizontal(|ui| {
            lamp(ui, match dmx.connected {
                true => GREEN,
                false => RED,
            });
            ui.label(match dmx.widget.is_empty() {
                true => "DMX USB PRO".to_string(),
                false => format!("DMX USB PRO ({})", dmx.widget),
            });
            // Right-aligned so the count and Scan sit opposite the adapter.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add_enabled(dmx.connected && !dmx.scanning, egui::Button::new("Scan")).clicked() {
                    dmx.scan();
                }
                match dmx.scanning {
                    true => _ = ui.spinner(),
                    false => {
                        let channels: usize = dmx.devices.iter().map(|d| d.footprint).sum();
                        ui.weak(format!("{} devices across {channels} channels", dmx.devices.len()));
                    }
                }
            });
        });

        ui.separator();
        if dmx.devices.is_empty() {
            ui.weak(match dmx.scanning {
                true => "discovering...",
                false => "nothing on the wire",
            });
        }

        let tx = &dmx.tx;
        for device in &mut dmx.devices {
            let kind = device.engine();
            ui.horizontal(|ui| {
                lamp(ui, match (device.identify, device.debug.is_some(), device.blink < BLINK_ON) {
                    (true, ..) => AMBER,
                    (_, true, _) => BLUE,
                    (.., true) => GREEN,
                    _ => DARK,
                });
                ui.monospace(format!("{:>3}", device.address))
                    .on_hover_text(format!("{} — {} ch", device.uid, device.footprint));
                match kind {
                    Some(kind) => _ = ui.label(kind),
                    None => _ = ui.weak(&device.desc),
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    assign(ui, device, patch);
                    if let Some(kind) = kind {
                        let open = device.debug.is_some();
                        if ui.selectable_label(open, "debug").clicked() {
                            device.debug = match open {
                                true => None,
                                false => Debug::new(kind),
                            };
                        }
                    }
                    if ui.selectable_label(device.identify, "id").clicked() {
                        device.identify = !device.identify;
                        _ = tx.send(Cmd::Identify(device.uid, device.identify));
                    }
                });
            });
            if let Some(debug) = &mut device.debug {
                knobs(ui, debug);
            }
        }

        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("Assign").clicked() {
                patch.auto(&dmx.devices);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Save").clicked()
                    && let Err(e) = patch.save()
                {
                    warn!("failed to save patch: {e}");
                }
            });
        });
    });
    Ok(())
}

/// Which fixture in the scene this device drives.
fn assign(ui: &mut egui::Ui, device: &Device, patch: &mut Patch) {
    let current = patch.slots.iter().position(|s| s.uid == Some(device.uid));
    let label = match current {
        Some(i) => patch.slots[i].name.clone(),
        None => "unassigned".to_string(),
    };
    egui::ComboBox::from_id_salt(device.uid.to_string()).selected_text(label).show_ui(ui, |ui| {
        if ui.selectable_label(current.is_none(), "unassigned").clicked() {
            patch.unassign(device.uid);
        }
        for i in 0..patch.slots.len() {
            let name = patch.slots[i].name.clone();
            if ui.selectable_label(current == Some(i), name).clicked() {
                patch.assign(i, device);
            }
        }
    });
}
