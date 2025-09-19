use std::path::Path;

use bevy::asset::io::memory::Dir;

use crate::prelude::*;

pub mod fixture;
pub mod layout;

mod moving_head;
pub use moving_head::MovingHeadDevice;

mod spot;
pub use spot::SpotDevice;

pub struct LightsPlugin {
    pub models: Dir,
}

impl Plugin for LightsPlugin {
    fn build(&self, app: &mut App) {
        // XXX: rethink this mess
        app.register_component_as::<dyn MovingHeadDevice, crate::lights::fixture::StealthBeam>();
        self.models.insert_asset(
            Path::new(crate::lights::fixture::StealthBeam::default().model_path()),
            crate::lights::fixture::StealthBeam::default().model(),
        );

        app.register_component_as::<dyn SpotDevice, crate::lights::fixture::SaberSpot>();
        self.models.insert_asset(
            Path::new(crate::lights::fixture::SaberSpot::default().model_path()),
            crate::lights::fixture::SaberSpot::default().model(),
        );

        app.add_systems(
            PreUpdate,
            (moving_head::setup_pre, moving_head::setup_post, moving_head::update),
        );

        app.add_systems(PreUpdate, (spot::setup_pre, spot::setup_post, spot::update));
    }
}
