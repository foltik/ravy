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
use lib::lights::fixture::{
    ColoradoSolo, HydroSpot, HydroSpotGobo as Gobo, HydroSpotPrism as Prism, HydroSpotWheel1 as Wheel1,
    HydroSpotWheel2 as Wheel2, HydroSpotWheels, OutcastBeamwash, OutcastBeamwashRingPattern as RingPattern,
};
use lib::prelude::*;
use lib::rdm::{RdmWidget, Uid, pid};

use crate::blackout::{Blackout, homed, polar};
use crate::home::{Home, Rest, SNAP, quarter};
use crate::logic::{PAN_RANGE, TILT_RANGE, degrees, normalized};
use crate::sim::{Highlight, Movers};
use crate::ui::{AMBER, BLUE, DARK, GREEN, RED, knob, knob_centered, lamp, pane, pick};

/// Saved assignments, one `<fixture> <uid>` per line.
const PATCH_FILE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/patch.txt");

///////////////////////// PATCH /////////////////////////

/// Slot order, which is the order the show counts fixtures in.
pub const OUTCASTS: Range<usize> = 0..2;
pub const HYDROS: Range<usize> = 2..4;
pub const PARS: Range<usize> = 4..10;

/// The movers, which are the slots with a rest aim and a blackout window. They
/// come first, so a mover's slot index is its index here.
pub const MOVERS: [&str; 4] = ["OutcastBeamwash.0", "OutcastBeamwash.1", "HydroSpot.0", "HydroSpot.1"];

/// A mover's pan and tilt travel, in degrees signed by which way the axle turns
/// as the DMX value rises. Anything working in angles rather than in raw
/// normalized pan and tilt needs it: the two movers do not travel the same
/// distance and do not pan the same way.
pub fn travel(slot: usize) -> [f32; 2] {
    match OUTCASTS.contains(&slot) {
        true => OutcastBeamwash::TRAVEL,
        false => HydroSpot::TRAVEL,
    }
}

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

/// How often to retransmit an unchanged frame as a liveness check while idle.
const RETRY: Duration = Duration::from_secs(1);

/// How often to look for the widget while it's unplugged.
const REOPEN: Duration = Duration::from_millis(100);

enum Cmd {
    Frame(Vec<u8>),
    Scan,
    Identify(Uid, bool),
}

enum Ev {
    Widget(String),
    Lost,
    Scanning(bool),
    /// A responder answered discovery. Nothing is known about it yet.
    Found(Uid),
    /// What it says about itself, once interrogated.
    Info(Uid, Info),
}

/// The answers to DEVICE_INFO and DEVICE_MODEL_DESC.
struct Info {
    model: u16,
    desc: String,
    address: usize,
    footprint: usize,
}

/// An RDM responder on the wire.
pub struct Device {
    pub uid: Uid,
    pub manufacturer: u16,
    pub model: u16,
    /// The fixture's own name for itself, when we don't have a driver for it.
    pub desc: String,
    /// 1-based start address, or 0 until the fixture has answered.
    pub address: usize,
    pub footprint: usize,
    /// Seconds since the traffic light was last re-armed.
    pub blink: f32,
    pub identify: bool,
    /// Set while the panel is driving this fixture by hand.
    pub debug: Option<Debug>,
    /// Set while its blackout window is open in the panel.
    pub blackout: bool,
    /// Set while its rest aim is open in the panel.
    pub home: bool,
}

