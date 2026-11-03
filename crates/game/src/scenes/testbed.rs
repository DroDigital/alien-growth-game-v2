//! Testbed scene: the Tier 1 "Impact Site" mood in grey-box.
//!
//! A displaced seafloor with a meteor crater, scattered basalt rocks, the
//! glowing meteor fragment, the organism, bioluminescent motes, marine snow
//! (GPU particles), distance fog, volumetric shafts (Medium+) and the
//! environment map. Everything is procedural, so it renders without any
//! downloaded asset; fetched textures replace the flat materials when present.

use super::{OrbitShot, SceneMember};
use crate::boot::{ActiveRenderSettings, AssetAvailability};
use crate::graphics::env::EnvironmentCubemap;
use crate::graphics::materials::{Bioluminescence, GlowMaterial};
use bevy::asset::RenderAssetUsages;
use bevy::ecs::system::SystemParam;
use bevy::light::{
    CascadeShadowConfigBuilder, FogVolume, GeneratedEnvironmentMapLight, NotShadowCaster, Skybox,
    VolumetricLight,
};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::pbr::{DistanceFog, ExtendedMaterial, FogFalloff};
use bevy::prelude::*;
use bevy_hanabi::prelude::*;
use noise::{Fbm, MultiFractal, NoiseFn, Perlin};
use rand::{RngExt, SeedableRng};
use rand_chacha::ChaCha8Rng;

/// Fixed seed: the testbed must render identically every time.
const SEED: u64 = 0x5EAF_100D;
const FLOOR_SIZE: f32 = 160.0;
const FLOOR_RES: u32 = 160;
const CRATER_RADIUS: f32 = 7.0;
/// Fog color: matches the environment palette's horizon so distant geometry
/// dissolves into the background.
const WATER_COLOR: Color = Color::linear_rgb(0.01, 0.06, 0.10);

pub struct TestbedPlugin;

impl Plugin for TestbedPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, bob_motes);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shot {
    /// Screenshot / run framing.
    Hero,
    /// Wide, slow orbit behind the main menu.
    Menu,
}

/// Small glowing plankton that drift up and down.
#[derive(Component)]
struct Mote {
    base_y: f32,
    phase: f32,
}

#[derive(SystemParam)]
pub struct TestbedSpawner<'w, 's> {
    commands: Commands<'w, 's>,
    meshes: ResMut<'w, Assets<Mesh>>,
    std_materials: ResMut<'w, Assets<StandardMaterial>>,
    glow_materials: ResMut<'w, Assets<GlowMaterial>>,
    effects: ResMut<'w, Assets<EffectAsset>>,
    env: Res<'w, EnvironmentCubemap>,
    settings: Res<'w, ActiveRenderSettings>,
    availability: Res<'w, AssetAvailability>,
    asset_server: Res<'w, AssetServer>,
}

/// Seafloor height: rolling fbm dunes plus the impact crater at the origin.
pub fn floor_height(fbm: &Fbm<Perlin>, x: f32, z: f32) -> f32 {
    let dunes = fbm.get([x as f64 * 0.035, z as f64 * 0.035]) as f32 * 2.2;
    let r = (x * x + z * z).sqrt();
    let bowl = if r < CRATER_RADIUS {
        let t = r / CRATER_RADIUS;
        -1.6 * (1.0 - t * t)
    } else {
        0.0
    };
    let rim = 0.9 * (-((r - CRATER_RADIUS) / 1.6).powi(2)).exp();
    // Flatten dunes inside the crater (the impact swept it clean).
    let dune_mask = (r / (CRATER_RADIUS * 1.4)).clamp(0.25, 1.0);
    dunes * dune_mask + bowl + rim
}

fn floor_noise() -> Fbm<Perlin> {
    Fbm::<Perlin>::new(SEED as u32).set_octaves(5)
}

