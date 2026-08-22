//! The link to the rig: one worker thread owns the ENTTEC widget.
//!
//! The classic fixtures are addressed by hand and don't speak RDM, so the
//! wire carries DMX frames only and the panel lists the rig straight from
//! [`crate::lights`] instead of discovering it. What the panel does patch is
//! which world fixture each address drives: identify lights the real fixture
//! full white, and the assignment permutes [`Addressing`] so scene indices
//! (a DJ booth par, a specific mover) land on the right stands.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, TryRecvError, channel};
use std::time::Duration;

use lib::dmx::device::bar_rgb_18w::Bar;
use lib::dmx::device::beam_rgbw_60w::{Beam, BeamRing};
use lib::dmx::device::beam_rgbw_90w::BigBeam;
use lib::dmx::device::par_rgbw_12x3w::Par;
use lib::dmx::device::spider_rgbw_8x10w::Spider;
use lib::dmx::device::strobe_rgb_35w::Strobe;
use lib::prelude::*;

use crate::lights::{
    Addressing, BAR_ADDRS, BEAM_ADDRS, BIGBEAM_ADDRS, LAST_CH, Lights, PAR_ADDRS, SPIDER_ADDRS,
    STROBE_ADDR, place,
};
use crate::logic::RINGS;
use crate::ui::{AMBER, BLUE, DARK, GREEN, RED, knob, lamp, pane, pick};

/// Saved assignments, one `<kind>.<address order> <world index>` per line.
const PATCH_FILE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/patch.txt");

///////////////////////// WIDGET /////////////////////////

/// How often to retransmit an unchanged frame as a liveness check while idle.
const RETRY: Duration = Duration::from_secs(1);

/// How often to look for the widget while it's unplugged.
const REOPEN: Duration = Duration::from_millis(100);

enum Ev {
    Connected,
    Lost,
}

fn open(path: Option<&str>) -> Result<Enttec> {
    Ok(match path {
        Some(path) => Enttec::open(path)?,
        None => Enttec::new()?,
    })
}

/// The worker owns the port and hotplug: retry the port while it's unplugged.
fn worker(path: Option<String>, rx: Receiver<Vec<u8>>, tx: Sender<Ev>) {
    let mut reconnect = false;
    // Kept across an unplug, so a widget that comes back has something to put
    // on the wire at once.
    let mut last: Option<Vec<u8>> = None;
    loop {
        let Ok(mut enttec) = open(path.as_deref()) else {
            // Drop stale traffic while unplugged, bail once the app is gone
            loop {
                match rx.try_recv() {
                    Ok(frame) => last = Some(frame),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => return,
                }
            }
            std::thread::sleep(REOPEN);
            continue;
        };

        match reconnect {
            true => info!("DMX reconnected"),
            false => info!("DMX connected"),
        }
        reconnect = true;
        _ = tx.send(Ev::Connected);

        // The show goes back on the wire before anything else.
        if let Some(frame) = &last {
            enttec.send(frame);
        }

        loop {
            let first = match rx.recv_timeout(RETRY) {
                Ok(frame) => Some(frame),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => return,
            };

            // Frames supersede each other, so only the newest is worth the
            // wire. Idle retransmits the last one to notice an unplug.
            let frame = first.into_iter().chain(rx.try_iter()).last();
            if let Some(frame) = frame.or_else(|| last.take()) {
                enttec.send(&frame);
                if !enttec.connected() {
                    error!("DMX disconnected");
                    _ = tx.send(Ev::Lost);
                    break;
                }
                last = Some(frame);
            }
        }
    }
}

///////////////////////// RESOURCE /////////////////////////

/// One physical fixture, at its classic address, and the world fixture (the
/// index the show and the sim count in) it drives.
pub struct Slot {
    kind: &'static str,
    pub address: usize,
    pub footprint: usize,
    pub world: usize,
    /// Seconds since the traffic light was last re-armed.
    pub blink: f32,
    /// Set while the panel holds it full white to pick it out of the rig.
    pub identify: bool,
    /// Set while the panel is driving this fixture by hand.
    pub debug: Option<Debug>,
}

