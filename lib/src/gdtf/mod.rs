//! GDTF fixtures: parsing a `.gdtf`, assembling it in 3d, and driving it from DMX.

use bevy::asset::io::memory::Dir;

use crate::prelude::*;

mod beam;
mod file;
mod fixture;
pub mod motion;
mod photometry;

pub use beam::BeamMaterial;
pub use file::{Channel, Gdtf, Geometry, Kind, Mode, Model, Slot};
pub use fixture::{
    Axle, BeamCone, Emitter, GdtfDevice, GdtfFixture, GdtfLibrary, GdtfType, Haze, Motor, Standing,
    Universe, corners,
};
pub use photometry::{Photometry, Profile, luminance, xy_to_rgb};

pub struct GdtfPlugin {
    /// Backs the `gdtf://` asset source that part meshes are served from.
    pub models: Dir,
}

/// Everything that reads the [`Universe`]. Whatever writes the frame has to be
/// ordered `before` this, or the fixtures pick up a half-written one.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct GdtfSystems;

impl Plugin for GdtfPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(beam::BeamPlugin)
            .insert_resource(GdtfLibrary::new(self.models.clone()))
            .init_resource::<Haze>()
            .add_systems(Startup, fixture::setup)
            .add_systems(
                Update,
                (fixture::build, fixture::drive, fixture::apply, fixture::mip_gobos)
                    .chain()
                    .in_set(GdtfSystems),
            )
            .add_systems(PostUpdate, fixture::project_beams.after(TransformSystems::Propagate));
    }
}
