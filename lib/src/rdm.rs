//! E1.20 RDM controller over the ENTTEC DMX USB PRO widget.
//!
//! Requires the RDM widget firmware (2.x, see mslive's `rdm.py flash`).
//! 2.4 firmware quirks handled here (ported from mslive's `rdm.py`):
//! - the widget drops RDM responses not addressed to its own UID, so the
//!   controller source UID must be 0x454E + widget serial (byte-reversed)
//! - receive mode must be set to 'send always' (label 8) after opening
//! - broadcast label-7 sends need ~300ms before the next command
//! - the widget emits undocumented label 0x0C status frames; skip them

use std::fmt;
use std::io::{Read, Write};
use std::str::FromStr;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};

use crate::enttec::{EOM, Enttec, SOM};

const LABEL_GET_PARAMS: u8 = 3;
const LABEL_RECEIVED: u8 = 5;
const LABEL_SEND_RDM: u8 = 7;
const LABEL_RECV_MODE: u8 = 8;
const LABEL_GET_SERIAL: u8 = 10;
const LABEL_SEND_DMX: u8 = 6;
const LABEL_SEND_DUB: u8 = 11;

// RDM command classes
pub const DISCOVERY_COMMAND: u8 = 0x10;
pub const GET_COMMAND: u8 = 0x20;
pub const GET_RESPONSE: u8 = 0x21;
pub const SET_COMMAND: u8 = 0x30;

pub const RESPONSE_ACK: u8 = 0x00;
pub const RESPONSE_ACK_TIMER: u8 = 0x01;
pub const RESPONSE_NACK: u8 = 0x02;

/// How long to wait for a DISC_UNIQUE_BRANCH response before declaring a
/// branch empty. Budget: E1.20 requires responders to start answering within
/// 2.8ms, and the FTDI usb-serial chip buffers bytes for its latency timer
/// (16ms by default) before flushing over USB, so a real response lands
/// within ~20ms — 50ms is ~2.5x margin on top. (`rdm.py` waited 600ms and
/// retried every silent branch: isolating UIDs that differ only in low bits
/// visits ~48 silent branches at 1.2s each, which is why its discovery took
/// minutes.)
const DUB_TIMEOUT: Duration = Duration::from_millis(50);
/// How long to wait for a unicast GET/SET response.
const REQUEST_TIMEOUT: Duration = Duration::from_millis(250);
/// Unicast retries before giving up.
const REQUEST_RETRIES: usize = 3;
/// The widget holds the port in input for its response window after a
/// broadcast; give it time before the next command.
const BROADCAST_SETTLE: Duration = Duration::from_millis(300);

///////////////////////// UID /////////////////////////

/// A 48-bit RDM unique ID: 16-bit manufacturer + 32-bit device.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Uid(pub [u8; 6]);

pub const BROADCAST: Uid = Uid([0xFF; 6]);

impl Uid {
    fn from_u64(v: u64) -> Self {
        let b = v.to_be_bytes();
        Self([b[2], b[3], b[4], b[5], b[6], b[7]])
    }

    pub fn manufacturer(&self) -> u16 {
        u16::from_be_bytes([self.0[0], self.0[1]])
    }
}

impl fmt::Display for Uid {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let b = self.0;
        write!(f, "{:02X}{:02X}:{:02X}{:02X}{:02X}{:02X}", b[0], b[1], b[2], b[3], b[4], b[5])
    }
}

impl FromStr for Uid {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        let hex: String = s.chars().filter(|c| c.is_ascii_hexdigit()).collect();
        if hex.len() != 12 {
            return Err(anyhow!("bad UID {s:?} (want 6 hex bytes, e.g. 21A4:07312D25)"));
        }
        let mut b = [0u8; 6];
        for (i, byte) in b.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16)?;
        }
        Ok(Self(b))
    }
}

///////////////////////// PIDS /////////////////////////