#[derive(Resource)]
pub struct Dmx {
    tx: Sender<Vec<u8>>,
    rx: Mutex<Receiver<Ev>>,
    pub connected: bool,
    pub slots: Vec<Slot>,
    /// Last frame handed to the widget, which only wants the changes.
    last: [u8; 512],
    /// Whether the widget has been given anything yet. Until it has, a black
    /// frame is still worth sending: it's what shuts the rig off.
    primed: bool,
}

fn slots() -> Vec<Slot> {
    fn all<D: DmxDevice + Default>(kind: &'static str, addresses: &[usize]) -> Vec<Slot> {
        addresses
            .iter()
            .enumerate()
            .map(|(world, &address)| Slot {
                kind,
                address,
                footprint: D::default().channels(),
                world,
                blink: f32::MAX,
                identify: false,
                debug: None,
            })
            .collect()
    }

    // Address order, which is how the panel reads.
    let mut slots = vec![];
    slots.extend(all::<Par>("Par", &PAR_ADDRS));
    slots.extend(all::<Beam>("Beam", &BEAM_ADDRS));
    slots.extend(all::<Strobe>("Strobe", &[STROBE_ADDR]));
    slots.extend(all::<Bar>("Bar", &BAR_ADDRS));
    slots.extend(all::<Spider>("Spider", &SPIDER_ADDRS));
    slots.extend(all::<BigBeam>("BigBeam", &BIGBEAM_ADDRS));
    load(&mut slots);
    slots
}

impl Dmx {
    /// Start the worker, which opens the widget whenever one is plugged in.
    /// Until then the show runs against the sim alone.
    pub fn open(path: Option<&str>) -> Self {
        let path = path.map(String::from);
        let (tx, frames) = channel();
        let (evs, rx) = channel();
        std::thread::spawn(move || worker(path, frames, evs));
        Self {
            tx,
            rx: Mutex::new(rx),
            connected: false,
            slots: slots(),
            last: [0; 512],
            primed: false,
        }
    }

    /// The address table the assignments amount to, for the encoder.
    pub fn addressing(&self) -> Addressing {
        let mut addrs = Addressing::default();
        for slot in &self.slots {
            match slot.kind {
                "Par" => addrs.pars[slot.world] = slot.address,
                "Beam" => addrs.beams[slot.world] = slot.address,
                "BigBeam" => addrs.bigbeams[slot.world] = slot.address,
                "Bar" => addrs.bars[slot.world] = slot.address,
                "Spider" => addrs.spiders[slot.world] = slot.address,
                _ => addrs.strobe = slot.address,
            }
        }
        addrs
    }
}

///////////////////////// PATCH /////////////////////////

/// Each slot's key in the patch file: its kind and place in address order,
/// which is stable however the worlds are shuffled.
fn keys(slots: &[Slot]) -> Vec<String> {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    slots
        .iter()
        .map(|slot| {
            let i = counts.entry(slot.kind).or_default();
            *i += 1;
            format!("{}.{}", slot.kind, *i - 1)
        })
        .collect()
}

fn save(slots: &[Slot]) -> std::io::Result<()> {
    let body: String =
        keys(slots).iter().zip(slots).map(|(key, slot)| format!("{key} {}\n", slot.world)).collect();
    std::fs::write(PATCH_FILE, body)
}

/// Apply saved assignments, falling back to the identity for any kind whose
/// saved worlds don't cover its fixtures exactly once.
fn load(slots: &mut [Slot]) {
    let Ok(text) = std::fs::read_to_string(PATCH_FILE) else { return };
    let saved: HashMap<&str, usize> = text
        .lines()
        .filter_map(|line| {
            let (key, world) = line.split_once(char::is_whitespace)?;
            Some((key, world.trim().parse().ok()?))
        })
        .collect();

    let keys = keys(slots);
    for (slot, key) in slots.iter_mut().zip(&keys) {
        if let Some(&world) = saved.get(key.as_str()) {
            slot.world = world;
        }
    }

    for kind in ["Par", "Beam", "BigBeam", "Bar", "Spider", "Strobe"] {
        let mut worlds: Vec<usize> =
            slots.iter().filter(|s| s.kind == kind).map(|s| s.world).collect();
        worlds.sort();
        if worlds.iter().enumerate().any(|(i, &w)| i != w) {
            warn!("patch: saved {kind} assignments aren't a permutation, ignoring");
            let mut i = 0;
            for slot in slots.iter_mut().filter(|s| s.kind == kind) {
                slot.world = i;
                i += 1;
            }
        }
    }
}