/// Grid mesh displaced by [`floor_height`], with smooth normals and tangents.
pub fn build_floor_mesh(size: f32, res: u32) -> Mesh {
    let fbm = floor_noise();
    let n = res as usize + 1;
    let step = size / res as f32;
    let half = size * 0.5;
    let mut positions = Vec::with_capacity(n * n);
    let mut normals = Vec::with_capacity(n * n);
    let mut uvs = Vec::with_capacity(n * n);
    for j in 0..n {
        for i in 0..n {
            let x = i as f32 * step - half;
            let z = j as f32 * step - half;
            let h = floor_height(&fbm, x, z);
            let e = step * 0.5;
            let dx = floor_height(&fbm, x + e, z) - floor_height(&fbm, x - e, z);
            let dz = floor_height(&fbm, x, z + e) - floor_height(&fbm, x, z - e);
            positions.push([x, h, z]);
            normals.push(Vec3::new(-dx, 2.0 * e, -dz).normalize().to_array());
            // Texture repeats every 4 m.
            uvs.push([x / 4.0, z / 4.0]);
        }
    }
    let mut indices = Vec::with_capacity(res as usize * res as usize * 6);
    for j in 0..res {
        for i in 0..res {
            let a = j * (res + 1) + i;
            let b = a + 1;
            let c = a + res + 1;
            let d = c + 1;
            indices.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_indices(Indices::U32(indices));
    if let Err(e) = mesh.generate_tangents() {
        warn!("floor tangents: {e}");
    }
    mesh
}

/// An icosphere with noise displacement: a lumpy rock.
pub fn build_rock_mesh(seed: u32, roughness: f32) -> Mesh {
    let mut mesh = Sphere::new(1.0)
        .mesh()
        .ico(3)
        .expect("ico subdivisions in range");
    let fbm = Fbm::<Perlin>::new(seed).set_octaves(4);
    if let Some(bevy::mesh::VertexAttributeValues::Float32x3(pos)) =
        mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION)
    {
        for p in pos.iter_mut() {
            let v = Vec3::from_array(*p);
            let d = fbm.get([v.x as f64 * 1.3, v.y as f64 * 1.3, v.z as f64 * 1.3]) as f32;
            // Flatten the bottom so rocks sit on the floor.
            let flat = if v.y < -0.3 { 0.6 } else { 1.0 };
            *p = (v * (1.0 + d * roughness) * Vec3::new(1.0, flat, 1.0)).to_array();
        }
    }
    mesh.compute_smooth_normals();
    mesh
}

impl TestbedSpawner<'_, '_> {
    pub fn spawn(&mut self, camera: Entity, shot: Shot, animate: bool) {
        let mut rng = ChaCha8Rng::seed_from_u64(SEED);
        let fbm = floor_noise();
        let s = self.settings.0.clone();

        // --- Camera: fog, environment light, framing -----------------------
        let (radius, height, speed, center) = match shot {
            Shot::Hero => (11.0, 3.2, 0.12, Vec3::new(0.0, -0.4, 0.0)),
            Shot::Menu => (16.0, 5.0, 0.05, Vec3::new(0.0, -0.8, 0.0)),
        };
        self.commands.entity(camera).insert((
            DistanceFog {
                color: WATER_COLOR,
                directional_light_color: Color::srgba(0.25, 0.55, 0.6, 0.5),
                directional_light_exponent: 12.0,
                falloff: FogFalloff::ExponentialSquared { density: 0.055 },
            },
            GeneratedEnvironmentMapLight {
                environment_map: self.env.image.clone(),
                intensity: 300.0,
                ..default()
            },
            // The same gradient as the visible background: brighter toward
            // the surface, black in the abyss.
            Skybox {
                image: Some(self.env.image.clone()),
                brightness: 1200.0,
                ..default()
            },
            OrbitShot {
                center,
                radius,
                height,
                angle: 0.6,
                speed: if animate { speed } else { 0.0 },
                bob: if animate { 0.6 } else { 0.0 },
            },
        ));
        // Never re-insert `Camera` here: that resets its computed viewport and
        // GPU clustering then sees a zero-sized view for a frame.
        self.commands.insert_resource(ClearColor(WATER_COLOR));

        // --- Light: sunlight filtering down through the water --------------
        let cascades = CascadeShadowConfigBuilder {
            num_cascades: s.shadow_cascades as usize,
            maximum_distance: s.shadow_max_distance,
            first_cascade_far_bound: 12.0,
            ..default()
        }
        .build();
        let mut sun = self.commands.spawn((
            SceneMember,
            Name::new("Sun shafts"),
            DirectionalLight {
                color: Color::srgb(0.65, 0.9, 1.0),
                illuminance: 2_500.0,
                shadow_maps_enabled: true,
                ..default()
            },
            cascades,
            Transform::from_xyz(4.0, 20.0, -6.0).looking_at(Vec3::ZERO, Vec3::Y),
        ));
        if s.volumetric_fog {
            sun.insert(VolumetricLight);
        }
        if s.volumetric_fog {
            self.commands.spawn((
                SceneMember,
                Name::new("Water volume"),
                FogVolume {
                    fog_color: Color::srgb(0.35, 0.7, 0.8),
                    density_factor: 0.04,
                    absorption: 0.05,
                    scattering: 0.3,
                    ..default()
                },
                Transform::from_xyz(0.0, 8.0, 0.0).with_scale(Vec3::new(90.0, 24.0, 90.0)),
            ));
        }

        // --- Seafloor -------------------------------------------------------
        let floor_material = self.pbr_material(
            "texture.seafloor_sand",
            StandardMaterial {
                base_color: Color::srgb(0.2, 0.18, 0.15),
                perceptual_roughness: 0.95,
                reflectance: 0.3,
                ..default()
            },
        );
        let floor = self.meshes.add(build_floor_mesh(FLOOR_SIZE, FLOOR_RES));
        self.commands.spawn((
            SceneMember,
            Name::new("Seafloor"),
            Mesh3d(floor),
            MeshMaterial3d(floor_material),
        ));

        // --- Rocks (6 unique meshes, instanced) ----------------------------
        let rock_material = self.pbr_material(
            "texture.basalt",
            StandardMaterial {
                base_color: Color::srgb(0.09, 0.085, 0.08),
                perceptual_roughness: 0.92,
                ..default()
            },
        );
        let rock_meshes: Vec<Handle<Mesh>> = (0..6)
            .map(|i| self.meshes.add(build_rock_mesh(SEED as u32 + i, 0.35)))
            .collect();
        for i in 0..36 {
            let angle = rng.random_range(0.0..std::f32::consts::TAU);
            let mut dist = rng.random_range(CRATER_RADIUS * 1.1..45.0);
            // Keep the camera orbit ring (radius ~10-18 m) clear.
            if (9.0..21.0).contains(&dist) {
                dist += 12.0;
            }
            let (x, z) = (angle.cos() * dist, angle.sin() * dist);
            let scale = rng.random_range(0.3..1.8) * if dist < 12.0 { 0.7 } else { 1.0 };
            let y = floor_height(&fbm, x, z) + scale * 0.2;
            self.commands.spawn((
                SceneMember,
                Name::new(format!("Rock {i}")),
                Mesh3d(rock_meshes[i % rock_meshes.len()].clone()),
                MeshMaterial3d(rock_material.clone()),
                Transform::from_xyz(x, y, z)
                    .with_rotation(Quat::from_rotation_y(
                        rng.random_range(0.0..std::f32::consts::TAU),
                    ))
                    .with_scale(Vec3::new(
                        scale * rng.random_range(0.8..1.4),
                        scale,
                        scale * rng.random_range(0.8..1.4),
                    )),
            ));
        }

        // --- Meteor fragment: dark crust with hot glowing seams ------------
        let meteor_mesh = self.meshes.add(build_rock_mesh(SEED as u32 + 99, 0.5));
        let meteor_material = self.glow_materials.add(ExtendedMaterial {
            base: StandardMaterial {
                base_color: Color::srgb(0.05, 0.04, 0.04),
                perceptual_roughness: 0.7,
                metallic: 0.3,
                ..default()
            },
            extension: Bioluminescence::new(LinearRgba::rgb(1.0, 0.3, 0.06), 4.0)
                .pulse(0.15, 0.5)
                .body(0.0)
                .rim(4.0, 0.08)
                .veins(1.6, 0.75),
        });
        let meteor_y = floor_height(&fbm, 0.0, 0.0) + 0.3;
        self.commands.spawn((
            SceneMember,
            Name::new("Meteor"),
            Mesh3d(meteor_mesh),
            MeshMaterial3d(meteor_material),
            Transform::from_xyz(0.0, meteor_y, 0.0).with_scale(Vec3::splat(1.3)),
        ));
        self.commands.spawn((
            SceneMember,
            Name::new("Meteor heat"),
            PointLight {
                color: Color::srgb(1.0, 0.45, 0.15),
                intensity: 30_000.0,
                range: 12.0,
                shadow_maps_enabled: false,
                ..default()
            },
            Transform::from_xyz(0.0, meteor_y + 1.6, 0.0),
        ));

        // --- The organism ---------------------------------------------------
        let organism_mesh = self.meshes.add(Sphere::new(0.38).mesh().ico(4).unwrap());
        let organism_material = self.glow_materials.add(ExtendedMaterial {
            base: StandardMaterial {
                base_color: Color::srgba(0.1, 0.6, 0.5, 1.0),
                perceptual_roughness: 0.25,
                ..default()
            },
            extension: Bioluminescence::new(LinearRgba::rgb(0.2, 1.0, 0.75), 1.2)
                .pulse(0.6, 0.5)
                .body(0.25)
                .rim(2.5, 1.0)
                .veins(9.0, 0.5),
        });
        let organism_pos = Vec3::new(1.6, floor_height(&fbm, 1.6, 1.2) + 0.9, 1.2);
        self.commands.spawn((
            SceneMember,
            Name::new("Organism"),
            Mesh3d(organism_mesh),
            MeshMaterial3d(organism_material),
            NotShadowCaster,
            Transform::from_translation(organism_pos),
        ));
        self.commands.spawn((
            SceneMember,
            Name::new("Organism glow"),
            PointLight {
                color: Color::srgb(0.3, 1.0, 0.8),
                intensity: 25_000.0,
                range: 8.0,
                shadow_maps_enabled: false,
                ..default()
            },
            Transform::from_translation(organism_pos + Vec3::Y * 0.2),
        ));

        // --- Bioluminescent motes (instanced) -------------------------------
        let mote_mesh = self.meshes.add(Sphere::new(0.05).mesh().ico(1).unwrap());
        let mote_material = self.glow_materials.add(ExtendedMaterial {
            base: StandardMaterial {
                base_color: Color::srgb(0.2, 0.6, 1.0),
                unlit: false,
                ..default()
            },
            extension: Bioluminescence::new(LinearRgba::rgb(0.25, 0.7, 1.0), 2.5)
                .pulse(0.8, 0.6)
                .body(0.8),
        });
        let mote_count = (120.0 * s.particle_density.max(0.25)) as usize;
        for _ in 0..mote_count {
            let angle = rng.random_range(0.0..std::f32::consts::TAU);
            let dist = rng.random_range(2.0..30.0);
            let (x, z) = (angle.cos() * dist, angle.sin() * dist);
            let base_y = floor_height(&fbm, x, z) + rng.random_range(0.3..6.0);
            self.commands.spawn((
                SceneMember,
                Mote {
                    base_y,
                    phase: rng.random_range(0.0..std::f32::consts::TAU),
                },
                Mesh3d(mote_mesh.clone()),
                MeshMaterial3d(mote_material.clone()),
                NotShadowCaster,
                Transform::from_xyz(x, base_y, z),
            ));
        }

        // --- Marine snow (GPU particles) ------------------------------------
        if s.particle_density > 0.0 {
            let effect = self.effects.add(marine_snow_effect(s.particle_density));
            self.commands.spawn((
                SceneMember,
                Name::new("Marine snow"),
                ParticleEffect::new(effect),
                Transform::from_xyz(0.0, 4.0, 0.0),
            ));
        }
    }

    /// A PBR material whose `<prefix>.color/.normal/.roughness` textures
    /// replace the flat fallback values when they were fetched.
    fn pbr_material(&mut self, prefix: &str, mut m: StandardMaterial) -> Handle<StandardMaterial> {
        let tex = |map: &str| self.availability.path_of(&format!("{prefix}.{map}"));
        if let Some(p) = tex("color") {
            m.base_color = Color::WHITE;
            m.base_color_texture = Some(self.asset_server.load(p));
        }
        if let Some(p) = tex("normal") {
            m.normal_map_texture = Some(self.asset_server.load(p));
        }
        if let Some(p) = tex("roughness") {
            m.perceptual_roughness = 1.0;
            m.metallic_roughness_texture = Some(self.asset_server.load(p));
        }
        self.std_materials.add(m)
    }
}

fn marine_snow_effect(density: f32) -> EffectAsset {
    let writer = ExprWriter::new();
    let init_age = SetAttributeModifier::new(Attribute::AGE, writer.lit(0.0).expr());
    let lifetime = (writer.lit(8.0) + writer.rand(ScalarType::Float) * writer.lit(6.0)).expr();
    let init_lifetime = SetAttributeModifier::new(Attribute::LIFETIME, lifetime);
    let init_pos = SetPositionSphereModifier {
        center: writer.lit(Vec3::ZERO).expr(),
        radius: writer.lit(22.0).expr(),
        dimension: ShapeDimension::Volume,
    };
    let init_vel = SetVelocitySphereModifier {
        center: writer.lit(Vec3::ZERO).expr(),
        speed: writer.lit(0.08).expr(),
    };
    // Slow sinking.
    let accel = AccelModifier::new(writer.lit(Vec3::new(0.0, -0.03, 0.0)).expr());
    let mut color = bevy_hanabi::Gradient::new();
    color.add_key(0.0, Vec4::new(0.5, 0.7, 0.75, 0.0));
    color.add_key(0.2, Vec4::new(0.5, 0.7, 0.75, 0.35));
    color.add_key(0.8, Vec4::new(0.5, 0.7, 0.75, 0.35));
    color.add_key(1.0, Vec4::new(0.5, 0.7, 0.75, 0.0));
    let mut module = writer.finish();
    let round = RoundModifier::ellipse(&mut module);
    let rate = 220.0 * density;
    EffectAsset::new(8192, SpawnerSettings::rate(rate.into()), module)
        .with_name("marine_snow")
        .init(init_pos)
        .init(init_vel)
        .init(init_age)
        .init(init_lifetime)
        .update(accel)
        .render(ColorOverLifetimeModifier::new(color))
        .render(round)
        .render(SizeOverLifetimeModifier {
            gradient: bevy_hanabi::Gradient::constant(Vec3::splat(0.02)),
            screen_space_size: false,
        })
        .render(OrientModifier {
            mode: OrientMode::FaceCameraPosition,
            rotation: None,
        })
}

fn bob_motes(time: Res<Time>, mut q: Query<(&mut Transform, &Mote)>) {
    let t = time.elapsed_secs();
    for (mut tr, mote) in &mut q {
        tr.translation.y = mote.base_y + 0.25 * (t * 0.4 + mote.phase).sin();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crater_is_lower_than_rim() {
        let fbm = floor_noise();
        let center = floor_height(&fbm, 0.0, 0.0);
        let rim = floor_height(&fbm, CRATER_RADIUS, 0.0);
        assert!(center < rim - 1.0, "center {center} rim {rim}");
    }

    #[test]
    fn floor_mesh_has_expected_topology() {
        let mesh = build_floor_mesh(10.0, 4);
        assert_eq!(mesh.count_vertices(), 25);
        assert_eq!(mesh.indices().unwrap().len(), 4 * 4 * 6);
    }

    #[test]
    fn generation_is_deterministic() {
        let a = build_rock_mesh(7, 0.3);
        let b = build_rock_mesh(7, 0.3);
        assert_eq!(
            a.attribute(Mesh::ATTRIBUTE_POSITION).unwrap().as_float3(),
            b.attribute(Mesh::ATTRIBUTE_POSITION).unwrap().as_float3()
        );
    }
}