/// E1.20 parameter IDs.
#[rustfmt::skip]
pub mod pid {
    pub const DISC_UNIQUE_BRANCH: u16        = 0x0001;
    pub const DISC_MUTE: u16                 = 0x0002;
    pub const DISC_UN_MUTE: u16              = 0x0003;
    pub const SUPPORTED_PARAMETERS: u16      = 0x0050;
    pub const PARAMETER_DESC: u16            = 0x0051;
    pub const DEVICE_INFO: u16               = 0x0060;
    pub const PRODUCT_DETAIL_ID_LIST: u16    = 0x0070;
    pub const DEVICE_MODEL_DESC: u16         = 0x0080;
    pub const MANUFACTURER_LABEL: u16        = 0x0081;
    pub const DEVICE_LABEL: u16              = 0x0082;
    pub const FACTORY_DEFAULTS: u16          = 0x0090;
    pub const SOFTWARE_VERSION_LABEL: u16    = 0x00C0;
    pub const BOOT_SOFTWARE_VERSION_ID: u16  = 0x00C1;
    pub const BOOT_SOFTWARE_LABEL: u16       = 0x00C2;
    pub const DMX_PERSONALITY: u16           = 0x00E0;
    pub const DMX_PERSONALITY_DESC: u16      = 0x00E1;
    pub const DMX_START_ADDRESS: u16         = 0x00F0;
    pub const SLOT_INFO: u16                 = 0x0120;
    pub const SLOT_DESCRIPTION: u16          = 0x0121;
    pub const DEFAULT_SLOT_VALUE: u16        = 0x0122;
    pub const SENSOR_DEFINITION: u16         = 0x0200;
    pub const SENSOR_VALUE: u16              = 0x0201;
    pub const DEVICE_HOURS: u16              = 0x0400;
    pub const LAMP_HOURS: u16                = 0x0401;
    pub const LAMP_STRIKES: u16              = 0x0402;
    pub const LAMP_STATE: u16                = 0x0403;
    pub const LAMP_ON_MODE: u16              = 0x0404;
    pub const DEVICE_POWER_CYCLES: u16       = 0x0405;
    pub const DISPLAY_INVERT: u16            = 0x0500;
    pub const DISPLAY_LEVEL: u16             = 0x0501;
    pub const PAN_INVERT: u16                = 0x0600;
    pub const TILT_INVERT: u16               = 0x0601;
    pub const PAN_TILT_SWAP: u16             = 0x0602;
    pub const REAL_TIME_CLOCK: u16           = 0x0603;
    pub const IDENTIFY_DEVICE: u16           = 0x1000;
    pub const RESET_DEVICE: u16              = 0x1001;
    pub const POWER_STATE: u16               = 0x1010;
    pub const PERFORM_SELFTEST: u16          = 0x1020;
}

/// Name of an E1.20 PID, if known.
pub fn pid_name(p: u16) -> Option<&'static str> {
    use pid::*;
    Some(match p {
        DISC_UNIQUE_BRANCH => "DISC_UNIQUE_BRANCH",
        DISC_MUTE => "DISC_MUTE",
        DISC_UN_MUTE => "DISC_UN_MUTE",
        SUPPORTED_PARAMETERS => "SUPPORTED_PARAMETERS",
        PARAMETER_DESC => "PARAMETER_DESCRIPTION",
        DEVICE_INFO => "DEVICE_INFO",
        PRODUCT_DETAIL_ID_LIST => "PRODUCT_DETAIL_ID_LIST",
        DEVICE_MODEL_DESC => "DEVICE_MODEL_DESCRIPTION",
        MANUFACTURER_LABEL => "MANUFACTURER_LABEL",
        DEVICE_LABEL => "DEVICE_LABEL",
        FACTORY_DEFAULTS => "FACTORY_DEFAULTS",
        SOFTWARE_VERSION_LABEL => "SOFTWARE_VERSION_LABEL",
        BOOT_SOFTWARE_VERSION_ID => "BOOT_SOFTWARE_VERSION_ID",
        BOOT_SOFTWARE_LABEL => "BOOT_SOFTWARE_LABEL",
        DMX_PERSONALITY => "DMX_PERSONALITY",
        DMX_PERSONALITY_DESC => "DMX_PERSONALITY_DESCRIPTION",
        DMX_START_ADDRESS => "DMX_START_ADDRESS",
        SLOT_INFO => "SLOT_INFO",
        SLOT_DESCRIPTION => "SLOT_DESCRIPTION",
        DEFAULT_SLOT_VALUE => "DEFAULT_SLOT_VALUE",
        SENSOR_DEFINITION => "SENSOR_DEFINITION",
        SENSOR_VALUE => "SENSOR_VALUE",
        DEVICE_HOURS => "DEVICE_HOURS",
        LAMP_HOURS => "LAMP_HOURS",
        LAMP_STRIKES => "LAMP_STRIKES",
        LAMP_STATE => "LAMP_STATE",
        LAMP_ON_MODE => "LAMP_ON_MODE",
        DEVICE_POWER_CYCLES => "DEVICE_POWER_CYCLES",
        DISPLAY_INVERT => "DISPLAY_INVERT",
        DISPLAY_LEVEL => "DISPLAY_LEVEL",
        PAN_INVERT => "PAN_INVERT",
        TILT_INVERT => "TILT_INVERT",
        PAN_TILT_SWAP => "PAN_TILT_SWAP",
        REAL_TIME_CLOCK => "REAL_TIME_CLOCK",
        IDENTIFY_DEVICE => "IDENTIFY_DEVICE",
        RESET_DEVICE => "RESET_DEVICE",
        POWER_STATE => "POWER_STATE",
        PERFORM_SELFTEST => "PERFORM_SELFTEST",
        _ => return None,
    })
}