impl Device {
    /// A responder that has answered discovery and nothing else yet.
    fn new(uid: Uid) -> Self {
        Self {
            uid,
            manufacturer: uid.manufacturer(),
            model: 0,
            desc: String::new(),
            address: 0,
            footprint: 0,
            blink: f32::MAX,
            identify: false,
            debug: None,
            blackout: false,
            home: false,
        }
    }

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
    let mut reconnect = false;
    // Kept across an unplug: the show's addresses are the patch's, not RDM's,
    // so a widget that comes back has something to put on the wire at once.
    let mut last: Option<Vec<u8>> = None;
    // A scan blocks the wire for as long as the bus takes, so only the first
    // one is automatic: after that the patch already knows the rig, and when
    // to spend the show's wire on re-reading it is the panel's call.
    let mut scanned = false;
    loop {
        let Ok(mut port) = Port::open(path.as_deref()) else {
            // Drop stale traffic while unplugged, bail once the app is gone
            loop {
                match rx.try_recv() {
                    Ok(cmd) => {
                        if let Cmd::Frame(frame) = cmd {
                            last = Some(frame);
                        }
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => return,
                }
            }
            std::thread::sleep(REOPEN);
            continue;
        };

        // Firmware version alone; the panel writes the rest of the name.
        let version = match port.rdm() {
            Some(widget) => widget.params().map(|p| format!("v{}", p.firmware)).unwrap_or_default(),
            None => "no RDM".to_string(),
        };
        match reconnect {
            true => info!("DMX reconnected"),
            false => info!("DMX connected"),
        }
        reconnect = true;
        _ = tx.send(Ev::Widget(version));

        // The show goes back on the wire before anything else: waiting on the
        // app's next frame, or on a sweep that runs for seconds, is a rig
        // sitting on whatever a replugged widget came up with.
        if let Some(frame) = &last {
            _ = port.send(frame);
        }

        let mut rescan = !scanned && port.rdm().is_some();
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
                    Cmd::Scan => rescan = true,
                    Cmd::Identify(uid, on) => {
                        if let Some(widget) = port.rdm() {
                            widget.set(uid, pid::IDENTIFY_DEVICE, &[on as u8]);
                        }
                    }
                }
            }

            if let Some(frame) = frame.or_else(|| last.take()) {
                if !port.send(&frame) {
                    error!("DMX disconnected");
                    _ = tx.send(Ev::Lost);
                    break;
                }
                last = Some(frame);
            }

            if rescan && let Some(widget) = port.rdm() {
                rescan = false;
                scanned = true;
                scan(widget, &tx);
            }
        }
    }
}

/// Discover the wire, then interrogate what answered, publishing devices as
/// they turn up and filling them in once they speak. Blocks for as long as the
/// bus takes: the wire is shared, so nothing else moves meanwhile.
fn scan(widget: &mut RdmWidget, tx: &Sender<Ev>) {
    _ = tx.send(Ev::Scanning(true));
    let uids = widget.discover(&mut |uid| _ = tx.send(Ev::Found(uid)));
    info!("discovered {} rdm devices", uids.len());
    for uid in uids {
        if let Some(info) = interrogate(widget, uid) {
            _ = tx.send(Ev::Info(uid, info));
        }
    }
    _ = tx.send(Ev::Scanning(false));
}

fn interrogate(widget: &mut RdmWidget, uid: Uid) -> Option<Info> {
    // DEVICE_INFO carries model, footprint and address in one GET.
    let info = widget.get(uid, pid::DEVICE_INFO, &[]).filter(|pd| pd.len() >= 19)?;
    let u16be = |i: usize| ((info[i] as u16) << 8) | info[i + 1] as u16;
    let desc = widget
        .get(uid, pid::DEVICE_MODEL_DESC, &[])
        .map(|pd| pd.iter().map(|&b| if b == 0 { ' ' } else { b as char }).collect::<String>())
        .unwrap_or_default();
    Some(Info {
        model: u16be(2),
        desc: desc.trim().to_string(),
        address: u16be(14) as usize,
        footprint: u16be(10) as usize,
    })
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
            Ev::Found(uid) => dmx.devices.push(Device::new(uid)),
            Ev::Info(uid, info) => {
                if let Some(device) = dmx.devices.iter_mut().find(|d| d.uid == uid) {
                    device.model = info.model;
                    device.desc = info.desc;
                    device.address = info.address;
                    device.footprint = info.footprint;
                }
            }
            Ev::Scanning(true) => {
                dmx.scanning = true;
                dmx.devices.clear();
            }
            Ev::Scanning(false) => {
                dmx.scanning = false;
                // Address order, not the order discovery happened to find them.
                dmx.devices.sort_by_key(|d| d.address);
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
    /// Its own wheel travel, since the panel commands slots the show did not.
    wheels: HydroSpotWheels,
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
        Some(Self { hue: 0.0, light, wheels: default() })
    }
}

