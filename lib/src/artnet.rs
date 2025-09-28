use std::net::{IpAddr, SocketAddr, UdpSocket};

use anyhow::Result;
use artnet::{ArtCommand, Output, PortAddress};

use crate::prelude::*;

/// The default ArtNet port.
const DEFAULT_PORT: u16 = 6454;

/// The default DMX universe to use, 0-indexed for ArtNet.
const DEFAULT_DMX_UNIVERSE: u8 = 0;

/// ArtNet sender.
///
/// # Protocol
///
/// ArtNet is a protocol for sending DMX over IP. It's widely used in the
/// lighting industry and is an alternative to E1.31/sACN. ArtNet uses
/// broadcast UDP packets and has excellent support in professional lighting
/// equipment and software.
///
/// See <https://art-net.org.uk/>
#[derive(Resource)]
pub struct Artnet {
    socket: UdpSocket,
    dest: SocketAddr,
}

impl Artnet {
    /// Constructs a new ArtNet sender.
    pub fn new(dest_ip: &str) -> Result<Self> {
        let socket = UdpSocket::bind(("0.0.0.0", 0))?;
        let dest_ip = dest_ip.parse().with_context(|| format!("failed to parse ip: {dest_ip:?}"))?;
        let dest = SocketAddr::new(dest_ip, DEFAULT_PORT);

        Ok(Self { socket, dest })
    }

    /// Send a packet of up to 512 DMX channels to the given destination.
    pub fn send(&mut self, payload: &[u8]) {
        assert!(payload.len() <= 512);

        let command = ArtCommand::Output(Output {
            port_address: PortAddress::from(DEFAULT_DMX_UNIVERSE),
            data: payload.to_vec().into(),
            ..Output::default()
        });

        let bytes = command.write_to_buffer().unwrap();
        if let Err(e) = self.socket.send_to(&bytes, &self.dest) {
            error!("Failed to send ArtNet to {}: {e}", &self.dest);
        }
    }
}
