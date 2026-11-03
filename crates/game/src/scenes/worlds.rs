//! World builders for each biome, shared by the run (strata) and the
//! testbed/menu scenes. Terrain comes from `game_core::terrain`, so what is
//! drawn is exactly what the simulation collides with.
//!
//! - Crater Seafloor (Tier 1, Impact Site): dim teal water, dunes around an
//!   impact crater, basalt rocks, the meteor with molten seams, marine snow.
//! - Coral Reef (Tier 2, Shallows): bright turquoise water, sand flats with
//!   channels, coral clusters (branching, brain, sea fans), stronger sunlight.
//!
//! Everything is procedural; fetched textures replace flat materials when
//! present.

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
use game_core::balance::Biome;
use game_core::terrain::Terrain;
use noise::{Fbm, MultiFractal, NoiseFn, Perlin};
use rand::{RngExt, SeedableRng};
use rand_chacha::ChaCha8Rng;

/// Fog colors match their palette's horizon so distant geometry dissolves
/// into the background.
const SEAFLOOR_WATER: Color = Color::linear_rgb(0.01, 0.06, 0.10);
const REEF_WATER: Color = Color::linear_rgb(0.03, 0.18, 0.28);

pub struct WorldsPlugin;

impl Plugin for WorldsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, bob_motes);
    }
}

/// How the camera is framed for a static (non-run) scene.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shot {
    /// Screenshot framing near the meteor.
    Hero,
    /// Wide, slow orbit behind the main menu.
    Menu,
}

#[derive(Debug, Clone)]
pub struct WorldSpec {
    pub biome: Biome,
    pub terrain: Terrain,
    /// Playable radius (the floor extends beyond it).
    pub radius: f32,
    pub camera: Entity,
    /// Static camera framing; `None` in runs (the follow camera drives it).
    pub shot: Option<(Shot, bool)>,
    /// Place the stand-in organism next to the meteor (testbed only; in runs
    /// the player creature is there).
    pub stand_in_organism: bool,
}

/// Small glowing plankton that drift up and down (decoration only).
#[derive(Component)]
struct Mote {
    base_y: f32,
    phase: f32,
}

#[derive(SystemParam)]
pub struct WorldSpawner<'w, 's> {
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

/// Grid mesh of the terrain height field, centered on the origin.
pub fn build_floor_mesh(terrain: &Terrain, size: f32, res: u32) -> Mesh {
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
            positions.push([x, terrain.height(x, z), z]);
            normals.push(terrain.normal(x, z));
            uvs.push([x / 4.0, z / 4.0]); // texture repeats every 4 m
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
            let flat = if v.y < -0.3 { 0.6 } else { 1.0 };
            *p = (v * (1.0 + d * roughness) * Vec3::new(1.0, flat, 1.0)).to_array();
        }
    }
    mesh.compute_smooth_normals();
    mesh
}

impl WorldSpawner<'_, '_> {
    pub fn build(&mut self, spec: &WorldSpec) {
        match spec.biome {
            Biome::CraterSeafloor => self.build_crater(spec),
            Biome::CoralReef => self.build_reef(spec),
        }
    }

    fn camera_setup(
        &mut self,
        spec: &WorldSpec,
        water: Color,
        density: f32,
        env: Handle<Image>,
        env_intensity: f32,
        sky: f32,
    ) {
        let mut cam = self.commands.entity(spec.camera);
        cam.insert((
            DistanceFog {
                color: water,
                directional_light_color: Color::srgba(0.25, 0.55, 0.6, 0.5),
                directional_light_exponent: 12.0,
                falloff: FogFalloff::ExponentialSquared { density },
            },
            GeneratedEnvironmentMapLight {
                environment_map: env.clone(),
                intensity: env_intensity,
                ..default()
            },
            Skybox {
                image: Some(env),
                brightness: sky,
                ..default()
            },
        ));
        if let Some((shot, animate)) = spec.shot {
            let (radius, height, speed, center) = match shot {
                Shot::Hero => (11.0, 3.2, 0.12, Vec3::new(0.0, -0.4, 0.0)),
                Shot::Menu => (16.0, 5.0, 0.05, Vec3::new(0.0, -0.8, 0.0)),
            };
            cam.insert(OrbitShot {
                center,
                radius,
                height,
                angle: 0.6,
                speed: if animate { speed } else { 0.0 },
                bob: if animate { 0.6 } else { 0.0 },
            });
        }
        // Never re-insert `Camera` here: that resets its computed viewport and
        // GPU clustering then sees a zero-sized view for a frame.
        self.commands.insert_resource(ClearColor(water));
    }

