//! The milstrike fx chain, collapsed into one pass over the feed. Each
//! pattern carries its own settings; switching patterns switches the chain
//! with it.

use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;
use lib::prelude::*;

use super::{Beats, Pattern, Screen};
use crate::logic::State;

/// The fx shader between the feed and the frame.
#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct FxMaterial {
    /// x: seconds since start, y: beats since start (unwrapped), z: flash,
    /// w: red.
    #[uniform(0)]
    pub clock: Vec4,
    /// x: glitch, y: vhs, z: shake, w: mega.
    #[uniform(1)]
    pub warp: Vec4,
    /// x: pause, y: edge, z: invert, w: brightness.
    #[uniform(2)]
    pub grade: Vec4,
    #[texture(3)]
    #[sampler(4)]
    pub feed: Handle<Image>,
}

impl Material for FxMaterial {
    fn fragment_shader() -> ShaderRef {
        "ledwall/fx.wgsl".into()
    }
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Opaque
    }
}

/// One pattern's chain amounts.
#[derive(Clone)]
pub struct FxParams {
    /// Brightness trim for this pattern, on top of the show's master.
    pub trim: f32,
    /// Block skew and channel tear.
    pub glitch: f32,
    /// Tape wobble, interference and channel bleed.
    pub vhs: f32,
    /// Whole-frame jitter.
    pub shake: f32,
    /// The everything-at-once freakout.
    pub mega: f32,
    /// VCR pause: line jitter, colour bands, static wedge.
    pub pause: f32,
    /// Sobel edge extraction.
    pub edge: f32,
    /// Invert the frame.
    pub invert: f32,
    /// Red wash.
    pub red: f32,
    /// White flash.
    pub flash: f32,
}

impl Default for FxParams {
    fn default() -> Self {
        Self {
            trim: 1.0,
            glitch: 0.0,
            vhs: 0.0,
            shake: 0.0,
            mega: 0.0,
            pause: 0.0,
            edge: 0.0,
            invert: 0.0,
            red: 0.0,
            flash: 0.0,
        }
    }
}

/// The chain's settings per pattern; [`drive`] applies the active one.
#[derive(Resource, Clone)]
pub struct Fx {
    pub isolines: FxParams,
    pub cuberoom: FxParams,
    pub spiral: FxParams,
    pub ball: FxParams,
    pub wormhole: FxParams,
    pub kaleido: FxParams,
    pub rings: FxParams,
    pub boxes: FxParams,
}

impl Default for Fx {
    fn default() -> Self {
        Self {
            isolines: FxParams::default(),
            // The cube reads as line work through a cranked edge pass.
            cuberoom: FxParams { edge: 1.0, ..default() },
            spiral: FxParams { edge: 0.4, ..default() },
            // Full edge whites out the checker's colour borders.
            ball: FxParams { edge: 0.3, ..default() },
            wormhole: FxParams { edge: 0.6, ..default() },
            // These three are line work already; edge would hollow them.
            kaleido: FxParams::default(),
            rings: FxParams::default(),
            boxes: FxParams::default(),
        }
    }
}

impl Fx {
    pub fn of(&mut self, pattern: Pattern) -> &mut FxParams {
        match pattern {
            Pattern::Isolines => &mut self.isolines,
            Pattern::CubeRoom => &mut self.cuberoom,
            Pattern::Spiral => &mut self.spiral,
            Pattern::Ball => &mut self.ball,
            Pattern::Wormhole => &mut self.wormhole,
            Pattern::Kaleido => &mut self.kaleido,
            Pattern::Rings => &mut self.rings,
            Pattern::Boxes => &mut self.boxes,
        }
    }

    fn get(&self, pattern: Pattern) -> &FxParams {
        match pattern {
            Pattern::Isolines => &self.isolines,
            Pattern::CubeRoom => &self.cuberoom,
            Pattern::Spiral => &self.spiral,
            Pattern::Ball => &self.ball,
            Pattern::Wormhole => &self.wormhole,
            Pattern::Kaleido => &self.kaleido,
            Pattern::Rings => &self.rings,
            Pattern::Boxes => &self.boxes,
        }
    }
}

pub(super) fn drive(
    screen: Res<Screen>,
    s: Res<State>,
    beats: Res<Beats>,
    fx: Res<Fx>,
    mut materials: ResMut<Assets<FxMaterial>>,
) {
    let Some(mut material) = materials.get_mut(&screen.fx) else { return };
    let params = fx.get(screen.pattern);
    material.clock = Vec4::new(s.t, beats.0, params.flash, params.red);
    material.warp = Vec4::new(params.glitch, params.vhs, params.shake, params.mega);
    material.grade = Vec4::new(params.pause, params.edge, params.invert, s.brightness * params.trim);
}
