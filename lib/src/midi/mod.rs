use std::sync::mpsc;
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::{Context, Result};
use midir::{MidiInput, MidiInputConnection, MidiOutput};

use crate::prelude::*;

pub mod device;
pub use device::MidiDevice;

/// A MIDI device.
pub enum Midi<D: MidiDevice> {
    Connected {
        in_rx: mpsc::Receiver<D::Input>,
        out_tx: mpsc::Sender<D::Output>,

        _raw: MidiRaw,
        _thread: JoinHandle<()>,
    },
    Disconnected,
}

impl<D: MidiDevice> Midi<D> {
    /// Try to open a MIDI device.
    pub fn new(name: &str, mut device: D) -> Self {
        match MidiRaw::connect(name) {
            Ok((_raw, raw_rx, raw_tx)) => {
                let (in_tx, in_rx) = mpsc::channel::<D::Input>();
                let (out_tx, out_rx) = mpsc::channel::<D::Output>();

                let _name = name.to_string();
                let _thread = thread::spawn(move || {
                    loop {
                        if let Ok(data) = raw_rx.try_recv() {
                            if let Some(input) = device.process_input(&data) {
                                trace!("{_name} <- {input:?}");
                                in_tx.send(input).unwrap();
                            }
                        }

                        if let Ok(output) = out_rx.try_recv() {
                            trace!("{_name} -> {output:?}");
                            let data = device.process_output(output);
                            if !data.is_empty() {
                                raw_tx.send(data).unwrap();
                            }
                        }

                        thread::sleep(Duration::from_millis(1));
                    }
                });

                let mut this = Self::Connected { in_rx, out_tx, _raw, _thread };
                D::init(&mut this);
                this
            }
            Err(e) => {
                warn!("Failed to open MIDI {name:?}: {e}");
                Self::Disconnected
            }
        }
    }

    /// Call the given callback with any pending MIDI events.
    pub fn recv(&mut self) -> Vec<D::Input> {
        match self {
            Midi::Connected { in_rx, .. } => {
                let mut msgs = vec![];
                while let Ok(event) = in_rx.try_recv() {
                    msgs.push(event);
                }
                msgs
            }
            Midi::Disconnected => vec![],
        }
    }

    /// Send a MIDI event.
    pub fn send(&mut self, output: D::Output) {
        match self {
            Midi::Connected { out_tx, .. } => out_tx.send(output).unwrap(),
            Midi::Disconnected => {}
        }
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

pub struct MidiRaw {
    _in_conn: MidiInputConnection<()>,
    _out_thread: JoinHandle<()>,
}

#[allow(clippy::type_complexity)]
impl MidiRaw {
    pub fn connect(name: &str) -> Result<(Self, mpsc::Receiver<Vec<u8>>, mpsc::Sender<Vec<u8>>)> {
        let (in_tx, in_rx) = mpsc::channel::<Vec<u8>>();
        let (out_tx, out_rx) = mpsc::channel::<Vec<u8>>();

        let midi_in = MidiInput::new(&format!("{}_in", name))?;
        let midi_out = MidiOutput::new(&format!("{}_out", name))?;

        let in_port = midi_in
            .ports()
            .into_iter()
            .find(|p| midi_in.port_name(p).unwrap().contains(name))
            .with_context(|| format!("no midi input '{name}'"))?;
        let out_port = midi_out
            .ports()
            .into_iter()
            .find(|p| midi_out.port_name(p).unwrap().contains(name))
            .with_context(|| format!("no midi output '{name}'"))?;

        let _name = name.to_string();
        let _in_conn = midi_in
            .connect(
                &in_port,
                "in",
                move |_, data, _| {
                    // trace!("{_name} <- [{data:X?}]");
                    in_tx.send(data.to_vec()).unwrap();
                },
                (),
            )
            .unwrap();

        let mut out_conn = midi_out.connect(&out_port, "out").unwrap();

        let _name = name.to_string();
        let _out_thread = thread::spawn(move || {
            loop {
                let data = out_rx.recv().unwrap();
                // trace!("{_name} -> {data:X?}");
                out_conn.send(&data).unwrap();
            }
        });

        Ok((Self { _in_conn, _out_thread }, in_rx, out_tx))
    }
}
