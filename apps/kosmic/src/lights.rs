use lib::lights::fixture::{ColoradoSolo, HydroSpot, OutcastBeamwash};
use lib::prelude::*;

use crate::dmx::{HYDROS, OUTCASTS, PARS, Patch, place};

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

impl Lights {
    pub fn reset(&mut self) {
        *self = Default::default();
    }

    /// Pack every fixture into the universe at its patched address, which is
    /// what both the wire and the sim read.
    pub fn encode(&self, patch: &Patch, universe: &mut Universe) {
        universe.0.fill(0);
        for (slot, light) in patch.slots[OUTCASTS].iter().zip(&self.outcasts) {
            place(light, slot.address, universe);
        }
        for (slot, light) in patch.slots[HYDROS].iter().zip(&self.hydros) {
            place(light, slot.address, universe);
        }
        for (slot, light) in patch.slots[PARS].iter().zip(&self.pars) {
            place(light, slot.address, universe);
        }
    }
}
