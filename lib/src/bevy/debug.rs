use bevy::prelude::*;

pub struct DebugPlugin;

impl Plugin for DebugPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Debug>();
    }
}

#[derive(Resource, Default)]
pub struct Debug {
    pub ui: bool,
}
