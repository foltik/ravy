use crate::prelude::*;

mod audio;
mod peak;
mod vu;

pub use audio::Audio;
pub use peak::AudioPeakHold;
pub use vu::AudioVU;

pub struct AudioPlugin;
impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<audio::Audio>()
            .add_systems(PreUpdate, (peak::update, vu::update))
            .add_systems(PostUpdate, audio::reload);
    }
}