///////////////////////// PACKETS /////////////////////////

/// A parsed RDM response frame.
pub struct Response {
    pub src: Uid,
    pub response_type: u8,
    pub cc: u8,
    pub pid: u16,
    pub pd: Vec<u8>,
}

fn build_rdm(dest: Uid, src: Uid, tn: u8, cc: u8, pid: u16, pd: &[u8]) -> Vec<u8> {
    let mut msg = Vec::with_capacity(26 + pd.len());
    msg.extend_from_slice(&[0xCC, 0x01, 24 + pd.len() as u8]);
    msg.extend_from_slice(&dest.0);
    msg.extend_from_slice(&src.0);
    msg.extend_from_slice(&[
        tn,
        0x01,
        0x00,
        0x00,
        0x00,
        cc,
        (pid >> 8) as u8,
        pid as u8,
        pd.len() as u8,
    ]);
    msg.extend_from_slice(pd);
    let cs: u16 = msg.iter().map(|&b| b as u16).fold(0u16, u16::wrapping_add);
    msg.extend_from_slice(&cs.to_be_bytes());
    msg
}

/// Parse a raw RDM frame (starting at the 0xCC start code).
fn parse_response(data: &[u8]) -> Option<Response> {
    if data.len() < 26 || data[0] != 0xCC || data[1] != 0x01 {
        return None;
    }
    let ml = data[2] as usize;
    if ml < 24 || data.len() < ml + 2 {
        return None;
    }
    let cs = ((data[ml] as u16) << 8) | data[ml + 1] as u16;
    let sum: u16 = data[..ml].iter().map(|&b| b as u16).fold(0u16, u16::wrapping_add);
    if sum != cs {
        return None;
    }
    let pdl = data[23] as usize;
    Some(Response {
        src: Uid(data[9..15].try_into().unwrap()),
        response_type: data[16],
        cc: data[20],
        pid: ((data[21] as u16) << 8) | data[22] as u16,
        pd: data[24..24 + pdl.min(data.len() - 24)].to_vec(),
    })
}

enum Dub {
    Silence,
    Collision,
    Found(Uid),
}

/// Decode a DISC_UNIQUE_BRANCH response stream: 0xFE preamble, 0xAA
/// separator, then the UID and checksum encoded as 16 or-masked bytes.
fn decode_dub(data: &[u8]) -> Dub {
    let mut i = 0;
    while i < data.len() && data[i] == 0xFE {
        i += 1;
    }
    if i >= data.len() || data[i] != 0xAA {
        return if data.is_empty() { Dub::Silence } else { Dub::Collision };
    }
    let e = &data[i + 1..];
    if e.len() < 16 {
        return Dub::Collision;
    }
    let mut uid = [0u8; 6];
    for (j, byte) in uid.iter_mut().enumerate() {
        *byte = e[2 * j] & e[2 * j + 1];
    }
    let cs = (((e[12] & e[13]) as u16) << 8) | (e[14] & e[15]) as u16;
    let sum: u16 = e[0..12].iter().map(|&b| b as u16).fold(0u16, u16::wrapping_add);
    if sum != cs {
        return Dub::Collision;
    }
    Dub::Found(Uid(uid))
}

///////////////////////// WIDGET /////////////////////////

/// Widget configuration, from label 3.
pub struct Params {
    pub firmware: String,
    pub break_time: u8,
    pub mab_time: u8,
    pub rate: u8,
}

pub struct RdmWidget {
    port: Box<dyn serialport::SerialPort>,
    src: Uid,
    tn: u8,
}

impl RdmWidget {
    /// Auto-detect the widget's port and open it.
    pub fn new() -> Result<Self> {
        Self::open(&Enttec::detect_port()?)
    }