pub fn poll(mut dmx: ResMut<Dmx>) {
    let events: Vec<Ev> = dmx.rx.lock().unwrap().try_iter().collect();
    for ev in events {
        match ev {
            Ev::Connected => {
                dmx.connected = true;
                // The widget forgets its frame across a replug
                dmx.primed = false;
            }
            Ev::Lost => dmx.connected = false,
        }
    }
}

/// Hand the widget the frame, and blink whichever fixtures it moved.
pub fn output(mut dmx: ResMut<Dmx>, universe: Res<Universe>, time: Res<Time>) {
    let dmx = &mut *dmx;

    let dt = time.delta_secs();
    for slot in &mut dmx.slots {
        slot.blink += dt;
        let start = slot.address.saturating_sub(1).min(512);
        let range = start..(start + slot.footprint).min(512);
        // Re-arm only once the lamp has been through a full on/off, so a stream
        // of updates reads as a flicker instead of pinning it solid.
        if universe.0[range.clone()] != dmx.last[range] && slot.blink >= BLINK_ON + BLINK_OFF {
            slot.blink = 0.0;
        }
    }

    if !dmx.primed || universe.0 != dmx.last {
        dmx.primed = true;
        dmx.last = universe.0;
        _ = dmx.tx.send(universe.0[..LAST_CH].to_vec());
    }
}

///////////////////////// DEBUG /////////////////////////

/// A fixture driven straight from the panel, over whatever the show sent it.
pub struct Debug {
    hue: f32,
    hue2: f32,
    /// The pars carry no dimmer of their own, so the panel scales their colour.
    level: f32,
    light: Light,
}

#[derive(Clone, Copy)]
enum Light {
    Par(Par),
    Beam(Beam),
    BigBeam(BigBeam),
    Bar(Bar),
    Spider(Spider),
    Strobe(Strobe),
}

impl Debug {
    fn new(kind: &str) -> Option<Self> {
        let light = match kind {
            "Par" => Light::Par(default()),
            "Beam" => Light::Beam(default()),
            "BigBeam" => Light::BigBeam(default()),
            "Bar" => Light::Bar(default()),
            "Spider" => Light::Spider(default()),
            "Strobe" => Light::Strobe(default()),
            _ => return None,
        };
        Some(Self { hue: 0.0, hue2: 0.0, level: 1.0, light })
    }
}

/// Full white at full, for picking a fixture out of the rig by eye.
fn white(kind: &str) -> Option<Light> {
    Some(match kind {
        "Par" => Light::Par(Par { color: Rgbw::WHITE }),
        "Beam" => Light::Beam(Beam { alpha: 1.0, color: Rgbw::WHITE, ..default() }),
        "BigBeam" => Light::BigBeam(BigBeam { alpha: 1.0, color: Rgbw::WHITE, ..default() }),
        "Bar" => Light::Bar(Bar { alpha: 1.0, color: Rgbw::WHITE }),
        "Spider" => {
            Light::Spider(Spider { alpha: 1.0, color0: Rgbw::WHITE, color1: Rgbw::WHITE, ..default() })
        }
        "Strobe" => Light::Strobe(Strobe { alpha: 1.0, color: Rgb::WHITE }),
        _ => return None,
    })
}

