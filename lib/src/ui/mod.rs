use crate::prelude::*;

mod audio_inspector;
mod inspector;
mod ui;
mod utils;
pub mod widgets;

pub use ui::Ui;
pub use utils::*;

pub struct UiPlugin;
impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(bevy_egui::EguiPlugin::default())
            .add_plugins(bevy_inspector_egui::DefaultInspectorConfigPlugin)
            .add_systems(Startup, audio_inspector::setup)
            .add_systems(Update, inspector::update_hidden)
            .add_systems(EguiPrimaryContextPass, ui::draw)
            .add_systems(PostUpdate, ui::update_viewport.after(ui::draw))
            .insert_resource(Ui::default());
    }
}
