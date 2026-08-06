//! DMX output to a real fixture, addressed by RDM discovery.

use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use lib::prelude::*;
use lib::rdm::{RdmWidget, Uid, pid};

use lib::gdtf::{GdtfFixture, GdtfLibrary};

use crate::{Fixture, KIND};

/// A responder found on the bus.
#[derive(Clone)]
pub struct Responder {
    pub uid: Uid,
    pub model: String,
    pub model_id: u16,
    pub address: u16,
    pub footprint: u16,
    pub personality: u8,
    /// (number, name, footprint), as the fixture reports them.
    pub personalities: Vec<(u8, String, u16)>,
    /// Pan/tilt reversal, if the fixture exposes those settings.
    pub invert: [Option<bool>; 2],
}

impl Responder {
    /// The personality the fixture is currently in.
    pub fn active(&self) -> Option<&(u8, String, u16)> {
        self.personalities.iter().find(|(n, _, _)| *n == self.personality)
    }

    /// Footprint of the personality the fixture is currently in.
    pub fn active_footprint(&self) -> u16 {
        self.active().map_or(self.footprint, |(_, _, f)| *f)
    }
}

/// Fold a mode or personality name for comparison: fixtures and GDTFs disagree
/// on spacing and case ("STDY" vs "STD Y").
fn fold(s: &str) -> String {
    s.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

enum Cmd {
    Discover,
}

enum Ev {
    Log(String),
    Discovering(bool),
    Responders(Vec<Responder>),
}

#[derive(Resource)]
pub struct Dmx {
    tx: Option<Sender<Cmd>>,
    rx: Option<Mutex<Receiver<Ev>>>,
    /// Frame handed to the worker; taken and sent on the wire.
    pending: Arc<Mutex<Option<Vec<u8>>>>,
    pub status: String,
    pub enabled: bool,
    pub discovering: bool,
    pub responders: Vec<Responder>,
    pub selected: Option<usize>,
    pub log: Vec<String>,
    sent: Vec<u8>,
}

impl Dmx {
    /// Open the widget and start its worker. A missing widget is not fatal:
    /// the app is still useful as a viewer.
    pub fn spawn(port: Option<&str>) -> Self {
        let mut dmx = Self {
            tx: None,
            rx: None,
            pending: Arc::new(Mutex::new(None)),
            status: String::new(),
            // Output still waits on a discovered target, so this sends nothing
            // until there is a fixture to send to.
            enabled: true,
            discovering: false,
            responders: vec![],
            selected: None,
            log: vec![],
            sent: vec![],
        };

        let widget = match port {
            Some(path) => RdmWidget::open(path),
            None => RdmWidget::new(),
        };
        match widget {
            Ok(widget) => {
                let (cmd_tx, cmd_rx) = channel();
                let (ev_tx, ev_rx) = channel();
                let pending = dmx.pending.clone();
                std::thread::spawn(move || worker(widget, cmd_rx, ev_tx, pending));
                _ = cmd_tx.send(Cmd::Discover);
                dmx.tx = Some(cmd_tx);
                dmx.rx = Some(Mutex::new(ev_rx));
                dmx.status = "ENTTEC DMX USB PRO".into();
            }
            Err(e) => dmx.status = format!("no widget: {e}"),
        }
        dmx
    }

    pub fn present(&self) -> bool {
        self.tx.is_some()
    }

    pub fn rescan(&mut self) {
        if let Some(tx) = &self.tx {
            self.responders.clear();
            self.selected = None;
            _ = tx.send(Cmd::Discover);
        }
    }

    pub fn target(&self) -> Option<&Responder> {
        self.selected.and_then(|i| self.responders.get(i))
    }
}

/// The worker owns the port: RDM transactions block for hundreds of ms.
fn worker(mut widget: RdmWidget, rx: Receiver<Cmd>, tx: Sender<Ev>, pending: Arc<Mutex<Option<Vec<u8>>>>) {
    let log = |s: String| _ = tx.send(Ev::Log(s));

    match widget.params() {
        Some(p) => {
            let serial = widget.serial().unwrap_or_default();
            log(format!("widget fw {} serial {serial}", p.firmware));
            if p.firmware.starts_with("1.") {
                log("DMX-only 1.x firmware: RDM needs 2.x (see rdm.py flash)".into());
            }
        }
        None => log("widget did not answer its parameter query".into()),
    }

    loop {
        match rx.try_recv() {
            Ok(Cmd::Discover) => {
                _ = tx.send(Ev::Discovering(true));
                // A silent bus is usually responders left muted by another
                // controller, so a second sweep after un-muting often finds them.
                let mut uids = widget.discover(&mut |uid| log(format!("found {uid}")));
                if uids.is_empty() {
                    log("nothing answered, un-muting and retrying".into());
                    widget.unmute_all();
                    uids = widget.discover(&mut |uid| log(format!("found {uid}")));
                }
                if uids.is_empty() {
                    log("no responders on the bus".into());
                }
                let responders: Vec<Responder> =
                    uids.into_iter().map(|uid| enumerate(&mut widget, uid)).collect();
                for r in &responders {
                    log(format!(
                        "{} {:?} mfr {:04X} model {:04X} @{} {}ch pers {}/{} invert {:?}",
                        r.uid,
                        r.model,
                        r.uid.manufacturer(),
                        r.model_id,
                        r.address,
                        r.active_footprint(),
                        r.personality,
                        r.personalities.len(),
                        r.invert,
                    ));
                    for (n, name, footprint) in &r.personalities {
                        log(format!("  personality {n}: {name:?} {footprint}ch"));
                    }
                }
                _ = tx.send(Ev::Responders(responders));
                _ = tx.send(Ev::Discovering(false));
            }
            Err(TryRecvError::Disconnected) => return,
            Err(TryRecvError::Empty) => {}
        }

        let frame = pending.lock().unwrap().take();
        if let Some(frame) = frame
            && let Err(e) = widget.send_dmx(&frame)
        {
            log(format!("send failed: {e}"));
        }

        std::thread::sleep(Duration::from_millis(20));
    }
}

fn enumerate(widget: &mut RdmWidget, uid: Uid) -> Responder {
    let ascii = |pd: Vec<u8>| {
        pd.iter()
            .map(|&b| if b == 0 { ' ' } else { b as char })
            .collect::<String>()
            .trim()
            .to_string()
    };

    let mut r = Responder {
        uid,
        model: String::new(),
        model_id: 0,
        address: 1,
        footprint: 0,
        personality: 0,
        personalities: vec![],
        invert: [None, None],
    };
    r.model = widget.get(uid, pid::DEVICE_MODEL_DESC, &[]).map(ascii).unwrap_or_default();

    let mut count = 0;
    if let Some(info) = widget.get(uid, pid::DEVICE_INFO, &[])
        && info.len() >= 19
    {
        r.model_id = u16::from_be_bytes([info[2], info[3]]);
        r.footprint = u16::from_be_bytes([info[10], info[11]]);
        r.personality = info[12];
        count = info[13];
        r.address = u16::from_be_bytes([info[14], info[15]]);
    }

    // The fixture's own personality list is authoritative: a GDTF's <FTRDM>
    // map is often stale and misses modes added in later firmware.
    for n in 1..=count {
        if let Some(pd) = widget.get(uid, pid::DMX_PERSONALITY_DESC, &[n])
            && pd.len() >= 3
        {
            r.personalities
                .push((n, ascii(pd[3..].to_vec()), u16::from_be_bytes([pd[1], pd[2]])));
        }
    }

    // Pan/tilt reversal is a menu setting on the fixture, so the model can
    // only match the real thing by asking.
    let flag = |w: &mut RdmWidget, p| w.get(uid, p, &[]).and_then(|pd| Some(pd.first()? == &1));
    r.invert = [flag(widget, pid::PAN_INVERT), flag(widget, pid::TILT_INVERT)];
    r
}

pub fn poll(
    mut dmx: ResMut<Dmx>,
    mut library: ResMut<GdtfLibrary>,
    fixture: Option<Single<&mut GdtfFixture, With<Fixture>>>,
) {
    let Some(rx) = &dmx.rx else {
        return;
    };
    let events: Vec<Ev> = rx.lock().unwrap().try_iter().collect();
    let Some(mut fixture) = fixture else { return };

    for ev in events {
        match ev {
            Ev::Log(s) => {
                info!("{s}");
                dmx.log.push(s);
            }
            Ev::Discovering(b) => dmx.discovering = b,
            Ev::Responders(responders) => {
                let mut switch_to = None;
                let Some(gdtf) = library.get(KIND).map(|ty| &ty.gdtf) else { continue };
                // Prefer the responder this GDTF describes.
                let matched = gdtf.rdm.as_ref().and_then(|rdm| {
                    responders
                        .iter()
                        .position(|r| r.uid.manufacturer() == rdm.manufacturer && r.model_id == rdm.model)
                });
                dmx.selected = matched.or_else(|| (!responders.is_empty()).then_some(0));
                dmx.responders = responders;

                // Follow the fixture: match the mode it's patched as and the
                // pan/tilt reversal set in its menu.
                if let Some(target) = dmx.target() {
                    // Footprint alone is ambiguous when two personalities share
                    // a channel count, so prefer a name match among those.
                    let footprint = target.active_footprint() as usize;
                    let name = target.active().map(|(_, name, _)| fold(name)).unwrap_or_default();
                    let fits: Vec<usize> = (0..gdtf.modes.len())
                        .filter(|&i| gdtf.modes[i].footprint() == footprint)
                        .collect();
                    let mode = fits
                        .iter()
                        .find(|&&i| fold(&gdtf.modes[i].name) == name)
                        .or(fits.first())
                        .copied();
                    for axle in 0..2 {
                        if let Some(invert) = target.invert[axle] {
                            fixture.invert[axle] = invert;
                        }
                    }
                    switch_to = mode;
                }

                // Taking the library mutably has to wait for the read above.
                if let Some(mode) = switch_to
                    && let Some(ty) = library.get_mut(KIND)
                    && ty.mode != mode
                {
                    ty.mode = mode;
                    fixture.rebuild();
                }
            }
        }
    }
}

pub fn output(
    mut dmx: ResMut<Dmx>,
    library: Res<GdtfLibrary>,
    fixture: Option<Single<&GdtfFixture, With<Fixture>>>,
) {
    if !dmx.enabled {
        return;
    }
    let Some(target) = dmx.target().cloned() else {
        return;
    };
    let (Some(fixture), Some(ty)) = (fixture, library.get(KIND)) else {
        return;
    };

    let mut universe = vec![0u8; 512];
    let start = target.address.max(1) as usize - 1;
    for (i, byte) in ty.mode().encode(&fixture.values).into_iter().enumerate() {
        if start + i < universe.len() {
            universe[start + i] = byte;
        }
    }

    if universe != dmx.sent {
        dmx.sent = universe.clone();
        *dmx.pending.lock().unwrap() = Some(universe);
    }
}
