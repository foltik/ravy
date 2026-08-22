//! GDTF fixtures: parsing a `.gdtf`, assembling it in 3d, and driving it from DMX.

use bevy::asset::io::memory::Dir;

use crate::prelude::*;

mod beam;
mod file;
mod fixture;
pub mod motion;
mod outcast;
mod photometry;
mod trace;

pub use beam::Volumetrics;
pub use file::{Channel, Gdtf, Geometry, Kind, Mode, Model, Slot};
pub use fixture::{
    Axle, Emitter, GdtfDevice, GdtfFixture, GdtfLibrary, GdtfType, Glow, Haze, Motor, Standing,
    Universe, WHEEL_SPEED, corners, declared_travel,
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
        // Builds the acceleration structure the beam pass traces occlusion
        // against. Not solari's scene: its bindings need buffer binding arrays,
        // which metal lacks, and occlusion only ever needs the structure itself.
        app.add_plugins(trace::TraceScenePlugin)
            .add_plugins(beam::BeamPlugin)
            .insert_resource(GdtfLibrary::new(self.models.clone()))
            .init_resource::<Haze>()
            .init_resource::<Glow>()
            .add_systems(Startup, fixture::setup)
            .add_systems(
                Update,
                (beam::pack_gobos, fixture::build, fixture::drive, fixture::apply)
                    .chain()
                    .in_set(GdtfSystems),
            )
            .add_systems(PostUpdate, fixture::project_beams.after(TransformSystems::Propagate));
    }
}
