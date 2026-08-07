use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use midir::{MidiInput, MidiInputConnection, MidiOutput};

use crate::prelude::*;

pub mod device;
pub use device::MidiDevice;

/// How often to scan ports for hotplug (dis)connects.
const RESCAN: Duration = Duration::from_millis(100);

/// Prefix on every port we open. Our own ports appear in the same scans as the
/// hardware, so they must be identifiable to be skipped.
const OURS: &str = "ravy ";

/// Whether a scanned port is the hardware we want, and not one of ours.
fn is_device(port: &str, name: &str) -> bool {
    !port.starts_with(OURS) && port.contains(name)
}

/// A MIDI device, connected and reconnected in the background as it hotplugs.
#[derive(Resource)]
pub struct Midi<D: MidiDevice> {
    name: String,
    in_rx: Mutex<mpsc::Receiver<D::Input>>,
    out_tx: Mutex<mpsc::Sender<D::Output>>,
    connected: Arc<AtomicBool>,
}

impl<D: MidiDevice> Midi<D> {
    /// Open a MIDI device by port name. Always succeeds: a supervisor thread
    /// connects whenever a matching port is present and reconnects on unplug,
    /// resending `D::init` each time.
    pub fn new(name: &str, device: D) -> Self {
        let (in_tx, in_rx) = mpsc::channel::<D::Input>();
        let (out_tx, out_rx) = mpsc::channel::<D::Output>();
        let connected = Arc::new(AtomicBool::new(false));

        let name = name.to_string();
        let conn = connected.clone();
        thread::spawn({
            let name = name.clone();
            move || supervise(name, device, in_tx, out_rx, conn)
        });

        Self { name, in_rx: Mutex::new(in_rx), out_tx: Mutex::new(out_tx), connected }
    }

    /// The port name the supervisor is looking for.
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }

    /// All pending MIDI events.
    pub fn recv(&mut self) -> Vec<D::Input> {
        self.in_rx.lock().unwrap().try_iter().collect()
    }

    /// Send a MIDI event. Dropped if disconnected.
    pub fn send(&mut self, output: D::Output) {
        let _ = self.out_tx.lock().unwrap().send(output);
    }

    /// Log all available midi devices.
    pub fn list() -> Result<()> {
        let midi_in = MidiInput::new(&format!("_list_inputs"))?;
        let midi_out = MidiOutput::new(&format!("_list_outputs"))?;

        for port in midi_in.ports() {
            info!("IN: '{:?}'", midi_in.port_name(&port).unwrap());
        }

        for port in midi_out.ports() {
            info!("OUT: '{:?}'", midi_out.port_name(&port).unwrap());
        }

        Ok(())
    }
}

/// Own the device: connect when the port appears, pump i/o until it vanishes.
fn supervise<D: MidiDevice>(
    name: String,
    mut device: D,
    in_tx: mpsc::Sender<D::Input>,
    out_rx: mpsc::Receiver<D::Output>,
    connected: Arc<AtomicBool>,
) {
    let mut reconnect = false;
    loop {
        let Ok((raw, raw_rx, raw_tx)) = MidiRaw::connect(&name) else {
            // Drop stale outputs while unplugged
            while out_rx.try_recv().is_ok() {}
            thread::sleep(RESCAN);
            continue;
        };

        match reconnect {
            true => info!("MIDI reconnected: {name}"),
            false => info!("MIDI connected: {name}"),
        }
        reconnect = true;
        connected.store(true, Ordering::Relaxed);
        for output in device.init() {
            let data = device.process_output(output);
            if !data.is_empty() {
                let _ = raw_tx.send(data);
            }
        }

        let mut scanned = Instant::now();
        loop {
            if let Ok(data) = raw_rx.try_recv() {
                if let Some(input) = device.process_input(&data) {
                    trace!("{name} <- {input:?}");
                    let _ = in_tx.send(input);
                }
            }

            if let Ok(output) = out_rx.try_recv() {
                trace!("{name} -> {output:?}");
                let data = device.process_output(output);
                if !data.is_empty() && raw_tx.send(data).is_err() {
                    break;
                }
            }

            if scanned.elapsed() >= RESCAN {
                scanned = Instant::now();
                if !MidiRaw::present(&name) {
                    break;
                }
            }

            thread::sleep(Duration::from_millis(1));
        }

        error!("MIDI disconnected: {name}");
        connected.store(false, Ordering::Relaxed);
        // Fully tear down before rescanning, so the next connect can't bind to
        // a port of ours that is still closing.
        drop(raw_tx);
        raw.close();
    }
}

pub struct MidiRaw {
    _in_conn: MidiInputConnection<()>,
    out_thread: JoinHandle<()>,
}

#[allow(clippy::type_complexity)]
impl MidiRaw {
    pub fn connect(name: &str) -> Result<(Self, mpsc::Receiver<Vec<u8>>, mpsc::Sender<Vec<u8>>)> {
        let (in_tx, in_rx) = mpsc::channel::<Vec<u8>>();
        let (out_tx, out_rx) = mpsc::channel::<Vec<u8>>();

        let midi_in = MidiInput::new(&format!("{OURS}{name} in"))?;
        let midi_out = MidiOutput::new(&format!("{OURS}{name} out"))?;

        let in_port = midi_in
            .ports()
            .into_iter()
            .find(|p| midi_in.port_name(p).is_ok_and(|n| is_device(&n, name)))
            .with_context(|| format!("no midi input '{name}'"))?;
        let out_port = midi_out
            .ports()
            .into_iter()
            .find(|p| midi_out.port_name(p).is_ok_and(|n| is_device(&n, name)))
            .with_context(|| format!("no midi output '{name}'"))?;

        let _name = name.to_string();
        let _in_conn = midi_in
            .connect(
                &in_port,
                "in",
                move |_, data, _| {
                    // trace!("{_name} <- [{data:X?}]");
                    let _ = in_tx.send(data.to_vec());
                },
                (),
            )
            .map_err(|e| anyhow!("failed to connect midi input '{name}': {e}"))?;

        let mut out_conn = midi_out
            .connect(&out_port, "out")
            .map_err(|e| anyhow!("failed to connect midi output '{name}': {e}"))?;

        let _name = name.to_string();
        let out_thread = thread::spawn(move || {
            loop {
                let Ok(data) = out_rx.recv() else { break };
                // trace!("{_name} -> {data:X?}");
                if out_conn.send(&data).is_err() {
                    break;
                }
            }
        });

        Ok((Self { _in_conn, out_thread }, in_rx, out_tx))
    }

    /// Whether a port matching `name` is currently present.
    pub fn present(name: &str) -> bool {
        MidiInput::new(&format!("{OURS}scan")).is_ok_and(|midi_in| {
            midi_in.ports().iter().any(|p| midi_in.port_name(p).is_ok_and(|n| is_device(&n, name)))
        })
    }

    /// Close, waiting for the output port to be released. The output sender
    /// must already be dropped, or this blocks.
    fn close(self) {
        let Self { _in_conn, out_thread } = self;
        drop(_in_conn);
        let _ = out_thread.join();
    }
}