    /// Open the widget at a specific serial port, e.g. `/dev/ttyUSB0`.
    pub fn open(path: &str) -> Result<Self> {
        let port = serialport::new(path, 57600)
            .timeout(Duration::from_millis(5))
            .open()
            .with_context(|| format!("failed to open ENTTEC at {path:?}"))?;

        let mut w = Self { port, src: BROADCAST, tn: 0 };

        // Receive mode 'send always': RDM responses arrive as label 5 frames.
        w.send(LABEL_RECV_MODE, &[0x00])?;
        std::thread::sleep(Duration::from_millis(100));
        let _ = w.port.clear(serialport::ClearBuffer::Input);

        // Source UID must be the widget's own UID (ENTTEC 0x454E + serial,
        // byte-reversed); the 2.4 firmware drops responses addressed elsewhere.
        let serial = w
            .serial_raw()
            .ok_or_else(|| anyhow!("widget did not answer serial number query (1.x DMX-only firmware?)"))?;
        w.src = Uid([0x45, 0x4E, serial[3], serial[2], serial[1], serial[0]]);

        Ok(w)
    }

    /// Send a DMX frame (label 6). The widget latches it and retransmits on
    /// its own clock, so this only needs calling when the frame changes.
    pub fn send_dmx(&mut self, payload: &[u8]) -> Result<()> {
        assert!(payload.len() <= 512);
        // Start code, then the channels, zero-padded to the label-6 minimum.
        let mut data = Vec::with_capacity(1 + payload.len().max(24));
        data.push(0x00);
        data.extend_from_slice(payload);
        data.resize(1 + payload.len().max(24), 0);
        self.send(LABEL_SEND_DMX, &data)
    }

    fn send(&mut self, label: u8, data: &[u8]) -> Result<()> {
        let mut msg = Vec::with_capacity(4 + data.len() + 1);
        msg.extend_from_slice(&[SOM, label, (data.len() & 0xFF) as u8, (data.len() >> 8) as u8]);
        msg.extend_from_slice(data);
        msg.push(EOM);
        self.port.write_all(&msg)?;
        Ok(())
    }

    /// Read one SOM/label/length/data/EOM frame, resyncing on garbage.
    fn read_frame(&mut self, timeout: Duration) -> Option<(u8, Vec<u8>)> {
        let deadline = Instant::now() + timeout;
        let mut byte = [0u8; 1];
        while Instant::now() < deadline {
            match self.port.read(&mut byte) {
                Ok(1) if byte[0] == SOM => {}
                _ => continue,
            }
            let Some(hdr) = self.read_exact(3, deadline) else {
                return None;
            };
            let length = hdr[1] as usize | ((hdr[2] as usize) << 8);
            let Some(data) = self.read_exact(length, deadline) else {
                return None;
            };
            match self.read_exact(1, deadline) {
                Some(end) if end[0] == EOM => return Some((hdr[0], data)),
                _ => continue, // resync
            }
        }
        None
    }

    fn read_exact(&mut self, n: usize, deadline: Instant) -> Option<Vec<u8>> {
        let mut out = vec![0u8; n];
        let mut have = 0;
        while have < n && Instant::now() < deadline {
            match self.port.read(&mut out[have..]) {
                Ok(got) => have += got,
                Err(_) => {} // timeout tick, keep polling until the deadline
            }
        }
        (have == n).then_some(out)
    }

    fn flush_input(&mut self) {
        let _ = self.port.clear(serialport::ClearBuffer::Input);
    }

    // ---- widget queries ----

    /// Widget firmware version and DMX timing parameters (label 3).
    pub fn params(&mut self) -> Option<Params> {
        self.flush_input();
        self.send(LABEL_GET_PARAMS, &[0x00, 0x00]).ok()?;
        let (label, p) = self.read_frame(Duration::from_secs(1))?;
        (label == LABEL_GET_PARAMS && p.len() >= 5).then(|| Params {
            firmware: format!("{}.{}", p[1], p[0]),
            break_time: p[2],
            mab_time: p[3],
            rate: p[4],
        })
    }

    fn serial_raw(&mut self) -> Option<[u8; 4]> {
        self.flush_input();
        self.send(LABEL_GET_SERIAL, &[]).ok()?;
        let (label, data) = self.read_frame(Duration::from_secs(1))?;
        (label == LABEL_GET_SERIAL && data.len() >= 4).then(|| data[..4].try_into().unwrap())
    }

    /// The widget's serial number, as printed on the sticker.
    pub fn serial(&mut self) -> Option<String> {
        let raw = self.serial_raw()?;
        Some(format!("{:02X}{:02X}{:02X}{:02X}", raw[3], raw[2], raw[1], raw[0]))
    }

    // ---- RDM transport ----

