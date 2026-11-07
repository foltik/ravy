//! The classic rig: its slots, and the E1.31 node that can carry them.
//!
//! The classic fixtures are addressed by hand and don't speak RDM, so the
//! panel lists the rig straight from [`crate::lights`] instead of discovering
//! it. What the panel does patch is which world fixture each address drives:
//! identify lights the real fixture full white, and the assignment permutes
//! [`Addressing`] so scene indices land on the right stands. The frame is the
//! one [`Universe`]; the node, when there is one, is sent its classic half.

use std::collections::HashMap;

use lib::dmx::device::bar_rgb_18w::Bar;
use lib::dmx::device::beam_rgbw_60w::{Beam, BeamRing};
use lib::dmx::device::beam_rgbw_90w::BigBeam;
use lib::dmx::device::hazer_hurricane_haze_4d::Hazer;
use lib::dmx::device::laser_scan_30w::{Laser, LaserColor};
use lib::dmx::device::par_rgbw_12x3w::Par;
use lib::dmx::device::spider_rgbw_8x10w::Spider;
use lib::dmx::device::strobe_rgb_35w::Strobe;
use lib::prelude::*;

use crate::dmx::place;
use crate::lights::{
    Addressing, BAR_ADDRS, BEAM_ADDRS, BIGBEAM_ADDRS, HAZER_ADDR, LASER_ADDR, LAST_CH, Lights,
    PAR_ADDRS, SPIDER_ADDRS, STROBE_ADDR, laser_circle,
};
use crate::sim::Highlight;
use crate::ui::{AMBER, BLUE, DARK, GREEN, RED, knob, lamp, pane, pick};

/// Saved assignments, one `<kind>.<address order> <world index>` per line.
const PATCH_FILE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/classic.txt");

/// How often the soloed slots' bytes are logged, in seconds.
const SOLO_LOG: f32 = 1.0;

/// The Beam ring colours worth a knob, in two rows of eight.
pub const RINGS: [BeamRing; 16] = [
    BeamRing::Off,
    BeamRing::Red,
    BeamRing::Green,
    BeamRing::Blue,
    BeamRing::Yellow,
    BeamRing::Purple,
    BeamRing::Teal,
    BeamRing::White,
    BeamRing::RedYellow,
    BeamRing::RedPurple,
    BeamRing::RedWhite,
    BeamRing::GreenYellow,
    BeamRing::GreenBlue,
    BeamRing::BluePurple,
    BeamRing::BlueWhite,
    BeamRing::Cycle,
];

///////////////////////// RESOURCE /////////////////////////

/// One physical fixture, at its classic address, and the world fixture (the
/// index the show and the sim count in) it drives.
pub struct Slot {
    pub kind: &'static str,
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
pub struct Node {
    /// The node's address, for the panel.
    pub ip: String,
    /// None when the socket could not be opened; the show runs against the
    /// sim alone.
    link: Option<E131>,
    pub slots: Vec<Slot>,
    /// Last frame seen, for the blink lamps.
    last: [u8; 512],
    /// World fixtures allowed on the wire; everything else is held at zero.
    /// For telling one fixture's trouble from its neighbours'.
    solo: Vec<String>,
    /// Seconds since the soloed slots' bytes were last logged.
    logged: f32,
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
    slots.extend(all::<Laser>("Laser", &[LASER_ADDR]));
    slots.extend(all::<Hazer>("Hazer", &[HAZER_ADDR]));
    slots.extend(all::<Spider>("Spider", &SPIDER_ADDRS));
    slots.extend(all::<BigBeam>("BigBeam", &BIGBEAM_ADDRS));
    load(&mut slots);
    slots
}

impl Node {
    /// No address means no wire: the classic rig runs against the sim alone.
    pub fn open(ip: Option<&str>, solo: Vec<String>) -> Self {
        let link = match ip.map(E131::new) {
            Some(Ok(link)) => Some(link),
            Some(Err(e)) => {
                warn!("No E1.31 out: {e}");
                None
            }
            None => None,
        };
        Self {
            ip: ip.unwrap_or("off").to_string(),
            link,
            slots: slots(),
            last: [0; 512],
            solo,
            logged: 0.0,
        }
    }

