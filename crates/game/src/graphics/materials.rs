//! Shared custom materials.
//!
//! `GlowMaterial` extends `StandardMaterial` with a pulsing bioluminescent
//! emission and a fresnel rim (shaders/bioluminescence.wesl). It is the
//! base look for glowing organisms and the main proof that our shader
//! pipeline (`shaders://` asset source, WESL imports of Bevy PBR) works.

use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;

pub const BIOLUMINESCENCE_SHADER: &str = "shaders://bioluminescence.wesl";

pub type GlowMaterial = ExtendedMaterial<StandardMaterial, Bioluminescence>;

#[derive(Clone, Copy, Debug, ShaderType, Reflect)]
pub struct BioluminescenceParams {
    /// Emissive color (linear RGB) and intensity multiplier in `w`.
    pub glow: Vec4,
    /// x: pulse speed (Hz), y: pulse depth (0..1), z: rim power, w: phase offset.
    pub pulse: Vec4,
    /// x: vein scale (1/m, 0 = off), y: vein sharpness, z: body amount, w: rim amount.
    pub pattern: Vec4,
}

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct Bioluminescence {
    #[uniform(100)]
    pub params: BioluminescenceParams,
}

impl Bioluminescence {
    /// A soft breathing glow with a rim; refine with the builder methods.
    pub fn new(color: LinearRgba, intensity: f32) -> Self {
        Self {
            params: BioluminescenceParams {
                glow: Vec4::new(color.red, color.green, color.blue, intensity),
                pulse: Vec4::new(0.5, 0.45, 2.5, 0.0),
                pattern: Vec4::new(0.0, 0.5, 0.3, 1.0),
            },
        }
    }

    pub fn pulse(mut self, hz: f32, depth: f32) -> Self {
        self.params.pulse.x = hz;
        self.params.pulse.y = depth.clamp(0.0, 1.0);
        self
    }

    pub fn rim(mut self, power: f32, amount: f32) -> Self {
        self.params.pulse.z = power;
        self.params.pattern.w = amount;
        self
    }

    pub fn body(mut self, amount: f32) -> Self {
        self.params.pattern.z = amount;
        self
    }

    /// Glowing seams from ridged noise (`scale` in 1/m, `sharpness` 0..1).
    pub fn veins(mut self, scale: f32, sharpness: f32) -> Self {
        self.params.pattern.x = scale;
        self.params.pattern.y = sharpness.clamp(0.0, 1.0);
        self
    }
}

impl MaterialExtension for Bioluminescence {
    fn fragment_shader() -> ShaderRef {
        BIOLUMINESCENCE_SHADER.into()
    }
}

pub struct MaterialsPlugin;

impl Plugin for MaterialsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<GlowMaterial>::default());
    }
}
