use std::net::{IpAddr, SocketAddr};

use anyhow::{Result, anyhow};
use sacn::packet::ACN_SDT_MULTICAST_PORT;
use sacn::source::SacnSource;

use crate::prelude::*;

/// The default E1.31 port.
const DEFAULT_PORT: u16 = ACN_SDT_MULTICAST_PORT;

/// The default DMX universe to use, 1-indexed.
const DEFAULT_DMX_UNIVERSE: u16 = 1;

/// Null start code: the packet carries dimmer data.
const DMX_START_CODE: u8 = 0;

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
    dest: SocketAddr,
}

impl E131 {
    /// Constructs a new E1.31 sender.
    pub fn new(dest_ip: &str) -> Result<Self> {
        let src_addr = SocketAddr::new("0.0.0.0".parse()?, 0);
        let dest_ip: IpAddr = dest_ip.parse().with_context(|| format!("failed to parse ip: {dest_ip:?}"))?;
        let dest = SocketAddr::new(dest_ip, DEFAULT_PORT);

        let mut src = SacnSource::with_ip("stagebridge", src_addr).map_err(|e| anyhow!("{e}"))?;
        src.register_universe(DEFAULT_DMX_UNIVERSE).unwrap();

        Ok(Self { src, dest })
    }

    /// Send up to 512 DMX channels, `channels[0]` being channel 1, to the
    /// given destination.
    pub fn send(&mut self, channels: &[u8]) {
        assert!(channels.len() <= 512);

        // The wire carries the DMX start code ahead of the channels, and the
        // library passes the payload through as it is given.
        let mut payload = Vec::with_capacity(channels.len() + 1);
        payload.push(DMX_START_CODE);
        payload.extend_from_slice(channels);

        if let Err(e) = self.src.send(&[DEFAULT_DMX_UNIVERSE], &payload, None, Some(self.dest), None) {
            error!("Failed to send E1.31 to {}: {e}", &self.dest);
        }
    }
}