    /// Whether a slot's world fixture is on the wire.
    fn live(&self, slot: &Slot) -> bool {
        let count = self.slots.iter().filter(|s| s.kind == slot.kind).count();
        self.solo.is_empty() || self.solo.contains(&world_name(slot.kind, slot.world, count))
    }

    /// Every slot and the channels it holds, for the console.
    pub fn log(&self) {
        info!("classic rig, up to channel {LAST_CH} (E1.31 node {}):", self.ip);
        for slot in &self.slots {
            let count = self.slots.iter().filter(|s| s.kind == slot.kind).count();
            let last = slot.address + slot.footprint - 1;
            info!("  {:>3}..{:<3}  {}", slot.address, last, world_name(slot.kind, slot.world, count));
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
                "Laser" => addrs.laser = slot.address,
                "Hazer" => addrs.hazer = slot.address,
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

    for kind in ["Par", "Beam", "BigBeam", "Bar", "Spider", "Strobe", "Laser", "Hazer"] {
        let mut worlds: Vec<usize> =
            slots.iter().filter(|s| s.kind == kind).map(|s| s.world).collect();
        worlds.sort();
        if worlds.iter().enumerate().any(|(i, &w)| i != w) {
            warn!("classic patch: saved {kind} assignments aren't a permutation, ignoring");
            let mut i = 0;
            for slot in slots.iter_mut().filter(|s| s.kind == kind) {
                slot.world = i;
                i += 1;
            }
        }
    }
}

/// Send the node its half of the frame every tick, and blink whichever
/// fixtures it moved. Every tick: a node clocks its DMX line off the packets
/// it gets.
pub fn output(mut node: ResMut<Node>, universe: Res<Universe>, time: Res<Time>) {
    let node = &mut *node;

    let dt = time.delta_secs();
    for slot in &mut node.slots {
        slot.blink += dt;
        let start = slot.address.saturating_sub(1).min(512);
        let range = start..(start + slot.footprint).min(512);
        // Re-arm only once the lamp has been through a full on/off, so a stream
        // of updates reads as a flicker instead of pinning it solid.
        if universe.0[range.clone()] != node.last[range] && slot.blink >= BLINK_ON + BLINK_OFF {
            slot.blink = 0.0;
        }
    }

    node.last = universe.0;
    if let Some(link) = &mut node.link {
        link.send(&universe.0[..LAST_CH]);
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
    Laser(Laser),
    Hazer(Hazer),
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
            "Laser" => Light::Laser(laser_circle(LaserColor::RGB)),
            "Hazer" => Light::Hazer(default()),
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
        "Laser" => Light::Laser(laser_circle(LaserColor::RGB)),
        // Nothing to see on a hazer but the fan and a puff.
        "Hazer" => Light::Hazer(Hazer { fan: 1.0, haze: 1.0 }),
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
        Light::Laser(l) => {
            ui.checkbox(&mut l.on, "on");
            pick(ui, "color", &mut l.color, &[
                (LaserColor::RED, "red"),
                (LaserColor::GREEN, "green"),
                (LaserColor::BLUE, "blue"),
                (LaserColor::RGB, "rgb"),
            ]);
            knob(ui, "size", &mut l.size, 0.0..=1.0);
            knob(ui, "x", &mut l.x, 0.0..=1.0);
            knob(ui, "y", &mut l.y, 0.0..=1.0);
            knob(ui, "rotate", &mut l.rotate, 0.0..=1.0);
        }
        Light::Hazer(l) => {
            knob(ui, "haze", &mut l.haze, 0.0..=1.0);
            knob(ui, "fan", &mut l.fan, 0.0..=1.0);
        }
    }
}

/// A fixture the panel has taken over: nothing the show wrote for it leaves,
/// and its own values go there instead, into [`Lights`] as well so the
/// assigned world fixture shows it in the sim. Identify outranks debug, and
/// lighting the world fixture it is assigned to is the point: if the sim's
/// fixture and the real one don't match, the assignment is wrong. Then the
/// solo, which blanks every classic fixture it does not name.
pub fn force(mut node: ResMut<Node>, mut lights: ResMut<Lights>, mut universe: ResMut<Universe>, time: Res<Time>) {
    let node = &mut *node;
    let lights = &mut *lights;
    let universe = &mut *universe;
    for slot in &mut node.slots {
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
                place(&l, slot.address, universe);
            }
            Light::Beam(l) => {
                lights.beams[slot.world] = l;
                place(&l, slot.address, universe);
            }
            Light::BigBeam(l) => {
                lights.bigbeams[slot.world] = l;
                place(&l, slot.address, universe);
            }
            Light::Bar(l) => {
                lights.bars[slot.world] = l;
                place(&l, slot.address, universe);
            }
            Light::Spider(l) => {
                lights.spiders[slot.world] = l;
                place(&l, slot.address, universe);
            }
            Light::Strobe(l) => {
                lights.strobe = l;
                place(&l, slot.address, universe);
            }
            Light::Laser(l) => {
                lights.laser = l;
                place(&l, slot.address, universe);
            }
            Light::Hazer(l) => {
                lights.hazer = l;
                place(&l, slot.address, universe);
            }
        }
    }

