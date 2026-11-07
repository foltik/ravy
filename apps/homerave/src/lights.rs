//! Every fixture in the house, and the one frame they are packed into.
//!
//! The classic rig (the mslive pars, beams, spiders, bars, strobe, laser and
//! hazer)
//! holds the addresses set on the fixtures' own panels, 1..256. The big rig
//! (Outcasts, Hydros, Colorados) is addressed by RDM through [`Patch`] and
//! sits above it. The ENTTEC carries the lot; the E1.31 node, when it is on
//! the wire, gets the same frame.

use lib::dmx::device::bar_rgb_18w::Bar;
use lib::dmx::device::beam_rgbw_60w::Beam;
use lib::dmx::device::beam_rgbw_90w::BigBeam;
use lib::dmx::device::hazer_hurricane_haze_4d::Hazer;
use lib::dmx::device::laser_scan_30w::{Laser, LaserColor, LaserPattern};
use lib::dmx::device::par_rgbw_12x3w::Par;
use lib::dmx::device::spider_rgbw_8x10w::Spider;
use lib::dmx::device::strobe_rgb_35w::Strobe;
use lib::lights::fixture::{ColoradoSolo, HydroSpot, HydroSpotWheels, OutcastBeamwash};
use lib::prelude::*;

use crate::blackout::Blackout;
use crate::dmx::{HYDROS, OUTCASTS, PARS, Patch, place};
use crate::home::Home;
use crate::sim::Movers;

///////////////////////// CLASSIC ADDRESSES /////////////////////////

/// 1-based DMX start addresses, as set on the fixtures' own panels:
///   Pars (8CH)      1..72      <- pars[0..9]
///   (free)          73..80     the tenth par, which broke
///   Beams (15CH)    81..140    <- beams[0..4]
///   Strobe (6CH)    142..147
///   Bars (7CH)      149..162   <- bars[0..2]
///   Laser (10CH)    163..172
///   Hazer (2CH)     173..174
///   Spiders (15CH)  175..204   <- spiders[0..2]
///   BigBeams (13CH) 205..256   <- bigbeams[0..4]
pub const PAR_ADDRS: [usize; 9] = [1, 9, 17, 25, 33, 41, 49, 57, 65];
pub const BEAM_ADDRS: [usize; 4] = [81, 96, 111, 126];
pub const STROBE_ADDR: usize = 142;
pub const BAR_ADDRS: [usize; 2] = [149, 156];
pub const LASER_ADDR: usize = 163;
pub const HAZER_ADDR: usize = 173;
pub const SPIDER_ADDRS: [usize; 2] = [175, 190];
pub const BIGBEAM_ADDRS: [usize; 4] = [205, 218, 231, 244];

/// Highest occupied classic channel; the big rig is patched above it.
pub const LAST_CH: usize = 256;

/// The address each classic world fixture drives, world index in. Defaults
/// to the layout above in scene order; the E1.31 panel's assignments permute
/// it to match how the rig was actually stood up.
pub struct Addressing {
    pub pars: [usize; 9],
    pub beams: [usize; 4],
    pub bigbeams: [usize; 4],
    pub bars: [usize; 2],
    pub spiders: [usize; 2],
    pub strobe: usize,
    pub laser: usize,
    pub hazer: usize,
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
            laser: LASER_ADDR,
            hazer: HAZER_ADDR,
        }
    }
}

/// The laser's one gesture so far: a big circle in the middle of its throw.
/// Size is the scan amplitude and x/y the centre, all of them `0..1`; the
/// edges of the throw clip, so the centre sits at 0.5 and the size short of
/// full.
pub fn laser_circle(color: LaserColor) -> Laser {
    Laser {
        on: true,
        pattern: LaserPattern::Circle,
        color,
        size: 0.75,
        x: 0.5,
        y: 0.5,
        ..default()
    }
}

///////////////////////// LIGHTS /////////////////////////

#[derive(Resource)]
pub struct Lights {
    pub outcasts: [OutcastBeamwash; 2],
    pub hydros: [HydroSpot; 2],
    pub colorados: [ColoradoSolo; 6],

    pub pars: [Par; 9],
    pub beams: [Beam; 4],
    pub bigbeams: [BigBeam; 4],
    pub spiders: [Spider; 2],
    pub bars: [Bar; 2],
    pub strobe: Strobe,
    pub laser: Laser,
    pub hazer: Hazer,
}

impl Default for Lights {
    fn default() -> Self {
        Self {
            outcasts: [Default::default(); 2],
            hydros: [Default::default(); 2],
            colorados: [Default::default(); 6],
            pars: Default::default(),
            beams: Default::default(),
            bigbeams: Default::default(),
            spiders: Default::default(),
            bars: Default::default(),
            strobe: Default::default(),
            laser: Default::default(),
            hazer: Default::default(),
        }
    }
}

/// How far along each hydro's wheels are, so the show can be blanked while they
/// turn. Kept out of [`Lights`], which is rebuilt every frame. The panel drives
/// its own copy for the fixtures it has taken over.
#[derive(Resource, Default)]
pub struct Wheels(pub [HydroSpotWheels; 2]);

impl Wheels {
    /// Follow where each hydro's wheels are being sent and hold it dark while
    /// they travel. Runs on the values headed for the wire, so the rig is cut
    /// along with the sim.
    pub fn mask(&mut self, hydros: &mut [HydroSpot; 2], dt: f32) {
        for (wheels, light) in self.0.iter_mut().zip(hydros) {
            wheels.mask(light, dt);
        }
    }
}

impl Lights {
    pub fn reset(&mut self) {
        *self = Default::default();
    }