    /// One RDM request/response transaction. Broadcasts return `None` after
    /// letting the widget's response window pass.
    pub fn request(&mut self, dest: Uid, cc: u8, pid: u16, pd: &[u8]) -> Option<Response> {
        let broadcast = dest == BROADCAST;
        for _ in 0..REQUEST_RETRIES {
            self.tn = self.tn.wrapping_add(1);
            let pkt = build_rdm(dest, self.src, self.tn, cc, pid, pd);
            self.flush_input();
            self.send(LABEL_SEND_RDM, &pkt).ok()?;
            if broadcast {
                std::thread::sleep(BROADCAST_SETTLE);
                return None;
            }
            let deadline = Instant::now() + REQUEST_TIMEOUT;
            while Instant::now() < deadline {
                let Some((label, data)) = self.read_frame(deadline - Instant::now()) else {
                    break;
                };
                if label != LABEL_RECEIVED || data.len() < 2 {
                    continue; // skip status frames (e.g. label 0x0C from 2.4 firmware)
                }
                let Some(resp) = parse_response(&data[1..]) else {
                    continue;
                };
                if resp.response_type == RESPONSE_ACK_TIMER {
                    std::thread::sleep(Duration::from_millis(250));
                    break; // retry the request
                }
                return Some(resp);
            }
        }
        None
    }

    /// GET a parameter, returning its data on ACK.
    pub fn get(&mut self, dest: Uid, pid: u16, pd: &[u8]) -> Option<Vec<u8>> {
        let resp = self.request(dest, GET_COMMAND, pid, pd)?;
        (resp.cc == GET_RESPONSE && resp.response_type == RESPONSE_ACK).then_some(resp.pd)
    }

    /// SET a parameter, returning whether the device ACKed.
    pub fn set(&mut self, dest: Uid, pid: u16, pd: &[u8]) -> bool {
        match self.request(dest, SET_COMMAND, pid, pd) {
            Some(resp) => resp.response_type == RESPONSE_ACK,
            None => false,
        }
    }

    // ---- discovery ----

    fn dub(&mut self, lo: u64, hi: u64) -> Dub {
        self.tn = self.tn.wrapping_add(1);
        let mut pd = [0u8; 12];
        pd[..6].copy_from_slice(&Uid::from_u64(lo).0);
        pd[6..].copy_from_slice(&Uid::from_u64(hi).0);
        let pkt = build_rdm(BROADCAST, self.src, self.tn, DISCOVERY_COMMAND, pid::DISC_UNIQUE_BRANCH, &pd);
        self.flush_input();
        if self.send(LABEL_SEND_DUB, &pkt).is_err() {
            return Dub::Silence;
        }
        let deadline = Instant::now() + DUB_TIMEOUT;
        while Instant::now() < deadline {
            match self.read_frame(deadline - Instant::now()) {
                Some((LABEL_RECEIVED, data)) if data.len() >= 2 => return decode_dub(&data[1..]),
                Some(_) => continue, // skip status frames
                None => break,
            }
        }
        Dub::Silence
    }

    /// Mute a device so it stops answering DISC_UNIQUE_BRANCH.
    pub fn mute(&mut self, uid: Uid) -> bool {
        match self.request(uid, DISCOVERY_COMMAND, pid::DISC_MUTE, &[]) {
            Some(resp) => resp.response_type == RESPONSE_ACK,
            None => false,
        }
    }

    /// Broadcast un-mute to all devices.
    pub fn unmute_all(&mut self) {
        self.request(BROADCAST, DISCOVERY_COMMAND, pid::DISC_UN_MUTE, &[]);
    }

    /// Full binary-search discovery. Calls `progress` with each UID as it's
    /// found, and returns all of them.
    pub fn discover(&mut self, progress: &mut impl FnMut(Uid)) -> Vec<Uid> {
        self.unmute_all();
        self.unmute_all();
        let mut found = vec![];
        self.find(0, 0xFFFF_FFFF_FFFF, &mut found, progress);
        found
    }

    fn find(&mut self, lo: u64, hi: u64, found: &mut Vec<Uid>, progress: &mut impl FnMut(Uid)) {
        loop {
            let resp = match self.dub(lo, hi) {
                Dub::Silence => self.dub(lo, hi), // one retry on silence
                resp => resp,
            };
            match resp {
                Dub::Silence => return,
                Dub::Collision => {
                    if lo < hi {
                        let mid = lo + (hi - lo) / 2;
                        self.find(lo, mid, found, progress);
                        self.find(mid + 1, hi, found, progress);
                    }
                    return;
                }
                Dub::Found(uid) => {
                    if self.mute(uid) {
                        if !found.contains(&uid) {
                            found.push(uid);
                            progress(uid);
                        }
                        // loop: more devices may hide in this branch
                    } else if lo < hi {
                        let mid = lo + (hi - lo) / 2;
                        self.find(lo, mid, found, progress);
                        self.find(mid + 1, hi, found, progress);
                        return;
                    } else {
                        return;
                    }
                }
            }
        }
    }
}