/// The knobs worth having by hand, per fixture type.
fn knobs(ui: &mut egui::Ui, debug: &mut Debug) {
    let Debug { hue, hue2, level, light } = debug;
    match light {
        Light::Par(l) => {
            knob(ui, "dimmer", level, 0.0..=1.0);
            knob(ui, "hue", hue, 0.0..=1.0);
            l.color = (Rgb::hsv(*hue, 1.0, 1.0) * *level).with_w();
        }
        Light::Beam(l) => {
            knob(ui, "dimmer", &mut l.alpha, 0.0..=1.0);
            knob(ui, "hue", hue, 0.0..=1.0);
            knob(ui, "pitch", &mut l.pitch, 0.0..=1.0);
            knob(ui, "yaw", &mut l.yaw, 0.0..=1.0);
            knob(ui, "speed", &mut l.speed, 0.0..=1.0);
            let names: Vec<String> = RINGS.iter().map(|r| format!("{r:?}")).collect();
            let rings: Vec<(BeamRing, &str)> =
                RINGS.iter().copied().zip(names.iter().map(String::as_str)).collect();
            pick(ui, "ring", &mut l.ring, &rings);
            l.color = Rgb::hsv(*hue, 1.0, 1.0).with_w();
        }
        Light::BigBeam(l) => {
            knob(ui, "dimmer", &mut l.alpha, 0.0..=1.0);
            knob(ui, "hue", hue, 0.0..=1.0);
            knob(ui, "pitch", &mut l.pitch, 0.0..=1.0);
            knob(ui, "yaw", &mut l.yaw, 0.0..=1.0);
            knob(ui, "speed", &mut l.speed, 0.0..=1.0);
            knob(ui, "strobe", &mut l.strobe, 0.0..=1.0);
            l.color = Rgb::hsv(*hue, 1.0, 1.0).with_w();
        }
        Light::Bar(l) => {
            knob(ui, "dimmer", &mut l.alpha, 0.0..=1.0);
            knob(ui, "hue", hue, 0.0..=1.0);
            l.color = Rgb::hsv(*hue, 1.0, 1.0).with_w();
        }
        Light::Spider(l) => {
            knob(ui, "dimmer", &mut l.alpha, 0.0..=1.0);
            knob(ui, "hue 1", hue, 0.0..=1.0);
            knob(ui, "pos 1", &mut l.pos0, 0.0..=1.0);
            knob(ui, "hue 2", hue2, 0.0..=1.0);
            knob(ui, "pos 2", &mut l.pos1, 0.0..=1.0);
            l.color0 = Rgb::hsv(*hue, 1.0, 1.0).with_w();
            l.color1 = Rgb::hsv(*hue2, 1.0, 1.0).with_w();
        }
        Light::Strobe(l) => {
            knob(ui, "dimmer", &mut l.alpha, 0.0..=1.0);
            knob(ui, "hue", hue, 0.0..=1.0);
            l.color = Rgb::hsv(*hue, 1.0, 1.0);
        }
    }
}

/// A fixture the panel has taken over: nothing the show wrote for it leaves,
/// and its own values go there instead — into [`Lights`] as well, so the
/// assigned world fixture shows it in the sim. Identify outranks debug, and
/// lighting the world fixture it is assigned to is the point: if the sim's
/// fixture and the real one don't match, the assignment is wrong.
pub fn force(mut dmx: ResMut<Dmx>, mut lights: ResMut<Lights>, mut universe: ResMut<Universe>) {
    let lights = &mut *lights;
    for slot in &mut dmx.slots {
        let Some(light) = (match slot.identify {
            true => white(slot.kind),
            false => slot.debug.as_ref().map(|debug| debug.light),
        }) else {
            continue;
        };

        // Blank first: an address too high to encode should sit dark, not
        // carry whatever the show wrote.
        let start = slot.address.saturating_sub(1).min(universe.0.len());
        let end = (start + slot.footprint).min(universe.0.len());
        universe.0[start..end].fill(0);

        match light {
            Light::Par(l) => {
                lights.pars[slot.world] = l;
                place(&l, slot.address, &mut universe);
            }
            Light::Beam(l) => {
                lights.beams[slot.world] = l;
                place(&l, slot.address, &mut universe);
            }
            Light::BigBeam(l) => {
                lights.bigbeams[slot.world] = l;
                place(&l, slot.address, &mut universe);
            }
            Light::Bar(l) => {
                lights.bars[slot.world] = l;
                place(&l, slot.address, &mut universe);
            }
            Light::Spider(l) => {
                lights.spiders[slot.world] = l;
                place(&l, slot.address, &mut universe);
            }
            Light::Strobe(l) => {
                lights.strobe = l;
                place(&l, slot.address, &mut universe);
            }
        }
    }
}

///////////////////////// DRAW /////////////////////////