    // Anything not soloed leaves as zeros, and what is soloed is logged as
    // it leaves, so the bytes on the wire can be read against the chart.
    if node.solo.is_empty() {
        return;
    }
    node.logged += time.delta_secs();
    let log = node.logged >= SOLO_LOG;
    for i in 0..node.slots.len() {
        let slot = &node.slots[i];
        let start = slot.address.saturating_sub(1).min(512);
        let range = start..(start + slot.footprint).min(512);
        match node.live(slot) {
            false => universe.0[range].fill(0),
            true if log => info!("{:>3} {}: {:?}", slot.address, slot.kind, &universe.0[range]),
            true => {}
        }
    }
    if log {
        node.logged = 0.0;
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

/// Stacked under the ENTTEC panel in the top right corner.
pub fn draw(
    mut ctxs: EguiContexts,
    mut node: ResMut<Node>,
    mut highlight: ResMut<Highlight>,
) -> Result {
    let ctx = ctxs.ctx_mut()?;
    let node = &mut *node;
    // Whichever fixture the assign dropdowns are pointing at, for the sim to
    // outline.
    let mut pointed: Option<(&str, usize)> = None;

    let panel = pane(ctx, "E1.31", egui::Align2::RIGHT_BOTTOM, egui::Vec2::ZERO);
    panel.default_width(360.0).show(ctx, |ui| {
        ui.spacing_mut().item_spacing = egui::vec2(8.0, 6.0);

        ui.horizontal(|ui| {
            lamp(ui, match node.link.is_some() {
                true => GREEN,
                false => RED,
            });
            ui.label(format!("E1.31 node ({})", node.ip));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let channels: usize = node.slots.iter().map(|s| s.footprint).sum();
                ui.weak(format!("{} fixtures across {channels} channels", node.slots.len()));
            });
        });

        ui.separator();
        for i in 0..node.slots.len() {
            let kind = node.slots[i].kind;
            let count = node.slots.iter().filter(|s| s.kind == kind).count();

            ui.horizontal(|ui| {
                let slot = &node.slots[i];
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
                        pointed = pointed.or(assign(ui, &mut node.slots, i, count).map(|w| (kind, w)));
                    }
                    let slot = &mut node.slots[i];
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
            if let Some(debug) = &mut node.slots[i].debug {
                knobs(ui, debug);
            }
        }

        ui.separator();
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Save").clicked()
                    && let Err(e) = save(&node.slots)
                {
                    warn!("failed to save classic patch: {e}");
                }
            });
        });
    });

    // The ENTTEC panel owns the highlight while it is pointing at something;
    // this one only ever fills in behind it.
    if let Some((kind, world)) = pointed {
        let count = node.slots.iter().filter(|s| s.kind == kind).count();
        let name = world_name(kind, world, count);
        if highlight.0.as_deref() != Some(name.as_str()) {
            highlight.0 = Some(name);
        }
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