/// A pan or tilt knob in degrees off home, which is what the blackout window
/// is set in. The fixture itself is driven with the normalized travel.
fn aim(ui: &mut egui::Ui, label: &str, value: &mut f32, travel: f32) {
    let mut angle = degrees(*value, travel);
    if knob(ui, label, &mut angle, -travel / 2.0..=travel / 2.0) {
        *value = normalized(angle, travel);
    }
}

/// The knobs worth having by hand, per fixture type. `travel` is the pan and
/// tilt of whichever fixture this is, so the aim knobs read in its own degrees.
fn knobs(ui: &mut egui::Ui, debug: &mut Debug, travel: [f32; 2]) {
    let Debug { hue, light, .. } = debug;
    let (pan, tilt) = (travel[0], travel[1]);
    match light {
        Light::Par(l) => {
            knob(ui, "dimmer", &mut l.alpha, 0.0..=1.0);
            knob(ui, "hue", hue, 0.0..=1.0);
            knob(ui, "zoom", &mut l.zoom, 0.0..=1.0);
            knob(ui, "dim speed", &mut l.dimmer_speed, 0..=255);
            l.color = Rgb::hsv(*hue, 1.0, 1.0).with_w();
        }
        Light::Wash(l) => {
            knob(ui, "ring", &mut l.ring_alpha, 0.0..=1.0);
            knob(ui, "beam", &mut l.center_alpha, 0.0..=1.0);
            knob(ui, "hue", hue, 0.0..=1.0);
            aim(ui, "pitch", &mut l.pitch, tilt);
            aim(ui, "yaw", &mut l.yaw, pan);
            knob(ui, "zoom", &mut l.zoom, 0.0..=1.0);
            let patterns = RingPattern::ALL.map(|p| (p, p.name()));
            pick(ui, "pattern", &mut l.ring_pattern, &patterns);
            knob(ui, "pattern speed", &mut l.ring_pattern_speed, 0.0..=1.0);
            // Ring and centre are dimmed apart but hold the one colour.
            l.ring = Rgb::hsv(*hue, 1.0, 1.0).with_w();
            l.center = l.ring;
        }
        Light::Spot(l) => {
            knob(ui, "dimmer", &mut l.alpha, 0.0..=1.0);
            aim(ui, "pitch", &mut l.pitch, tilt);
            aim(ui, "yaw", &mut l.yaw, pan);
            let one: Vec<_> = Wheel1::ALL.iter().map(|w| (*w, w.name())).collect();
            let two: Vec<_> = Wheel2::ALL.iter().map(|w| (*w, w.name())).collect();
            pick(ui, "color 1", &mut l.color, &one);
            pick(ui, "color 2", &mut l.color2, &two);
            let gobos: Vec<_> = Gobo::ALL.iter().map(|g| (*g, g.name())).collect();
            pick(ui, "gobo", &mut l.gobo, &gobos);
            // The fixture will not blank itself while a colour wheel crosses
            // to a slot that is not its neighbour, so the sweep shows unless
            // we cut it.
            ui.checkbox(&mut l.color_mask, "blank while turning");
            ui.checkbox(&mut l.gobo_mask, "blank while gobo turning");
            knob(ui, "gobo shake", &mut l.gobo_shake, 0.0..=1.0);
            pick(ui, "prism", &mut l.prism, &[
                (Prism::Off, "off"),
                (Prism::Linear, "linear"),
                (Prism::Circular, "circular"),
            ]);
            knob_centered(ui, "prism rot", &mut l.prism_rot, -1.0..=1.0);
            knob(ui, "frost", &mut l.frost, 0.0..=1.0);
            knob(ui, "frost 2", &mut l.frost2, 0.0..=1.0);
            knob(ui, "zoom", &mut l.zoom, 0.0..=1.0);
            knob(ui, "focus", &mut l.focus, 0.0..=1.0);
        }
    }
}

