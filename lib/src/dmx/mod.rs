pub mod device;
pub use device::Device;

// pub struct DMX {
//     buffer: Vec<u8>,
// }

// impl DMX {
//     pub fn new(channels: usize) -> Self {
//         Self {
//             buffer: vec![0; channels + 1]
//         }
//     }

//     pub fn buffer(&self) -> &[u8] {
//         &self.buffer
//     }

//     fn slice(&mut self, i: usize, j: usize) -> &mut [u8] {
//         debug_assert!(i < self.buffer.len() && j < self.buffer.len());
//         &mut self.buffer[i..j]
//     }
// }

// pub trait DMXDevice {
//     // Size in channels
//     fn size(&self) -> usize;

//     // Encode to a byte buffer of `self.size()` bytes
//     fn encode(&self, buffer: &mut [u8]);

//     // Write to a DMX instance
//     fn write(&self, dmx: &mut DMX, channel: usize) {
//         self.encode(dmx.slice(channel, channel + self.size()))
//     }
// }
