use std::net::{IpAddr, SocketAddr};

use anyhow::{Result, anyhow};
use sacn::packet::ACN_SDT_MULTICAST_PORT;
use sacn::source::SacnSource;

use crate::prelude::*;

/// The default E1.31 port.
const DEFAULT_PORT: u16 = ACN_SDT_MULTICAST_PORT;

/// The default DMX universe to use, 1-indexed.
const DEFAULT_DMX_UNIVERSE: u16 = 1;

/// E1.31 (aka Streaming ACN) sender.
///
/// # Protocol
///
/// E1.31 (aka Streaming ACN) is a protocol for sending DMX over IP. It's widely
/// used in the lighting industry, and has excellent library support on various
/// platforms including microcontrollers.
///
/// See <https://wiki.openlighting.org/index.php/E1.31>
#[derive(Resource)]
pub struct E131 {
    src: SacnSource,
    dest: IpAddr,
}

impl E131 {
    /// Constructs a new E1.31 sender.
    pub fn new(dest_ip: &str) -> Result<Self> {
        let src_addr = SocketAddr::new("0.0.0.0".parse()?, 0);
        let dest = dest_ip.parse().with_context(|| format!("failed to parse ip: {dest_ip:?}"))?;

        let mut src = SacnSource::with_ip("stagebridge", src_addr).map_err(|e| anyhow!("{e}"))?;
        src.register_universe(DEFAULT_DMX_UNIVERSE).unwrap();

        Ok(Self { src, dest })
    }

    /// Send a packet of up to 512 DMX channels to the given destination.
    pub fn send(&mut self, payload: &[u8]) {
        assert!(payload.len() <= 512);

        let dest = SocketAddr::new(self.dest.clone(), DEFAULT_PORT);
        if let Err(e) = self.src.send(&[DEFAULT_DMX_UNIVERSE], payload, None, Some(dest), None) {
            error!("Failed to send E1.31 to {dest}: {e}");
        }
    }
}
