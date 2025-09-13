use crate::prelude::*;

mod audio;
mod ui;

pub use audio::Audio;

pub struct AudioPlugin;
impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<audio::Audio>()
            .init_resource::<ui::AudioMeter>()
            .add_systems(EguiPrimaryContextPass, ui::audio_ui)
            .add_systems(PreUpdate, audio::audio_emas)
            .add_systems(PostUpdate, audio::audio_reload);
    }
}
