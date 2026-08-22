//! The classic rig, at the addresses the fixtures have held since mslive.
//!
//! The 60w Beams and 90w BigBeams stand interleaved in one line across the
//! stage, each bank behind its own trim; see `MOVERS` for the stage order.

use lib::dmx::device::bar_rgb_18w::Bar;
use lib::dmx::device::beam_rgbw_60w::Beam;
use lib::dmx::device::beam_rgbw_90w::BigBeam;
use lib::dmx::device::par_rgbw_12x3w::Par;
use lib::dmx::device::spider_rgbw_8x10w::Spider;
use lib::dmx::device::strobe_rgb_35w::Strobe;
use lib::prelude::*;

/// 1-based DMX start addresses, as set on the fixtures' own panels:
///   Pars (8CH)      1..72      <- pars[0..9]
///   (free)          73..80     the tenth par, which broke
///   Beams (15CH)    81..140    <- beams[0..4]
///   Strobe (6CH)    142..147
///   Bars (7CH)      149..162   <- bars[0..2]
///   (free)          164..174   reserved for the hazer + fogger
///   Spiders (15CH)  175..204   <- spiders[0..2]
///   BigBeams (13CH) 205..256   <- bigbeams[0..4]
pub const PAR_ADDRS: [usize; 9] = [1, 9, 17, 25, 33, 41, 49, 57, 65];
pub const BEAM_ADDRS: [usize; 4] = [81, 96, 111, 126];
pub const STROBE_ADDR: usize = 142;
pub const BAR_ADDRS: [usize; 2] = [149, 156];
pub const SPIDER_ADDRS: [usize; 2] = [175, 190];
pub const BIGBEAM_ADDRS: [usize; 4] = [205, 218, 231, 244];

/// Highest occupied channel, which is all of the frame worth sending.
pub const LAST_CH: usize = 256;

#[derive(Resource)]
pub struct Lights {
    pub pars: [Par; 9],
    pub beams: [Beam; 4],
    pub bigbeams: [BigBeam; 4],
    pub bars: [Bar; 2],
    pub spiders: [Spider; 2],
    pub strobe: Strobe,
}

impl Default for Lights {
    fn default() -> Self {
        Self {
            pars: Default::default(),
            beams: Default::default(),
            bigbeams: Default::default(),
            bars: Default::default(),
            spiders: Default::default(),
            strobe: Default::default(),
        }
    }
}

/// The address each world fixture drives, world index in. Defaults to the
/// classic layout in scene order; the DMX panel's assignments permute it to
/// match how the rig was actually stood up.
pub struct Addressing {
    pub pars: [usize; 9],
    pub beams: [usize; 4],
    pub bigbeams: [usize; 4],
    pub bars: [usize; 2],
    pub spiders: [usize; 2],
    pub strobe: usize,
}

impl Default for Addressing {
    fn default() -> Self {
        Self {
            pars: PAR_ADDRS,
            beams: BEAM_ADDRS,
            bigbeams: BIGBEAM_ADDRS,
            bars: BAR_ADDRS,
            spiders: SPIDER_ADDRS,
            strobe: STROBE_ADDR,
        }
    }
}

impl Lights {
    pub fn reset(&mut self) {
        *self = Default::default();
    }

    /// Pack every fixture into the universe, which is what the wire reads.
    pub fn encode(&self, universe: &mut Universe, addrs: &Addressing) {
        universe.0.fill(0);
        for (light, addr) in self.pars.iter().zip(addrs.pars) {
            place(light, addr, universe);
        }
        for (light, addr) in self.beams.iter().zip(addrs.beams) {
            place(light, addr, universe);
        }
        for (light, addr) in self.bigbeams.iter().zip(addrs.bigbeams) {
            place(light, addr, universe);
        }
        for (light, addr) in self.bars.iter().zip(addrs.bars) {
            place(light, addr, universe);
        }
        for (light, addr) in self.spiders.iter().zip(addrs.spiders) {
            place(light, addr, universe);
        }
        place(&self.strobe, addrs.strobe, universe);
    }
}

/// Pack a fixture into the universe at a 1-based address.
pub fn place(light: &impl DmxDevice, address: usize, universe: &mut Universe) {
    let start = address.saturating_sub(1);
    if start + light.channels() <= universe.0.len() {
        light.encode(&mut universe.0[start..]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every fixture lands where the classic layout says, and nothing overlaps.
    #[test]
    fn the_classic_layout_is_disjoint() {
        let spans: Vec<(usize, usize)> = PAR_ADDRS
            .iter()
            .map(|&a| (a, 8))
            .chain(BEAM_ADDRS.iter().map(|&a| (a, 15)))
            .chain([(STROBE_ADDR, 6)])
            .chain(BAR_ADDRS.iter().map(|&a| (a, 7)))
            .chain(SPIDER_ADDRS.iter().map(|&a| (a, 15)))
            .chain(BIGBEAM_ADDRS.iter().map(|&a| (a, 13)))
            .collect();
        let mut taken = [false; LAST_CH + 1];
        for (addr, footprint) in spans {
            for ch in addr..addr + footprint {
                assert!(ch <= LAST_CH, "channel {ch} past the end of the frame");
                assert!(!taken[ch], "channel {ch} claimed twice");
                taken[ch] = true;
            }
        }
    }

    /// A lit fixture reaches the frame at its own address.
    #[test]
    fn encode_reaches_the_classic_addresses() {
        let mut lights = Lights::default();
        lights.pars[8].color = Rgbw::WHITE;
        lights.strobe.alpha = 1.0;
        let mut universe = Universe::default();
        lights.encode(&mut universe, &Addressing::default());
        // Par 9 dimmer is its channel 4; the strobe's dimmer its channel 1.
        assert_eq!(universe.0[PAR_ADDRS[8] - 1 + 3], 255);
        assert_eq!(universe.0[STROBE_ADDR - 1], 255);
    }
}
