use std::net::{SocketAddr, UdpSocket};
use std::sync::{Mutex, mpsc};

use anyhow::{Context, Result};
use rosc::{OscArray as RoscArray, OscMessage as RoscMessage, OscPacket as RoscPacket, OscType as RoscType};

use crate::prelude::*;

/// OSC (Open Sound Control) sender and receiver.
///
/// # Protocol
///
/// OSC is a protocol for networking sound synthesizers, like MIDI for the
/// modern age. It's supported by a wide range of software and open source
/// tools.
///
/// See <https://en.wikipedia.org/wiki/Open_Sound_Control>
pub struct Osc {
    send_sock: UdpSocket,
    rx: Mutex<mpsc::Receiver<OscMessage>>,
}

/// An OSC message consisting of an address and list of args, e.g. `("/effects/slider1", [OscType::Float(0.42)])`.
pub type OscMessage = (String, Vec<OscType>);

/// An OSC type, e.g. `Float(1.337)` or `Int(42)`.
//
// We define our own simpler enum instead of reusing the underlying `RoscType` to make the API better.
// There's no need for separate types for i32/i64, types for nil and infinity, and other such siliness.
#[derive(Debug)]
pub enum OscType {
    Bool(bool),
    Int(i64),
    Float(f32),
    String(String),
    Blob(Vec<u8>),
    Array(Vec<OscType>),
    Other(RoscType),
}

impl Osc {
    /// Constructs a new OSC sender/receiver listening at the given address.
    pub fn new(listen_addr: SocketAddr) -> Result<Self> {
        Self::new_inner(listen_addr).with_context(|| format!("Failed to initialize OSC at {listen_addr}"))
    }

    fn new_inner(listen_addr: SocketAddr) -> Result<Self> {
        let send_sock = UdpSocket::bind("0.0.0.0:0").context("Failed to bind sending socket")?;
        let recv_sock = UdpSocket::bind(listen_addr).context("Failed to bind receiving socket")?;

        // Spawn a worker thread which parses incoming packets and pushes them to the back of the queue.
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut buf = [0u8; rosc::decoder::MTU];
            loop {
                let size = match recv_sock.recv_from(&mut buf) {
                    Ok((size, _addr)) => size,
                    Err(e) => {
                        error!("Failed to receive on OSC socket: {e}");
                        continue;
                    }
                };

                let packet = match rosc::decoder::decode(&buf[..size]) {
                    Ok(packet) => flatten_packet(packet),
                    Err(e) => {
                        error!("Failed to parse OSC packet: {e}");
                        continue;
                    }
                };

                for RoscMessage { addr, args } in packet {
                    // Convert the args from `RoscType` to `OscType`.
                    let args = args.into_iter().map(OscType::from).collect();
                    // Drop any errors, that means the main thread exited and we're shutting down anyways.
                    let _ = tx.send((addr, args));
                }
            }
        });

        Ok(Self { send_sock, rx: Mutex::new(rx) })
    }

    /// Receive any pending OSC messages.
    pub fn recv(&mut self) -> Vec<(String, Vec<OscType>)> {
        let mut msgs = vec![];
        while let Ok(msg) = self.rx.lock().unwrap().try_recv() {
            msgs.push(msg);
        }
        msgs
    }

    /// Send an OSC message to the given destination.
    pub fn send(&mut self, dest: &SocketAddr, addr: String, args: Vec<OscType>) {
        // Convert the args from `OscType` to `RoscType`.
        let args = args.into_iter().map(RoscType::from).collect();
        let packet = RoscPacket::Message(RoscMessage { addr, args });
        // unwrap(): It literally can't return an error, I checked the source...
        let buf = rosc::encoder::encode(&packet).unwrap();

        if let Err(e) = self.send_sock.send_to(&buf, dest) {
            error!("Failed to send OSC to {dest}: {e}");
        }
    }
}

/// OSC packets can recursively contain "bundles" of more packets. Flatten them out for easier processing.
fn flatten_packet(packet: RoscPacket) -> Vec<RoscMessage> {
    match packet {
        RoscPacket::Message(msg) => vec![msg],
        RoscPacket::Bundle(bundle) => {
            let mut packets = vec![];
            for packet in bundle.content {
                packets.extend(flatten_packet(packet));
            }
            packets
        }
    }
}

/// Convert a `RoscType` to our own internal `OscType`.
impl From<RoscType> for OscType {
    fn from(ty: RoscType) -> Self {
        match ty {
            RoscType::Bool(b) => OscType::Bool(b),
            RoscType::Int(i) => OscType::Int(i as i64),
            RoscType::Long(i) => OscType::Int(i),
            RoscType::Float(f) => OscType::Float(f),
            RoscType::Double(f) => OscType::Float(f as f32),
            RoscType::String(str) => OscType::String(str),
            RoscType::Blob(b) => OscType::Blob(b),
            RoscType::Array(arr) => OscType::Array(arr.content.into_iter().map(OscType::from).collect()),
            ty => OscType::Other(ty),
        }
    }
}

/// Convert an `OscType` back into a `RoscType`.
impl From<OscType> for RoscType {
    fn from(ty: OscType) -> Self {
        match ty {
            OscType::Bool(b) => RoscType::Bool(b),
            OscType::Int(i) => RoscType::Long(i),
            OscType::Float(f) => RoscType::Float(f),
            OscType::String(str) => RoscType::String(str),
            OscType::Blob(b) => RoscType::Blob(b),
            OscType::Array(arr) => {
                RoscType::Array(RoscArray { content: arr.into_iter().map(RoscType::from).collect() })
            }
            OscType::Other(ty) => ty,
        }
    }
}
