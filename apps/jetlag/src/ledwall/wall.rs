//! The LED panel material the sim's wall mesh wears over the frame.

use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;
use lib::prelude::*;

use super::{MODULE_M, MODULE_PX, PX, Screen};

/// The physical panel, tweakable live on the `Screen` node. [`tune`] pushes
/// changes into the wall material.
#[derive(Component, Reflect, Clone)]
#[reflect(Component)]
pub struct LedWall {
    /// Black gap between pixel wells in millimetres, an even border all
    /// around; butted modules keep the spacing with a half-gap edge margin.
    pub gap: f32,
    /// Lens diameter as a fraction of the pitch.
    pub lens: f32,
    /// Emissive gain on the frame.
    pub emit: f32,
    /// Off-axis dimming exponent.
    pub falloff: f32,
    /// Light caught by the mask around lit emitters, as a fraction of them.
    pub bleed: f32,
}

impl Default for LedWall {
    fn default() -> Self {
        Self { gap: 0.6, lens: 0.55, emit: 4.0, falloff: 0.65, bleed: 0.15 }
    }
}

pub type LedWallMaterial = ExtendedMaterial<StandardMaterial, LedWallExtension>;

/// The wall mesh's LED panel shader over the black PBR face.
#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct LedWallExtension {
    /// xy: emitter counts, one per frame texel, z: well fill fraction of the
    /// pitch, w: lens diameter fraction.
    #[uniform(100)]
    pub geometry: Vec4,
    /// x: emissive gain, y: off-axis exponent, z: mask bleed.
    #[uniform(100)]
    pub tuning: Vec4,
    #[texture(101)]
    #[sampler(102)]
    pub frame: Handle<Image>,
}

impl LedWallExtension {
    pub(super) fn new(wall: &LedWall, frame: Handle<Image>) -> Self {
        let mut this = Self { geometry: Vec4::ZERO, tuning: Vec4::ZERO, frame };
        this.set(wall);
        this
    }

    fn set(&mut self, wall: &LedWall) {
        let pitch = MODULE_M * 1000.0 / MODULE_PX as f32;
        let well = 1.0 - (wall.gap / pitch).clamp(0.05, 0.95);
        self.geometry =
            Vec4::new(PX.x as f32, PX.y as f32, well, wall.lens.clamp(0.05, well));
        self.tuning = Vec4::new(wall.emit, wall.falloff, wall.bleed, 0.0);
    }
}

impl MaterialExtension for LedWallExtension {
    fn fragment_shader() -> ShaderRef {
        "ledwall/wall.wgsl".into()
    }
}

/// Push `LedWall` tweaks into the wall material.
pub(super) fn tune(
    walls: Query<&LedWall, Changed<LedWall>>,
    screen: Res<Screen>,
    mut materials: ResMut<Assets<LedWallMaterial>>,
) {
    let Ok(wall) = walls.single() else { return };
    let Some(mut material) = materials.get_mut(&screen.wall) else { return };
    material.extension.set(wall);
}