    /// Pack every fixture into the universe, which is what the wire and the
    /// sim read. The big movers are faded by whatever their blackout window
    /// is worth where they are aimed, which is measured off where they rest.
    pub fn encode(
        &self,
        patch: &Patch,
        blackout: &Blackout,
        home: &Home,
        movers: &Movers,
        addrs: &Addressing,
        universe: &mut Universe,
    ) {
        universe.0.fill(0);
        for (i, (slot, light)) in patch.slots[OUTCASTS].iter().zip(&self.outcasts).enumerate() {
            let (mut light, at) = (*light, OUTCASTS.start + i);
            blackout.wash(at, &home.movers[at], movers.aim[at], &mut light);
            place(&light, slot.address, universe);
        }
        for (i, (slot, light)) in patch.slots[HYDROS].iter().zip(&self.hydros).enumerate() {
            let (mut light, at) = (*light, HYDROS.start + i);
            blackout.spot(at, &home.movers[at], movers.aim[at], &mut light);
            place(&light, slot.address, universe);
        }
        for (slot, light) in patch.slots[PARS].iter().zip(&self.colorados) {
            place(light, slot.address, universe);
        }

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
        place(&self.laser, addrs.laser, universe);
        place(&self.hazer, addrs.hazer, universe);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blackout::{Arc, WINDOWS, Zone, direction};
    use crate::dmx::MOVERS;
    use crate::home::Rest;

    /// A mover aimed into its window leaves the wire dimmed. Everything between
    /// the panel and the ENTTEC is here: the window is applied where the show
    /// packs the universe, so what the sim draws and what the rig is sent can
    /// only ever be the same frame.
    #[test]
    fn the_window_reaches_the_wire() {
        let patch = Patch::default();
        let shut = Zone {
            pitch: Arc { at: 60.0, width: 20.0, fade: 20.0 },
            yaw: Arc { at: 45.0, width: 20.0, fade: 20.0 },
            level: 0.0,
            ..default()
        };
        // The first of each pair has the window; nothing else has one.
        let (some, none) = ([shut, Zone::default()], [Zone::default(); WINDOWS]);
        let blackout = Blackout { movers: [some, none, some, none] };

        let mut lights = Lights::default();
        for light in &mut lights.outcasts {
            (light.ring, light.center) = (Rgbw::WHITE, Rgbw::WHITE);
        }
        for light in &mut lights.hydros {
            light.alpha = 1.0;
        }

        // One of each pair pointing into the window it has, the other straight
        // down and well clear of it.
        let mut movers = Movers::default();
        let (into, clear) = (Some(direction(60.0, 45.0)), Some(Vec3::NEG_Y));
        movers.aim = [into, clear, into, clear];
        // Resting in the middle of the travel, so the window's bearing is the
        // one the aims above are written in.
        let home = Home { movers: [Rest::default(); MOVERS.len()] };
        let mut universe = Universe::default();
        lights.encode(&patch, &blackout, &home, &movers, &Addressing::default(), &mut universe);
        let dimmer = |slot: usize, channel: usize| {
            universe.0[patch.slots[slot].address - 1 + channel]
        };
        assert_eq!(MOVERS.len(), 4);
        // Outcast ring is channel 16, its centre 18, the hydro's dimmer 10.
        assert_eq!((dimmer(0, 15), dimmer(0, 17)), (0, 0), "the outcast in the window");
        assert_eq!((dimmer(1, 15), dimmer(1, 17)), (255, 255), "the outcast clear of it");
        assert_eq!(dimmer(2, 9), 0, "the hydro in the window");
        assert_eq!(dimmer(3, 9), 255, "the hydro clear of it");
    }

    /// Every fixture of both rigs lands where its table says, inside one
    /// universe, and nothing overlaps.
    #[test]
    fn the_whole_rig_fits_one_universe() {
        let patch = Patch::default();
        let spans: Vec<(usize, usize)> = PAR_ADDRS
            .iter()
            .map(|&a| (a, 8))
            .chain(BEAM_ADDRS.iter().map(|&a| (a, 15)))
            .chain([(STROBE_ADDR, 6), (LASER_ADDR, 10), (HAZER_ADDR, 2)])
            .chain(BAR_ADDRS.iter().map(|&a| (a, 7)))
            .chain(SPIDER_ADDRS.iter().map(|&a| (a, 15)))
            .chain(BIGBEAM_ADDRS.iter().map(|&a| (a, 13)))
            .chain(patch.slots.iter().map(|s| (s.address, s.footprint)))
            .collect();
        let mut taken = [false; 513];
        for (addr, footprint) in spans {
            for ch in addr..addr + footprint {
                assert!(ch <= 512, "channel {ch} past the end of the universe");
                assert!(!taken[ch], "channel {ch} claimed twice");
                taken[ch] = true;
            }
        }
        assert!(patch.slots.iter().all(|s| s.address > LAST_CH), "the big rig sits above the classic rig");
    }

    /// A lit classic fixture reaches its frame at its own address.
    #[test]
    fn encode_reaches_the_classic_addresses() {
        let mut lights = Lights::default();
        lights.pars[8].color = Rgbw::WHITE;
        lights.strobe.alpha = 1.0;
        let mut universe = Universe::default();
        let (patch, blackout, home, movers) = (Patch::default(), Blackout::default(), Home::default(), Movers::default());
        lights.encode(&patch, &blackout, &home, &movers, &Addressing::default(), &mut universe);
        // Par 9 dimmer is its channel 4; the strobe's dimmer its channel 1.
        assert_eq!(universe.0[PAR_ADDRS[8] - 1 + 3], 255);
        assert_eq!(universe.0[STROBE_ADDR - 1], 255);
    }
}
