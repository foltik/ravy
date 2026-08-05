use std::io::Write;
use std::sync::Mutex;
use std::time::Duration;

use anyhow::{Result, anyhow};

use crate::prelude::*;

/// ENTTEC DMX USB PRO sender.
///
/// # Protocol
///
/// The DMX USB PRO is an FTDI usb-serial widget. DMX frames are framed with
/// the label-6 (Output Only Send DMX) message, and the widget retransmits the
/// last frame on the wire on its own clock. On open, the widget is configured
/// (label-4, refresh rate 0) to output as fast as possible instead of its
/// stock 40 packet/s cap, so the wire rate is bounded only by frame size:
/// ~44 Hz for a full 512-channel frame, faster for shorter frames.
/// The serial baud rate setting is ignored by the widget.
///
/// See <https://cdn.enttec.com/pdf/assets/70304/70304_DMX_USB_PRO_API.pdf>
#[derive(Resource)]
pub struct Enttec {
    // Mutex only to make the resource Sync: send() already takes &mut self.
    port: Mutex<Box<dyn serialport::SerialPort>>,
}

pub(crate) const SOM: u8 = 0x7E;
pub(crate) const EOM: u8 = 0xE7;
const LABEL_SET_PARAMS: u8 = 4;
const LABEL_SEND_DMX: u8 = 6;

/// Label-6 requires at least 24 channels of payload.
const MIN_CHANNELS: usize = 24;

impl Enttec {
    /// Auto-detect and open the widget: the first FTDI (vid 0403) usb-serial port.
    pub fn new() -> Result<Self> {
        Self::open(&Self::detect_port()?)
    }

    /// Find the widget's serial port: the first FTDI (vid 0403) usb-serial port.
    pub fn detect_port() -> Result<String> {
        serialport::available_ports()
            .unwrap_or_default()
            .into_iter()
            .find_map(|p| match &p.port_type {
                serialport::SerialPortType::UsbPort(usb) if usb.vid == 0x0403 => Some(p.port_name),
                _ => None,
            })
            .ok_or_else(|| anyhow!("no FTDI usb-serial port found"))
    }

    /// Open the widget at a specific serial port, e.g. `/dev/ttyUSB0`.
    pub fn open(path: &str) -> Result<Self> {
        let port = serialport::new(path, 57600)
            .timeout(Duration::from_millis(50))
            .open()
            .with_context(|| format!("failed to open ENTTEC at {path:?}"))?;
        let mut enttec = Self { port: Mutex::new(port) };

        // Set widget parameters: no user config, default break (9 x 10.67us)
        // and mark-after-break (1 x 10.67us), refresh rate 0 = as fast as possible.
        enttec
            .write(LABEL_SET_PARAMS, &[0, 0, 9, 1, 0])
            .with_context(|| format!("failed to configure ENTTEC at {path:?}"))?;

        info!("DMX out: ENTTEC DMX USB PRO on {path}");
        Ok(enttec)
    }

    /// Send a packet of up to 512 DMX channels. The widget latches the frame
    /// and retransmits it on the wire as fast as possible.
    pub fn send(&mut self, payload: &[u8]) {
        assert!(payload.len() <= 512);

        // Message data is a 0x00 DMX start code followed by the channels,
        // zero-padded up to the label-6 minimum.
        let len = 1 + payload.len().max(MIN_CHANNELS);
        let mut data = Vec::with_capacity(len);
        data.push(0x00);
        data.extend_from_slice(payload);
        data.resize(len, 0);

        if let Err(e) = self.write(LABEL_SEND_DMX, &data) {
            error!("Failed to send DMX to ENTTEC: {e}");
        }
    }

    /// Write one SOM/label/length/data/EOM message to the widget.
    fn write(&mut self, label: u8, data: &[u8]) -> std::io::Result<()> {
        let mut msg = Vec::with_capacity(4 + data.len() + 1);
        msg.extend_from_slice(&[SOM, label, (data.len() & 0xFF) as u8, (data.len() >> 8) as u8]);
        msg.extend_from_slice(data);
        msg.push(EOM);
        self.port.get_mut().unwrap().write_all(&msg)
    }
}
