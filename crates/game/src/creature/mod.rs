//! The player organism's presentation: metaball body (marching cubes on the
//! AsyncComputeTaskPool, regrown as biomass passes growth stages), wobbling
//! bioluminescent skin, glow light, interpolated transforms for every sim
//! entity, and player input -> `PlayerIntent`.

mod feel;
pub mod mc_tables;
pub mod mesher;

use crate::boot::{ActiveRenderSettings, PlayerSettings};
use crate::graphics::follow::FollowCamera;
use crate::graphics::materials::{Bioluminescence, CreatureMaterial, CreatureSkin};
use crate::input::{GameAction, PlayerInput};
use bevy::asset::RenderAssetUsages;
use bevy::light::NotShadowCaster;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::pbr::ExtendedMaterial;
use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, poll_once};
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use game_core::sim::{Player, PlayerIntent, PlayerVitals, Pos, PrevPos, Radius, RunInfo, Vel};
use game_core::states::{AppState, RunState};
use leafwing_input_manager::prelude::ActionState;
use mesher::{Blob, MeshData, polygonize};
use rand::{RngExt, SeedableRng};
use rand_chacha::ChaCha8Rng;

pub use feel::Trauma;

pub struct CreaturePlugin;

impl Plugin for CreaturePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(feel::FeelPlugin)
            .add_systems(
                Update,
                (
                    attach_player_visuals,
                    request_body_mesh,
                    apply_body_mesh,
                    sync_sim_transforms,
                    update_player_glow,
                )
                    .chain()
                    .run_if(in_state(AppState::InRun)),
            )
            .add_systems(
                Update,
                player_input
                    .run_if(in_state(RunState::Playing).and_then(not(resource_exists::<AutoPilot>)))
                    .before(sync_sim_transforms),
            )
            .add_systems(Update, cursor_grab);
    }
}

/// Present while a bot (`game_core::bots`) drives the player, e.g. for run
/// screenshots; device input is then ignored.
#[derive(Resource, Debug, Default)]
pub struct AutoPilot;

/// Render-side scale for a sim entity (multiplied by its sim radius).
#[derive(Component, Debug, Clone, Copy)]
pub struct SimVisual {
    pub shape_scale: Vec3,
}

/// The organism's regrowable body mesh.
#[derive(Component)]
struct CreatureBody {
    stage: i32,
    mesh: Handle<Mesh>,
    task: Option<Task<MeshData>>,
}

#[derive(Component)]
struct CreatureGlow;

/// Growth stage: a new body shape every half doubling of biomass.
fn growth_stage(biomass: f32) -> i32 {
    (biomass.max(1.0).log2() * 2.0).floor() as i32
}

/// Body blobs for a stage. Later stages keep earlier lobes and add more, so
/// the body visibly grows rather than reshuffling.
pub fn body_blobs(seed: u64, stage: i32) -> Vec<Blob> {
    let mut rng = ChaCha8Rng::seed_from_u64(seed ^ 0xB0D7_5EED);
    let mut blobs = vec![
        Blob {
            center: Vec3::ZERO,
            radius: 0.72,
        },
        // Mouth lobe toward -Z (forward).
        Blob {
            center: Vec3::new(0.0, -0.05, -0.42),
            radius: 0.42,
        },
    ];
    let lobes = (2 + stage / 2).clamp(2, 9);
    for i in 0..lobes {
        let a = rng.random_range(0.0..std::f32::consts::TAU);
        let y = rng.random_range(-0.6..0.6);
        let dir = Vec3::new(a.cos(), y, a.sin()).normalize();
        let grow = (stage - i * 2).clamp(0, 6) as f32 / 6.0;
        blobs.push(Blob {
            center: dir * rng.random_range(0.42..0.62),
            radius: rng.random_range(0.24..0.34) + 0.1 * grow,
        });
    }
    // Normalize the overall size to radius ~1 (the sim radius scales it).
    let extent = blobs
        .iter()
        .map(|b| b.center.length() + b.radius)
        .fold(0.0f32, f32::max);
    for b in &mut blobs {
        b.center /= extent;
        b.radius /= extent;
    }
    blobs
}

fn to_mesh(data: MeshData) -> Mesh {
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, data.positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, data.normals)
    .with_inserted_indices(Indices::U32(data.indices))
}

fn attach_player_visuals(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<CreatureMaterial>>,
    players: Query<(Entity, &Pos, &Radius), Added<Player>>,
) {
    for (e, pos, r) in &players {
        let mesh = meshes.add(Sphere::new(1.0).mesh().ico(3).expect("valid ico"));
        let glow = Bioluminescence::new(LinearRgba::rgb(0.2, 1.0, 0.75), 1.4)
            .pulse(0.6, 0.5)
            .body(0.3)
            .rim(2.5, 1.0)
            .veins(9.0, 0.5);
        let material = materials.add(ExtendedMaterial {
            base: StandardMaterial {
                base_color: Color::srgba(0.1, 0.6, 0.5, 0.82),
                alpha_mode: AlphaMode::Blend,
                perceptual_roughness: 0.25,
                ..default()
            },
            extension: CreatureSkin::from_glow(glow, 0.035, 5.0, 2.2, 0.025),
        });
        commands
            .entity(e)
            .insert((
                Name::new("Organism"),
                Mesh3d(mesh.clone()),
                MeshMaterial3d(material),
                Transform::from_translation(pos.0).with_scale(Vec3::splat(r.0)),
                Visibility::default(),
                NotShadowCaster,
                SimVisual {
                    shape_scale: Vec3::ONE,
                },
                CreatureBody {
                    stage: -1,
                    mesh,
                    task: None,
                },
            ))
            .with_children(|c| {
                c.spawn((
                    CreatureGlow,
                    PointLight {
                        color: Color::srgb(0.3, 1.0, 0.8),
                        intensity: 4_000.0,
                        range: 6.0,
                        shadow_maps_enabled: false,
                        ..default()
                    },
                    Transform::from_xyz(0.0, 0.3, 0.0),
                ));
            });
    }
}

