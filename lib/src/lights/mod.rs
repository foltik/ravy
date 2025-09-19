use std::path::Path;

use crate::prelude::*;

pub mod fixture;
pub mod layout;

mod moving_head;
use bevy::asset::io::memory::Dir;
pub use moving_head::MovingHeadDevice;

pub struct LightsPlugin {
    pub models: Dir,
}

impl Plugin for LightsPlugin {
    fn build(&self, app: &mut App) {
        // XXX: rethink this mess
        app.register_component_as::<dyn MovingHeadDevice, crate::lights::fixture::EliminatorStealthBeam>();
        self.models.insert_asset(
            Path::new(crate::lights::fixture::EliminatorStealthBeam::default().model_path()),
            crate::lights::fixture::EliminatorStealthBeam::default().model(),
        );

        app.add_systems(
            PreUpdate,
            (moving_head::setup_pre, moving_head::setup_post, moving_head::update),
        );
    }
}
