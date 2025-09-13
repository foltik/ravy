use crate::e131::E131;

pub mod device;

pub trait DmxDevice {
    fn channels(&self) -> usize;
    fn encode(&self, buf: &mut [u8]);
}

pub trait DmxUniverse {
    fn send(&self, e131: &mut E131);
}