fn mesh_resolution(settings: &ActiveRenderSettings) -> usize {
    // Grid cells per axis: denser bodies on higher presets.
    (20.0 * settings.lod_distance_scale.clamp(0.75, 2.0)).round() as usize + 4
}

fn request_body_mesh(
    info: Option<Res<RunInfo>>,
    settings: Res<ActiveRenderSettings>,
    mut q: Query<(&mut CreatureBody, &PlayerVitals)>,
) {
    let seed = info.map(|i| i.seed).unwrap_or(0);
    let res = mesh_resolution(&settings);
    for (mut body, vitals) in &mut q {
        let stage = growth_stage(vitals.0.biomass);
        if stage != body.stage && body.task.is_none() {
            body.stage = stage;
            let blobs = body_blobs(seed, stage);
            body.task =
                Some(AsyncComputeTaskPool::get().spawn(async move { polygonize(&blobs, res) }));
        }
    }
}

fn apply_body_mesh(mut meshes: ResMut<Assets<Mesh>>, mut q: Query<&mut CreatureBody>) {
    for mut body in &mut q {
        let Some(task) = body.task.as_mut() else {
            continue;
        };
        if let Some(data) = block_on(poll_once(task)) {
            body.task = None;
            if !data.indices.is_empty() {
                let _ = meshes.insert(&body.mesh, to_mesh(data));
            }
        }
    }
}

/// Interpolates every sim entity's Transform between fixed steps and faces
/// it along its velocity.
fn sync_sim_transforms(
    fixed: Res<Time<Fixed>>,
    time: Res<Time>,
    mut q: Query<(&Pos, &PrevPos, &Vel, &Radius, &SimVisual, &mut Transform)>,
) {
    let a = fixed.overstep_fraction();
    let turn = 1.0 - (-6.0 * time.delta_secs()).exp();
    for (pos, prev, vel, r, visual, mut t) in &mut q {
        t.translation = prev.0.lerp(pos.0, a);
        t.scale = visual.shape_scale * r.0;
        let flat = Vec3::new(vel.0.x, vel.0.y * 0.5, vel.0.z);
        if flat.length_squared() > 1e-4 {
            let target = Transform::IDENTITY.looking_to(flat, Vec3::Y).rotation;
            t.rotation = t.rotation.slerp(target, turn);
        }
    }
}

fn update_player_glow(
    players: Query<(&PlayerVitals, &Radius, &Children), With<Player>>,
    mut lights: Query<&mut PointLight, With<CreatureGlow>>,
) {
    for (vitals, r, children) in &players {
        for child in children.iter() {
            if let Ok(mut light) = lights.get_mut(child) {
                // Dims as energy drops: a starving organism visibly fades.
                let energy = (vitals.0.energy / 100.0).clamp(0.15, 1.0);
                light.range = 6.0 * r.0.max(0.25) * 4.0;
                light.intensity = 30_000.0 * r.0.powf(1.6).max(0.1) * energy;
            }
        }
    }
}

/// Keyboard/gamepad -> `PlayerIntent`, relative to the camera's heading.
fn player_input(
    actions: Single<&ActionState<GameAction>, With<PlayerInput>>,
    camera: Query<&FollowCamera>,
    mut intent: ResMut<PlayerIntent>,
) {
    let axis = actions.clamped_axis_pair(&GameAction::Move);
    let yaw = camera.single().map(|c| c.yaw).unwrap_or(0.0);
    let forward = Vec3::new(-yaw.sin(), 0.0, -yaw.cos());
    let right = Vec3::new(yaw.cos(), 0.0, -yaw.sin());
    let mut dir = right * axis.x + forward * axis.y;
    if actions.pressed(&GameAction::Ascend) {
        dir.y += 1.0;
    }
    if actions.pressed(&GameAction::Descend) {
        dir.y -= 1.0;
    }
    intent.move_dir = dir.clamp_length_max(1.0);
    if actions.just_pressed(&GameAction::Dash) {
        intent.dash = true;
    }
    if actions.just_pressed(&GameAction::Consume) {
        intent.consume_presses += 1;
    }
}

/// Locks and hides the cursor while playing; frees it everywhere else.
fn cursor_grab(
    state: Option<Res<State<RunState>>>,
    settings: Res<PlayerSettings>,
    mut windows: Query<&mut CursorOptions, With<PrimaryWindow>>,
) {
    let _ = &settings;
    let playing = state.is_some_and(|s| *s.get() == RunState::Playing);
    for mut cursor in &mut windows {
        let (mode, visible) = if playing {
            (CursorGrabMode::Locked, false)
        } else {
            (CursorGrabMode::None, true)
        };
        if cursor.grab_mode != mode {
            cursor.grab_mode = mode;
        }
        if cursor.visible != visible {
            cursor.visible = visible;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_grows_lobes_with_stage_and_fits_unit_radius() {
        let small = body_blobs(5, 0);
        let big = body_blobs(5, 12);
        assert!(big.len() > small.len());
        for b in &big {
            assert!(b.center.length() + b.radius <= 1.0001);
        }
        assert_eq!(body_blobs(5, 4), body_blobs(5, 4));
    }

    #[test]
    fn stages_advance_every_half_doubling() {
        assert_eq!(growth_stage(1.0), 0);
        assert_eq!(growth_stage(1.5), 1);
        assert_eq!(growth_stage(2.0), 2);
        assert!(growth_stage(150.0) > growth_stage(100.0));
    }
}