/// How long the traffic light stays lit, and the dark gap that follows it.
const BLINK_ON: f32 = 0.05;
const BLINK_OFF: f32 = 0.04;

/// The world fixture's name in the scene's counting.
fn world_name(kind: &str, world: usize, count: usize) -> String {
    match count {
        1 => kind.to_string(),
        _ => format!("{kind}.{world}"),
    }
}

pub fn draw(
    mut ctxs: EguiContexts,
    mut dmx: ResMut<Dmx>,
    mut highlight: ResMut<crate::sim::Highlight>,
) -> Result {
    let ctx = ctxs.ctx_mut()?;
    let dmx = &mut *dmx;
    // Whichever fixture the assign dropdowns are pointing at, for the sim to
    // outline.
    let mut pointed = None;

    let panel = pane(ctx, "DMX", egui::Align2::RIGHT_TOP, egui::Vec2::ZERO);
    panel.default_width(360.0).show(ctx, |ui| {
        ui.spacing_mut().item_spacing = egui::vec2(8.0, 6.0);

        ui.horizontal(|ui| {
            lamp(ui, match dmx.connected {
                true => GREEN,
                false => RED,
            });
            ui.label("DMX USB PRO");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let channels: usize = dmx.slots.iter().map(|s| s.footprint).sum();
                ui.weak(format!("{} fixtures across {channels} channels", dmx.slots.len()));
            });
        });

        ui.separator();
        for i in 0..dmx.slots.len() {
            let kind = dmx.slots[i].kind;
            let count = dmx.slots.iter().filter(|s| s.kind == kind).count();

            ui.horizontal(|ui| {
                let slot = &dmx.slots[i];
                lamp(ui, match (slot.identify, slot.debug.is_some(), slot.blink < BLINK_ON) {
                    (true, ..) => AMBER,
                    (_, true, _) => BLUE,
                    (.., true) => GREEN,
                    _ => DARK,
                });
                ui.monospace(format!("{:>3}", slot.address))
                    .on_hover_text(format!("{} ch", slot.footprint));
                ui.label(kind);

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if count > 1 {
                        pointed = pointed.or(assign(ui, &mut dmx.slots, i, count).map(|w| (kind, w)));
                    }
                    let slot = &mut dmx.slots[i];
                    let open = slot.debug.is_some();
                    if ui.selectable_label(open, "debug").clicked() {
                        slot.debug = match open {
                            true => None,
                            false => Debug::new(kind),
                        };
                    }
                    let hint = "hold this fixture full white, to find it in the rig";
                    if ui.selectable_label(slot.identify, "id").on_hover_text(hint).clicked() {
                        slot.identify = !slot.identify;
                    }
                });
            });
            if let Some(debug) = &mut dmx.slots[i].debug {
                knobs(ui, debug);
            }
        }

        ui.separator();
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Save").clicked()
                    && let Err(e) = save(&dmx.slots)
                {
                    warn!("failed to save patch: {e}");
                }
            });
        });
    });

    if highlight.0 != pointed {
        highlight.0 = pointed;
    }
    Ok(())
}

/// Which world fixture this address drives. Picking one trades with whichever
/// slot held it, so every world fixture always has exactly one address.
/// Returns the world the pointer is over, so the sim can outline the fixture
/// about to be picked.
fn assign(ui: &mut egui::Ui, slots: &mut [Slot], i: usize, count: usize) -> Option<usize> {
    let (kind, current) = (slots[i].kind, slots[i].world);
    let mut pointed = None;
    let combo = egui::ComboBox::from_id_salt(("assign", kind, i))
        .selected_text(world_name(kind, current, count))
        .show_ui(ui, |ui| {
            for world in 0..count {
                let option = ui.selectable_label(current == world, world_name(kind, world, count));
                if option.hovered() {
                    pointed = Some(world);
                }
                if option.clicked() && world != current {
                    if let Some(other) = slots.iter().position(|s| s.kind == kind && s.world == world)
                    {
                        slots[other].world = current;
                    }
                    slots[i].world = world;
                }
            }
        });
    // With the list shut, the box stands for what it is already assigned to.
    match combo.response.hovered() {
        true => pointed.or(Some(current)),
        false => pointed,
    }
}
