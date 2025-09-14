use crate::prelude::*;

mod audio;
mod inspector;
mod ui;
pub mod widgets;

pub use ui::Ui;

pub struct UiPlugin;
impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(bevy_egui::EguiPlugin::default())
            .add_plugins(bevy_inspector_egui::DefaultInspectorConfigPlugin)
            .add_systems(Startup, audio::setup)
            .add_systems(Update, inspector::update_hidden)
            .add_systems(EguiPrimaryContextPass, ui::draw)
            .add_systems(PostUpdate, ui::update_viewport.after(ui::draw));

        app.insert_resource(Ui::default());
    }
}
