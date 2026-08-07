use lib::lights::fixture::{ColoradoSolo, HydroSpot, HydroSpotWheels, OutcastBeamwash};
use lib::prelude::*;

use crate::blackout::Blackout;
use crate::dmx::{HYDROS, OUTCASTS, PARS, Patch, place};
use crate::home::Home;
use crate::sim::Movers;

#[derive(Resource)]
pub struct Lights {
    pub outcasts: [OutcastBeamwash; 2],
    pub hydros: [HydroSpot; 2],
    pub pars: [ColoradoSolo; 6],
}

impl Default for Lights {
    fn default() -> Self {
        Self {
            outcasts: [Default::default(); 2],
            hydros: [Default::default(); 2],
            pars: [Default::default(); 6],
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

    /// Pack every fixture into the universe at its patched address, which is
    /// what both the wire and the sim read. The movers are faded by whatever
    /// their blackout window is worth where they are aimed, which is measured
    /// off where they rest.
    pub fn encode(
        &self,
        patch: &Patch,
        blackout: &Blackout,
        home: &Home,
        movers: &Movers,
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
        for (slot, light) in patch.slots[PARS].iter().zip(&self.pars) {
            place(light, slot.address, universe);
        }
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
        lights.encode(&patch, &blackout, &home, &movers, &mut universe);
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
}
