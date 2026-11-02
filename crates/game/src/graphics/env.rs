//! Environment lighting. Underwater strata use a procedural gradient
//! cubemap built from a per-stratum palette; Bevy filters it on the GPU
//! (`GeneratedEnvironmentMapLight`). Sky HDRIs from the asset pipeline
//! (`hdri.*` in the manifest) take over from the Coast stratum (M4).

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{
    Extent3d, TextureDimension, TextureFormat, TextureViewDescriptor, TextureViewDimension,
};
use game_core::half::f32_to_f16_bits;

/// Colors of the procedural environment.
#[derive(Debug, Clone, Copy)]
pub struct EnvPalette {
    /// Light from above (surface glow), linear RGB.
    pub zenith: Vec3,
    /// Horizon water color, linear RGB.
    pub horizon: Vec3,
    /// Abyss below, linear RGB.
    pub nadir: Vec3,
    /// Direction toward the bright patch (sun through the surface).
    pub sun_dir: Vec3,
    pub sun_color: Vec3,
    /// Higher = tighter sun glow.
    pub sun_sharpness: f32,
}

impl EnvPalette {
    /// Tier 1 (Impact Site) palette: dim teal above, deep navy below.
    pub const SEAFLOOR: EnvPalette = EnvPalette {
        zenith: Vec3::new(0.10, 0.35, 0.45),
        horizon: Vec3::new(0.01, 0.06, 0.10),
        nadir: Vec3::new(0.002, 0.008, 0.02),
        sun_dir: Vec3::new(0.2, 1.0, -0.3),
        sun_color: Vec3::new(0.6, 0.9, 1.0),
        sun_sharpness: 24.0,
    };

    pub fn sample(&self, dir: Vec3) -> Vec3 {
        let d = dir.normalize();
        let base = if d.y >= 0.0 {
            self.horizon.lerp(self.zenith, d.y.powf(0.6))
        } else {
            self.horizon.lerp(self.nadir, (-d.y).powf(0.4))
        };
        let sun = d
            .dot(self.sun_dir.normalize())
            .max(0.0)
            .powf(self.sun_sharpness);
        base + self.sun_color * sun
    }
}

/// Handle to the active environment cubemap.
#[derive(Resource, Debug, Clone)]
pub struct EnvironmentCubemap {
    pub image: Handle<Image>,
}

pub struct EnvironmentPlugin;

impl Plugin for EnvironmentPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, setup_environment);
    }
}

fn setup_environment(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    commands.insert_resource(EnvironmentCubemap {
        image: images.add(procedural_cubemap(&EnvPalette::SEAFLOOR, 128)),
    });
}

/// Cubemap face directions in wgpu layer order (+X, -X, +Y, -Y, +Z, -Z).
/// `u`, `v` in [-1, 1], `v` pointing down the image.
pub fn cube_face_direction(face: usize, u: f32, v: f32) -> Vec3 {
    match face {
        0 => Vec3::new(1.0, -v, -u),
        1 => Vec3::new(-1.0, -v, u),
        2 => Vec3::new(u, 1.0, v),
        3 => Vec3::new(u, -1.0, -v),
        4 => Vec3::new(u, -v, 1.0),
        _ => Vec3::new(-u, -v, -1.0),
    }
}

/// Builds an Rgba16Float cubemap of `size`×`size` per face from a palette.
pub fn procedural_cubemap(palette: &EnvPalette, size: u32) -> Image {
    assert!(size.is_power_of_two(), "env cubemaps must be power-of-two");
    let n = size as usize;
    let mut data = Vec::with_capacity(n * n * 6 * 8);
    for face in 0..6 {
        for y in 0..n {
            for x in 0..n {
                let u = 2.0 * (x as f32 + 0.5) / n as f32 - 1.0;
                let v = 2.0 * (y as f32 + 0.5) / n as f32 - 1.0;
                let c = palette.sample(cube_face_direction(face, u, v));
                for ch in [c.x, c.y, c.z, 1.0] {
                    data.extend_from_slice(&f32_to_f16_bits(ch).to_le_bytes());
                }
            }
        }
    }
    let mut image = Image::new(
        Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 6,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba16Float,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::Cube),
        ..default()
    });
    image
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cube_faces_point_outward() {
        let axes = [Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, Vec3::Z, -Vec3::Z];
        for (face, axis) in axes.iter().enumerate() {
            assert_eq!(cube_face_direction(face, 0.0, 0.0), *axis);
        }
    }

    #[test]
    fn palette_is_brighter_above() {
        let p = EnvPalette::SEAFLOOR;
        assert!(p.sample(Vec3::Y).length() > p.sample(-Vec3::Y).length());
    }
}