    fn sun(&mut self, illuminance: f32, color: Color, dir_from: Vec3) {
        let s = self.settings.0.clone();
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
                color,
                illuminance,
                shadow_maps_enabled: true,
                ..default()
            },
            cascades,
            Transform::from_translation(dir_from).looking_at(Vec3::ZERO, Vec3::Y),
        ));
        if s.volumetric_fog {
            sun.insert(VolumetricLight);
        }
    }

    fn water_volume(&mut self, color: Color, center_y: f32, size: Vec3) {
        if !self.settings.volumetric_fog {
            return;
        }
        self.commands.spawn((
            SceneMember,
            Name::new("Water volume"),
            FogVolume {
                fog_color: color,
                density_factor: 0.04,
                absorption: 0.05,
                scattering: 0.3,
                ..default()
            },
            Transform::from_xyz(0.0, center_y, 0.0).with_scale(size),
        ));
    }

    fn floor(&mut self, spec: &WorldSpec, prefix: &str, base: StandardMaterial) {
        let size = (spec.radius * 2.0 + 40.0).max(80.0);
        let res = ((size * 1.2) as u32).clamp(96, 200);
        let material = self.pbr_material(prefix, base);
        let mesh = self.meshes.add(build_floor_mesh(&spec.terrain, size, res));
        self.commands.spawn((
            SceneMember,
            Name::new("Seafloor"),
            Mesh3d(mesh),
            MeshMaterial3d(material),
        ));
    }

    fn marine_snow(&mut self, radius: f32, center_y: f32, density: f32) {
        let d = self.settings.particle_density * density;
        if d <= 0.0 {
            return;
        }
        let effect = self.effects.add(marine_snow_effect(d, radius));
        self.commands.spawn((
            SceneMember,
            Name::new("Marine snow"),
            ParticleEffect::new(effect),
            Transform::from_xyz(0.0, center_y, 0.0),
        ));
    }

    fn build_crater(&mut self, spec: &WorldSpec) {
        let env = self.env.image.clone();
        self.camera_setup(spec, SEAFLOOR_WATER, 0.055, env, 300.0, 1200.0);
        self.sun(
            2_500.0,
            Color::srgb(0.65, 0.9, 1.0),
            Vec3::new(4.0, 20.0, -6.0),
        );
        self.water_volume(
            Color::srgb(0.35, 0.7, 0.8),
            8.0,
            Vec3::new(90.0, 24.0, 90.0),
        );
        self.floor(
            spec,
            "texture.seafloor_sand",
            StandardMaterial {
                base_color: Color::srgb(0.2, 0.18, 0.15),
                perceptual_roughness: 0.95,
                reflectance: 0.3,
                ..default()
            },
        );
        let terrain = spec.terrain.clone();
        let mut rng = ChaCha8Rng::seed_from_u64(terrain.seed as u64 ^ 0xC0FFEE);

        // Basalt rocks (6 unique meshes, instanced), clear of the menu orbit ring.
        let rock_material = self.pbr_material(
            "texture.basalt",
            StandardMaterial {
                base_color: Color::srgb(0.09, 0.085, 0.08),
                perceptual_roughness: 0.92,
                ..default()
            },
        );
        let rock_meshes: Vec<Handle<Mesh>> = (0..6)
            .map(|i| {
                self.meshes
                    .add(build_rock_mesh(terrain.seed.wrapping_add(i), 0.35))
            })
            .collect();
        let crater = game_core::terrain::CRATER_RADIUS;
        let outer = (spec.radius * 1.4).max(45.0);
        for i in 0..40 {
            let angle = rng.random_range(0.0..std::f32::consts::TAU);
            let mut dist = rng.random_range(crater * 1.1..outer);
            if (9.0..21.0).contains(&dist) {
                dist += 12.0;
            }
            let (x, z) = (angle.cos() * dist, angle.sin() * dist);
            let scale = rng.random_range(0.3..1.8) * if dist < 12.0 { 0.7 } else { 1.0 };
            let y = terrain.height(x, z) + scale * 0.2;
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

        // The meteor fragment: dark crust with hot glowing seams.
        let meteor_mesh = self
            .meshes
            .add(build_rock_mesh(terrain.seed.wrapping_add(99), 0.5));
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
        let meteor_y = terrain.height(0.0, 0.0) + 0.3;
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

        if spec.stand_in_organism {
            let organism_mesh = self
                .meshes
                .add(Sphere::new(0.38).mesh().ico(4).expect("ico"));
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
            let pos = Vec3::new(1.6, terrain.height(1.6, 1.2) + 0.9, 1.2);
            self.commands.spawn((
                SceneMember,
                Name::new("Organism stand-in"),
                Mesh3d(organism_mesh),
                MeshMaterial3d(organism_material),
                NotShadowCaster,
                Transform::from_translation(pos),
            ));
            self.commands.spawn((
                SceneMember,
                PointLight {
                    color: Color::srgb(0.3, 1.0, 0.8),
                    intensity: 25_000.0,
                    range: 8.0,
                    shadow_maps_enabled: false,
                    ..default()
                },
                Transform::from_translation(pos + Vec3::Y * 0.2),
            ));
        }

        self.motes(&terrain, &mut rng, 30.0, LinearRgba::rgb(0.25, 0.7, 1.0));
        self.marine_snow(22.0, 4.0, 1.0);
    }

    fn build_reef(&mut self, spec: &WorldSpec) {
        let env = self.env.reef.clone();
        self.camera_setup(spec, REEF_WATER, 0.012, env, 900.0, 2500.0);
        self.sun(
            7_000.0,
            Color::srgb(0.95, 1.0, 0.92),
            Vec3::new(-8.0, 40.0, 10.0),
        );
        self.water_volume(
            Color::srgb(0.5, 0.85, 0.95),
            18.0,
            Vec3::new(300.0, 50.0, 300.0),
        );
        self.floor(
            spec,
            "texture.seafloor_sand",
            StandardMaterial {
                base_color: Color::srgb(0.62, 0.56, 0.42),
                perceptual_roughness: 0.92,
                reflectance: 0.35,
                ..default()
            },
        );
        let terrain = spec.terrain.clone();
        let mut rng = ChaCha8Rng::seed_from_u64(terrain.seed as u64 ^ 0x2EEF);
        let palette = [
            Color::srgb(0.95, 0.45, 0.55),
            Color::srgb(1.0, 0.6, 0.25),
            Color::srgb(0.6, 0.35, 0.85),
            Color::srgb(0.95, 0.85, 0.35),
            Color::srgb(0.3, 0.75, 0.65),
        ];
        let coral_mats: Vec<Handle<StandardMaterial>> = palette
            .iter()
            .map(|c| {
                self.std_materials.add(StandardMaterial {
                    base_color: *c,
                    emissive: LinearRgba::from(*c) * 0.08,
                    perceptual_roughness: 0.8,
                    ..default()
                })
            })
            .collect();
        let branch = self.meshes.add(Capsule3d::new(0.12, 1.0));
        let blob = self
            .meshes
            .add(Sphere::new(1.0).mesh().ico(2).expect("ico"));
        let fan = self.meshes.add(Cylinder::new(1.0, 0.05));
        let rock_mesh = self.meshes.add(build_rock_mesh(terrain.seed ^ 7, 0.45));
        let rock_mat = self.std_materials.add(StandardMaterial {
            base_color: Color::srgb(0.35, 0.32, 0.28),
            perceptual_roughness: 0.95,
            ..default()
        });
        let clusters = 90;
        let extent = spec.radius * 1.3;
        for i in 0..clusters {
            let a = rng.random_range(0.0..std::f32::consts::TAU);
            let d = extent * rng.random::<f32>().sqrt();
            let (cx, cz) = (a.cos() * d, a.sin() * d);
            if (cx * cx + cz * cz).sqrt() < 8.0 {
                continue; // keep the arrival point open
            }
            let base_y = terrain.height(cx, cz);
            let size = rng.random_range(1.5..5.0);
            // Bommie: a rock base for each cluster.
            self.commands.spawn((
                SceneMember,
                Name::new(format!("Bommie {i}")),
                Mesh3d(rock_mesh.clone()),
                MeshMaterial3d(rock_mat.clone()),
                Transform::from_xyz(cx, base_y + size * 0.15, cz).with_scale(Vec3::new(
                    size,
                    size * 0.55,
                    size * rng.random_range(0.8..1.2),
                )),
            ));
            let top = base_y + size * 0.6;
            let mat = coral_mats[rng.random_range(0..coral_mats.len())].clone();
            match rng.random_range(0..3) {
                0 => {
                    // Branching coral: a fan of capsules.
                    for _ in 0..rng.random_range(5..10) {
                        let tilt = Quat::from_euler(
                            EulerRot::YXZ,
                            rng.random_range(0.0..std::f32::consts::TAU),
                            rng.random_range(-0.6..0.6),
                            0.0,
                        );
                        let len = size * rng.random_range(0.35..0.7);
                        let off = Vec3::new(
                            rng.random_range(-0.4..0.4),
                            0.0,
                            rng.random_range(-0.4..0.4),
                        ) * size;
                        self.commands.spawn((
                            SceneMember,
                            Mesh3d(branch.clone()),
                            MeshMaterial3d(mat.clone()),
                            Transform::from_translation(
                                Vec3::new(cx, top, cz) + off + tilt * Vec3::Y * len * 0.5,
                            )
                            .with_rotation(tilt)
                            .with_scale(Vec3::new(
                                size * 0.6,
                                len,
                                size * 0.6,
                            )),
                            NotShadowCaster,
                        ));
                    }
                }
                1 => {
                    // Brain coral: squashed dome.
                    self.commands.spawn((
                        SceneMember,
                        Mesh3d(blob.clone()),
                        MeshMaterial3d(mat.clone()),
                        Transform::from_xyz(cx, top, cz).with_scale(Vec3::new(
                            size * 0.5,
                            size * 0.32,
                            size * 0.5,
                        )),
                    ));
                }
                _ => {
                    // Sea fans: thin upright discs.
                    for _ in 0..rng.random_range(2..4) {
                        let yaw = rng.random_range(0.0..std::f32::consts::TAU);
                        let h = size * rng.random_range(0.4..0.8);
                        self.commands.spawn((
                            SceneMember,
                            Mesh3d(fan.clone()),
                            MeshMaterial3d(mat.clone()),
                            Transform::from_xyz(cx, top + h * 0.6, cz)
                                .with_rotation(
                                    Quat::from_rotation_y(yaw)
                                        * Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
                                )
                                .with_scale(Vec3::new(h * 0.7, 1.0, h)),
                            NotShadowCaster,
                        ));
                    }
                }
            }
        }
        self.motes(&terrain, &mut rng, 60.0, LinearRgba::rgb(0.6, 0.9, 1.0));
        self.marine_snow(60.0, 12.0, 0.6);
    }

    fn motes(&mut self, terrain: &Terrain, rng: &mut ChaCha8Rng, spread: f32, color: LinearRgba) {
        let mote_mesh = self
            .meshes
            .add(Sphere::new(0.05).mesh().ico(1).expect("ico"));
        let mote_material = self.glow_materials.add(ExtendedMaterial {
            base: StandardMaterial {
                base_color: Color::srgb(0.2, 0.6, 1.0),
                ..default()
            },
            extension: Bioluminescence::new(color, 2.5).pulse(0.8, 0.6).body(0.8),
        });
        let count = (120.0 * self.settings.particle_density.max(0.25)) as usize;
        for _ in 0..count {
            let angle = rng.random_range(0.0..std::f32::consts::TAU);
            let dist = rng.random_range(2.0..spread);
            let (x, z) = (angle.cos() * dist, angle.sin() * dist);
            let base_y = terrain.height(x, z) + rng.random_range(0.3..6.0);
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

fn marine_snow_effect(density: f32, radius: f32) -> EffectAsset {
    let writer = ExprWriter::new();
    let init_age = SetAttributeModifier::new(Attribute::AGE, writer.lit(0.0).expr());
    let lifetime = (writer.lit(8.0) + writer.rand(ScalarType::Float) * writer.lit(6.0)).expr();
    let init_lifetime = SetAttributeModifier::new(Attribute::LIFETIME, lifetime);
    let init_pos = SetPositionSphereModifier {
        center: writer.lit(Vec3::ZERO).expr(),
        radius: writer.lit(radius).expr(),
        dimension: ShapeDimension::Volume,
    };
    let init_vel = SetVelocitySphereModifier {
        center: writer.lit(Vec3::ZERO).expr(),
        speed: writer.lit(0.08).expr(),
    };
    let accel = AccelModifier::new(writer.lit(Vec3::new(0.0, -0.03, 0.0)).expr());
    let mut color = bevy_hanabi::Gradient::new();
    color.add_key(0.0, Vec4::new(0.5, 0.7, 0.75, 0.0));
    color.add_key(0.2, Vec4::new(0.5, 0.7, 0.75, 0.35));
    color.add_key(0.8, Vec4::new(0.5, 0.7, 0.75, 0.35));
    color.add_key(1.0, Vec4::new(0.5, 0.7, 0.75, 0.0));
    let mut module = writer.finish();
    let round = RoundModifier::ellipse(&mut module);
    // Keep particle density similar regardless of the volume's size.
    let rate = 220.0 * density * (radius / 22.0).powi(2).max(1.0);
    EffectAsset::new(16384, SpawnerSettings::rate(rate.into()), module)
        .with_name("marine_snow")
        .init(init_pos)
        .init(init_vel)
        .init(init_age)
        .init(init_lifetime)
        .update(accel)
        .render(ColorOverLifetimeModifier::new(color))
        .render(round)
        .render(SizeOverLifetimeModifier {
            gradient: bevy_hanabi::Gradient::constant(Vec3::splat(0.02 * (radius / 22.0).max(1.0))),
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
    fn floor_mesh_has_expected_topology() {
        let t = Terrain::new(Biome::CraterSeafloor, 1);
        let mesh = build_floor_mesh(&t, 10.0, 4);
        assert_eq!(mesh.count_vertices(), 25);
        assert_eq!(mesh.indices().unwrap().len(), 4 * 4 * 6);
    }

    #[test]
    fn rock_generation_is_deterministic() {
        let a = build_rock_mesh(7, 0.3);
        let b = build_rock_mesh(7, 0.3);
        assert_eq!(
            a.attribute(Mesh::ATTRIBUTE_POSITION).unwrap().as_float3(),
            b.attribute(Mesh::ATTRIBUTE_POSITION).unwrap().as_float3()
        );
    }
}
