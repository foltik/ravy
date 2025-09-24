use crate::prelude::*;

pub mod motor;

pub struct SimPlugin;

impl Plugin for SimPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreUpdate, motor::zero);
        app.add_systems(PostUpdate, motor::simulate);
    }
}
