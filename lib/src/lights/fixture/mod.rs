mod adj_stealth_beam;
pub use adj_stealth_beam::StealthBeam;

mod adj_saber_spot;
pub use adj_saber_spot::SaberSpot;

mod adj_hydro_spot;
pub use adj_hydro_spot::{
    HydroSpot, HydroSpotGobo, HydroSpotPrism, HydroSpotWheel1, HydroSpotWheel2, HydroSpotWheels,
};

mod chauvet_outcast_beamwash;
pub use chauvet_outcast_beamwash::{OutcastBeamwash, OutcastBeamwashRingPattern};

mod chauvet_colorado_solo;
pub use chauvet_colorado_solo::ColoradoSolo;

use crate::dmx::RdmDevice;
use crate::gdtf::{GdtfDevice, motion};

/// Measured travel for the fixture with this RDM identity, if one of these types
/// covers it.
///
/// [`crate::gdtf::GdtfFixture::of`] reads it straight off the type, but a sim
/// that loaded a `.gdtf` by path has no static type to ask, so it matches on the
/// identity the file declares instead. Without this such a fixture silently gets
/// the generic pan/tilt/zoom defaults.
pub fn motion_for(manufacturer: u16, model: u16) -> Option<[motion::Params; 3]> {
    fn entry<D: GdtfDevice + RdmDevice>() -> (u16, u16, [motion::Params; 3]) {
        (D::MANUFACTURER, D::MODEL, D::motion())
    }
    // Every type with its own `GdtfDevice::motion`.
    [entry::<HydroSpot>(), entry::<ColoradoSolo>(), entry::<OutcastBeamwash>()]
        .into_iter()
        .find(|&(m, d, _)| (m, d) == (manufacturer, model))
        .map(|(_, _, params)| params)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sim that loaded a `.gdtf` by path has only the file's RDM identity to go
    /// on, and used to fall back to the generic defaults without saying so.
    #[test]
    fn measured_travel_is_found_by_rdm_identity() {
        let fitted = motion_for(OutcastBeamwash::MANUFACTURER, OutcastBeamwash::MODEL)
            .expect("the outcast declares its own travel");
        assert_eq!(fitted, OutcastBeamwash::motion());
        assert_ne!(fitted[0], motion::Params::pan(), "should not be the generic default");
        assert_eq!(motion_for(0xdead, 0xbeef), None);
    }

    /// Nothing in the list may share an identity, or the wrong travel would win.
    #[test]
    fn identities_are_unique() {
        let ids = [
            (HydroSpot::MANUFACTURER, HydroSpot::MODEL),
            (ColoradoSolo::MANUFACTURER, ColoradoSolo::MODEL),
            (OutcastBeamwash::MANUFACTURER, OutcastBeamwash::MODEL),
        ];
        for (i, a) in ids.iter().enumerate() {
            assert!(!ids[..i].contains(a), "duplicate rdm identity {a:?}");
        }
    }
}