/// A device the panel has taken over: nothing the show wrote for it leaves,
/// and a debug row puts its own values there instead. Its blackout window
/// still applies, so sweeping a head by hand shows what the window does.
///
/// A mover with its rest aim open is parked there at full instead, since the
/// point of setting a rest aim is seeing where it lands.
pub fn force(
    mut dmx: ResMut<Dmx>,
    patch: Res<Patch>,
    blackout: Res<Blackout>,
    home: Res<Home>,
    movers: Res<Movers>,
    time: Res<Time>,
    mut universe: ResMut<Universe>,
) {
    for device in &mut dmx.devices {
        if device.debug.is_none() && !device.identify && !device.home {
            continue;
        }
        let start = device.address.saturating_sub(1).min(universe.0.len());
        let end = (start + device.footprint).min(universe.0.len());
        universe.0[start..end].fill(0);

        let slot = patch.slots.iter().position(|s| s.uid == Some(device.uid));
        let parked = slot.filter(|slot| device.home && *slot < MOVERS.len());
        if let Some(slot) = parked {
            park(&home.movers[slot], slot, device.address, &mut universe);
            continue;
        }
        let Some(debug) = &mut device.debug else { continue };
        match &debug.light {
            Light::Par(l) => place(l, device.address, &mut universe),
            Light::Wash(l) => {
                let mut light = *l;
                if let Some(slot) = slot.filter(|slot| *slot < MOVERS.len()) {
                    blackout.wash(slot, &home.movers[slot], movers.aim[slot], &mut light);
                }
                place(&light, device.address, &mut universe);
            }
            Light::Spot(l) => {
                let mut light = *l;
                if let Some(slot) = slot.filter(|slot| *slot < MOVERS.len()) {
                    blackout.spot(slot, &home.movers[slot], movers.aim[slot], &mut light);
                }
                // The panel commands slots of its own, so it carries its own
                // travel rather than reading the show's.
                debug.wheels.mask(&mut light, time.delta_secs());
                place(&light, device.address, &mut universe);
            }
        }
    }
}

