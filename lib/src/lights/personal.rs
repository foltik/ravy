use crate::dmx::device::bar_rgb_18w::Bar;
use crate::dmx::device::beam_rgbw_60w::Beam;
use crate::dmx::device::laser_scan_30w::Laser;
use crate::dmx::device::par_rgbw_12x3w::Par;
use crate::dmx::device::spider_rgbw_8x10w::Spider;
use crate::dmx::device::strobe_rgb_35w::Strobe;
use crate::prelude::*;

/// My personal collection of lights.
#[derive(Default)]
pub struct Personal {
    pub pars: [Par; 10],
    pub beams: [Beam; 4],
    pub bars: [Bar; 2],
    pub spiders: [Spider; 2],
    pub strobe: Strobe,
    pub laser: Laser,
}

impl Personal {
    pub fn reset(&mut self) {
        *self = Default::default();
    }
}

impl Personal {
    /// Pars and bars one color, spiders and bars another
    pub fn split(&mut self, col0: Rgbw, col1: Rgbw) {
        self.for_each_par(|par, _i, _fr| par.color = col0);
        self.for_each_beam(|beam, _i, _fr| beam.color = col1);
        self.for_each_spider(|spider, _i, _fr| {
            spider.color0 = col1;
            spider.color1 = col1;
        });
        self.for_each_bar(|bar, _i, _fr| bar.color = col1.into());
        self.strobe.color = col0.into();
    }

    /// Apply a function to the color of each light
    pub fn map_colors(&mut self, mut f: impl FnMut(Rgbw) -> Rgbw) {
        self.for_each_par(|par, _i, _fr| par.color = f(par.color));
        self.for_each_beam(|beam, _i, _fr| beam.color = f(beam.color));
        self.for_each_spider(|spider, _i, _fr| {
            spider.color0 = f(spider.color0);
            spider.color1 = f(spider.color1);
        });
        self.for_each_bar(|bar, _i, _fr| bar.color = f(bar.color.into()).into());
        self.strobe.color = f(self.strobe.color.into()).into();
    }

    // Iterate through the lights, with additional index and fr (from 0 to 1) parameters.
    pub fn for_each_par(&mut self, f: impl FnMut(&mut Par, usize, f64)) {
        Self::for_each(&mut self.pars, f);
    }
    pub fn for_each_beam(&mut self, f: impl FnMut(&mut Beam, usize, f64)) {
        Self::for_each(&mut self.beams, f);
    }
    pub fn for_each_bar(&mut self, f: impl FnMut(&mut Bar, usize, f64)) {
        Self::for_each(&mut self.bars, f);
    }
    pub fn for_each_spider(&mut self, f: impl FnMut(&mut Spider, usize, f64)) {
        Self::for_each(&mut self.spiders, f);
    }

    fn for_each<T>(slice: &mut [T], mut f: impl FnMut(&mut T, usize, f64)) {
        let n = slice.len();
        slice.iter_mut().enumerate().for_each(|(i, t)| f(t, i, i as f64 / n as f64));
    }
}

impl DmxUniverse for Personal {
    fn send(&self, e131: &mut E131) {
        let mut dmx = [0u8; 205];

        for (i, par) in self.pars.iter().enumerate() {
            par.encode(&mut dmx[1 + 8 * i..]);
        }
        for (i, beam) in self.beams.iter().enumerate() {
            let beam = Beam { alpha: beam.alpha * 0.5, ..beam.clone() };
            beam.encode(&mut dmx[81 + 15 * i..]);
        }
        for (i, bar) in self.bars.iter().enumerate() {
            if bar.color.0 == bar.color.1 && bar.color.1 == bar.color.2 {
                Bar { alpha: 0.0, color: Rgbw::BLACK }.encode(&mut dmx[149 + 7 * i..]);
            } else {
                bar.encode(&mut dmx[149 + 7 * i..]);
            }
        }
        for (i, spider) in self.spiders.iter().enumerate() {
            spider.encode(&mut dmx[175 + 15 * i..]);
        }
        self.strobe.encode(&mut dmx[142..]);
        self.laser.encode(&mut dmx[164..]);

        e131.send(&dmx);
    }
}
