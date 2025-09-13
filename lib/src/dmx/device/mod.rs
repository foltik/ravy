pub mod bar_rgb_18w;
pub mod beam_rgbw_60w;
pub mod beam_rgbw_90w;
pub mod gobo_60w;
pub mod laser_array;
pub mod laser_scan_30w;
pub mod par_rgbw_12x3w;
pub mod spider_rgbw_8x10w;
pub mod strobe_rgb_35w;

pub trait Device {
    fn channels(&self) -> usize;
    fn encode(&self, buf: &mut [u8]);
}