/// Sit a mover at its rest aim, lit white and open, so the aim being set can
/// actually be seen. Flipping an axle turns it round whole, so the head moves
/// as soon as the button is pressed.
///
/// The blackout window is deliberately not applied: a rest aim inside one would
/// otherwise be invisible, which is the case most worth looking at.
fn park(rest: &Rest, slot: usize, address: usize, universe: &mut Universe) {
    let travel = travel(slot);
    let (rest_pitch, rest_yaw) = rest.aim();
    let (pitch, yaw) = (normalized(rest_pitch, travel[1]), normalized(rest_yaw, travel[0]));
    match OUTCASTS.contains(&slot) {
        true => place(
            &OutcastBeamwash {
                pitch,
                yaw,
                zoom: rest.zoom,
                ring: Rgbw::WHITE,
                center: Rgbw::WHITE,
                ..default()
            },
            address,
            universe,
        ),
        false => place(
            &HydroSpot { pitch, yaw, zoom: rest.zoom, alpha: 1.0, ..default() },
            address,
            universe,
        ),
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

/// Wide enough for the fixture name and every button on its row.
const WIDTH: f32 = 540.0;

/// How long the traffic light stays lit, and the dark gap that follows it.
const BLINK_ON: f32 = 0.05;
const BLINK_OFF: f32 = 0.04;

pub fn draw(
    mut ctxs: EguiContexts,
    mut dmx: ResMut<Dmx>,
    mut patch: ResMut<Patch>,
    mut blackout: ResMut<Blackout>,
    mut home: ResMut<Home>,
    mut highlight: ResMut<Highlight>,
    movers: Res<Movers>,
) -> Result {
    let ctx = ctxs.ctx_mut()?;
    let dmx = &mut *dmx;
    let patch = patch.as_mut();
    // Whichever slot the assign dropdowns are pointing at, for the sim to ring.
    let mut pointed = None;
    // The movers whose window is open, which is when the sim draws its volume.
    let mut showing = [false; MOVERS.len()];

    let panel = pane(ctx, "DMX", egui::Align2::RIGHT_TOP, egui::Vec2::ZERO);
    panel.default_width(WIDTH).show(ctx, |ui| {
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
                if dmx.scanning {
                    ui.spinner();
                }
                let channels: usize = dmx.devices.iter().map(|d| d.footprint).sum();
                ui.weak(format!("{} devices across {channels} channels", dmx.devices.len()));
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
            // Only the movers have a blackout window; the pars slot after them.
            let assigned = patch.slots.iter().position(|s| s.uid == Some(device.uid));
            let mover = assigned.filter(|slot| *slot < MOVERS.len());
            ui.horizontal(|ui| {
                lamp(ui, match (device.identify, device.debug.is_some(), device.blink < BLINK_ON) {
                    (true, ..) => AMBER,
                    (_, true, _) => BLUE,
                    (.., true) => GREEN,
                    _ => DARK,
                });
                // A device that has answered discovery but not yet been asked
                // what it is stands in with its UID until the answers land.
                ui.monospace(match device.address {
                    0 => format!("{:>3}", "??"),
                    address => format!("{address:>3}"),
                })
                .on_hover_text(match device.footprint {
                    0 => device.uid.to_string(),
                    footprint => format!("{} — {footprint} ch", device.uid),
                });
                match kind {
                    Some(kind) => _ = ui.label(kind),
                    None if device.desc.is_empty() => _ = ui.weak(device.uid.to_string()),
                    None => _ = ui.weak(&device.desc),
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    pointed = pointed.or(assign(ui, device, patch));
                    if mover.is_some() && ui.selectable_label(device.blackout, "blackout").clicked() {
                        device.blackout = !device.blackout;
                    }
                    if mover.is_some() && ui.selectable_label(device.home, "home").clicked() {
                        device.home = !device.home;
                    }
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
                // A fixture with no slot has no travel to read; the generic
                // figures are the best that can be said for it.
                let travel = mover.map_or([PAN_RANGE, TILT_RANGE], travel);
                knobs(ui, debug, travel);
            }
            if let (true, Some(slot)) = (device.home, mover) {
                rest(ui, &mut home, slot);
            }
            if let (true, Some(slot)) = (device.blackout, mover) {
                showing[slot] = true;
                window(ui, &mut blackout, &home.movers[slot], slot, movers.aim[slot]);
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

    for (slot, show) in showing.into_iter().enumerate() {
        for zone in &mut blackout.movers[slot] {
            if zone.show != show {
                zone.show = show;
            }
        }
    }

    let pointed = pointed.map(|slot| patch.slots[slot].name.clone());
    if highlight.0 != pointed {
        highlight.0 = pointed;
    }
    Ok(())
}

/// One mover's rest aim: where it parks with nothing to do, and what the keyed
/// patterns are written against. Quarter turns only, so the axles keep even
/// room to work in either direction.
fn rest(ui: &mut egui::Ui, home: &mut ResMut<Home>, slot: usize) {
    let mut at: Rest = home.movers[slot];
    let (mut reload, mut save) = (false, false);

    let travel = travel(slot);
    ui.indent(("home", slot), |ui| {
        turns(ui, "pitch", &mut at.pitch, travel[1], &mut at.flip[0]);
        turns(ui, "yaw", &mut at.yaw, travel[0], &mut at.flip[1]);
        ui.horizontal(|ui| {
            ui.label("zoom");
            if ui.selectable_label(at.zoom < 0.5, "narrow").clicked() {
                at.zoom = 0.0;
            }
            if ui.selectable_label(at.zoom >= 0.5, "wide").clicked() {
                at.zoom = 1.0;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                save = ui.button("Save").clicked();
                reload = ui.button("Reload").clicked();
            });
        });
    });

    if at != home.movers[slot] {
        home.movers[slot] = at.snapped(travel);
    }
    if reload {
        home.reload();
    }
    if save && let Err(e) = home.save() {
        warn!("failed to save home: {e}");
    }
}

/// The quarter turns inside an axle's travel, as a row to pick from, and
/// whether patterns work backwards from the one picked.
fn turns(ui: &mut egui::Ui, label: &str, value: &mut f32, travel: f32, flip: &mut bool) {
    ui.horizontal(|ui| {
        ui.label(label);
        let steps = (travel.abs() / 2.0 / SNAP) as i32;
        for step in -steps..=steps {
            let angle = quarter(step as f32 * SNAP, travel);
            if ui.selectable_label(*value == angle, format!("{angle:.0}")).clicked() {
                *value = angle;
            }
        }
        let hint = "turn the axle round, aim and all";
        if ui.selectable_label(*flip, "flip").on_hover_text(hint).clicked() {
            *flip = !*flip;
        }
    });
}

/// One mover's blackout window: the patch of the sphere around it that fades,
/// how far down, and whether the sim is drawing the volume it covers. Pitch is
/// degrees off straight down, bearing is round from where the head rests.
///
/// `aim` is where the head is really pointing, so the window can say what it is
/// worth there.
fn window(
    ui: &mut egui::Ui,
    blackout: &mut ResMut<Blackout>,
    rest: &Rest,
    slot: usize,
    aim: Option<Vec3>,
) {
    let mut zones = blackout.movers[slot];
    let (mut reload, mut save) = (false, false);

    let travel = travel(slot);
    ui.indent(("blackout", slot), |ui| {
        // Where the head has actually got to, in the numbers the windows are
        // written in.
        ui.weak(match aim {
            None => "nothing aimed yet".to_string(),
            Some(aim) => {
                let (tip, bearing) = polar(aim);
                format!("aimed {tip:.0} / {:.0}", homed(rest, bearing))
            }
        });
        for (n, zone) in zones.iter_mut().enumerate() {
            ui.push_id(n, |ui| {
                ui.horizontal(|ui| {
                    ui.label(format!("blackout {}", n + 1));
                    let hint = "draw what the fade reaches as well, around the window";
                    if ui.selectable_label(zone.show_fade, "fade").on_hover_text(hint).clicked() {
                        zone.show_fade = !zone.show_fade;
                    }
                    // What this window is worth where the head is. 1.00 is it
                    // not biting at this aim.
                    if let Some(aim) = aim {
                        ui.weak(format!("x{:.2}", zone.gain(rest, aim)));
                    }
                });
                // Pitch on the same signed scale the debug knob is set in, so
                // the two read the same; below zero is the far side of the
                // fixture.
                let tip = travel[1].abs() / 2.0;
                knob(ui, "pitch", &mut zone.pitch.at, -tip..=tip);
                knob(ui, "pitch arc", &mut zone.pitch.width, 0.0..=2.0 * tip);
                knob(ui, "pitch fade", &mut zone.pitch.fade, 0.0..=180.0);
                knob(ui, "bearing", &mut zone.yaw.at, -180.0..=180.0);
                knob(ui, "bearing arc", &mut zone.yaw.width, 0.0..=360.0);
                knob(ui, "bearing fade", &mut zone.yaw.fade, 0.0..=180.0);
                knob(ui, "level", &mut zone.level, 0.0..=1.0);
            });
        }
        // Inside a horizontal, or the right-to-left layout takes every pixel it
        // is allowed and drags the whole window out with it.
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                save = ui.button("Save").clicked();
                reload = ui.button("Reload").clicked();
            });
        });
    });

    // Writing the resource is what rebuilds the volume in the sim, so only
    // write it when something moved.
    if zones != blackout.movers[slot] {
        blackout.movers[slot] = zones;
    }
    if reload {
        blackout.reload();
    }
    if save && let Err(e) = blackout.save() {
        warn!("failed to save blackout: {e}");
    }
}

/// Which fixture in the scene this device drives. Returns the slot the pointer
/// is over, so the sim can ring the fixture about to be picked.
fn assign(ui: &mut egui::Ui, device: &Device, patch: &mut Patch) -> Option<usize> {
    let current = patch.slots.iter().position(|s| s.uid == Some(device.uid));
    let label = match current {
        Some(i) => patch.slots[i].name.clone(),
        None => "unassigned".to_string(),
    };
    let mut pointed = None;
    let combo =
        egui::ComboBox::from_id_salt(device.uid.to_string()).selected_text(label).show_ui(ui, |ui| {
            if ui.selectable_label(current.is_none(), "unassigned").clicked() {
                patch.unassign(device.uid);
            }
            let mut order: Vec<usize> = (0..patch.slots.len()).collect();
            order.sort_by_key(|&i| patch.slots[i].address);
            for i in order {
                let slot = &patch.slots[i];
                let label = format!("{:>3}  {}", slot.address, slot.name);
                let option = ui.selectable_label(current == Some(i), label);
                if option.hovered() {
                    pointed = Some(i);
                }
                if option.clicked() {
                    patch.assign(i, device);
                }
            }
        });
    // With the list shut, the box stands for what it is already assigned to.
    match combo.response.hovered() {
        true => pointed.or(current),
        false => pointed,
    }
}
