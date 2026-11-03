//! Creature populations, presentation side: grey-box bodies per species
//! shape, size-class tinting (edible / risky / dangerous) with colorblind-safe
//! shape markers, and the swallow animation when the player eats.
//!
//! The population model itself (Lotka-Volterra food web) arrives in M2; the
//! fixed-step sim in `game_core::sim` owns positions, AI and respawns.

use crate::boot::PlayerSettings;
use crate::creature::SimVisual;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use game_core::balance::{Balance, Shape};
use game_core::config::ColorblindMode;
use game_core::rules::{SizeClass, classify};
use game_core::sim::{Agent, Consumed, Player, Pos, Radius, RunInfo, TierEntered};
use game_core::states::AppState;

pub struct EcosystemPlugin;

impl Plugin for EcosystemPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_shared).add_systems(
            Update,
            (
                rebuild_species_materials,
                attach_agent_visuals,
                update_size_classes,
                spawn_swallow_fx,
                animate_swallow_fx,
            )
                .chain()
                .run_if(in_state(AppState::InRun)),
        );
    }
}

/// Meshes shared by every creature.
#[derive(Resource)]
struct SharedMeshes {
    smooth: Handle<Mesh>,
    faceted: Handle<Mesh>,
    marker_danger: Handle<Mesh>,
    marker_risky: Handle<Mesh>,
    marker_danger_mat: Handle<StandardMaterial>,
    marker_risky_mat: Handle<StandardMaterial>,
}

/// Materials per (species, size class) for the current tier.
#[derive(Resource, Default)]
struct SpeciesMaterials {
    tier_index: Option<usize>,
    by_class: Vec<[Handle<StandardMaterial>; 3]>,
}

#[derive(Component)]
struct ShownClass(Option<SizeClass>);

#[derive(Component)]
struct ClassMarker;

#[derive(Component)]
struct SwallowFx {
    t: f32,
    start: Vec3,
    scale: Vec3,
}

const SWALLOW_SECS: f32 = 0.22;

fn class_index(c: SizeClass) -> usize {
    match c {
        SizeClass::Edible => 0,
        SizeClass::Risky => 1,
        SizeClass::Dangerous => 2,
    }
}

/// Emissive tint per class. Colorblind modes swap to palettes that stay
/// distinguishable; the shape markers carry the meaning regardless.
fn class_tint(class: SizeClass, mode: ColorblindMode) -> LinearRgba {
    let (edible, risky, danger) = match mode {
        ColorblindMode::Off => (
            LinearRgba::rgb(0.1, 0.9, 0.45),
            LinearRgba::rgb(1.0, 0.65, 0.05),
            LinearRgba::rgb(1.0, 0.08, 0.05),
        ),
        ColorblindMode::Protanopia | ColorblindMode::Deuteranopia => (
            LinearRgba::rgb(0.1, 0.45, 1.0),
            LinearRgba::rgb(1.0, 0.85, 0.1),
            LinearRgba::rgb(0.9, 0.1, 0.9),
        ),
        ColorblindMode::Tritanopia => (
            LinearRgba::rgb(0.1, 0.8, 0.8),
            LinearRgba::rgb(1.0, 0.5, 0.6),
            LinearRgba::rgb(0.9, 0.05, 0.1),
        ),
    };
    match class {
        SizeClass::Edible => edible,
        SizeClass::Risky => risky,
        SizeClass::Dangerous => danger,
    }
}

fn shape_scale(shape: Shape) -> Vec3 {
    match shape {
        Shape::Blob => Vec3::new(1.0, 0.9, 1.0),
        Shape::Elongated => Vec3::new(0.55, 0.5, 1.4),
        Shape::Jelly => Vec3::new(1.0, 0.75, 1.0),
        Shape::Spiky => Vec3::new(0.6, 0.6, 1.25),
        Shape::Fish => Vec3::new(0.55, 0.75, 1.35),
    }
}

fn setup_shared(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let marker = |c: LinearRgba| StandardMaterial {
        base_color: Color::BLACK,
        emissive: c * 6.0,
        unlit: true,
        ..default()
    };
    commands.insert_resource(SharedMeshes {
        smooth: meshes.add(Sphere::new(1.0).mesh().ico(2).expect("ico")),
        faceted: meshes.add(Sphere::new(1.0).mesh().ico(0).expect("ico")),
        marker_danger: meshes.add(Cone {
            radius: 0.3,
            height: 0.55,
        }),
        marker_risky: meshes.add(Torus::new(0.85, 1.0)),
        marker_danger_mat: materials.add(marker(LinearRgba::rgb(1.0, 0.1, 0.08))),
        marker_risky_mat: materials.add(marker(LinearRgba::rgb(1.0, 0.7, 0.1))),
    });
    commands.init_resource::<SpeciesMaterials>();
}

fn rebuild_species_materials(
    balance: Res<Balance>,
    settings: Res<PlayerSettings>,
    info: Option<Res<RunInfo>>,
    mut entered: MessageReader<TierEntered>,
    mut cache: ResMut<SpeciesMaterials>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let entered_tier = entered.read().last().map(|e| e.tier_index);
    let Some(info) = info else { return };
    let tier_index = entered_tier.unwrap_or(info.tier_index);
    if cache.tier_index == Some(tier_index) && entered_tier.is_none() {
        return;
    }
    let mode = settings.accessibility.colorblind_mode;
    cache.by_class = balance
        .tier(tier_index)
        .map(|t| {
            t.species
                .iter()
                .map(|s| {
                    let base = Color::srgb(s.color.0, s.color.1, s.color.2);
                    let jelly = s.shape == Shape::Jelly;
                    std::array::from_fn(|i| {
                        let class = [SizeClass::Edible, SizeClass::Risky, SizeClass::Dangerous][i];
                        materials.add(StandardMaterial {
                            base_color: if jelly { base.with_alpha(0.6) } else { base },
                            alpha_mode: if jelly {
                                AlphaMode::Blend
                            } else {
                                AlphaMode::Opaque
                            },
                            emissive: class_tint(class, mode) * 0.9,
                            perceptual_roughness: 0.6,
                            ..default()
                        })
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    cache.tier_index = Some(tier_index);
}

fn attach_agent_visuals(
    mut commands: Commands,
    balance: Res<Balance>,
    info: Option<Res<RunInfo>>,
    shared: Res<SharedMeshes>,
    cache: Res<SpeciesMaterials>,
    agents: Query<(Entity, &Agent, &Pos, &Radius), Added<Agent>>,
) {
    let Some(info) = info else { return };
    let Some(tier) = balance.tier(info.tier_index) else {
        return;
    };
    for (e, agent, pos, r) in &agents {
        let Some(species) = tier.species.get(agent.species as usize) else {
            continue;
        };
        let Some(mats) = cache.by_class.get(agent.species as usize) else {
            continue;
        };
        let mesh = if species.shape == Shape::Spiky {
            shared.faceted.clone()
        } else {
            shared.smooth.clone()
        };
        let scale = shape_scale(species.shape);
        commands
            .entity(e)
            .insert((
                Mesh3d(mesh),
                MeshMaterial3d(mats[0].clone()),
                Transform::from_translation(pos.0).with_scale(scale * r.0),
                Visibility::default(),
                NotShadowCaster,
                SimVisual { shape_scale: scale },
                ShownClass(None),
            ))
            .with_children(|c| {
                c.spawn((
                    ClassMarker,
                    Mesh3d(shared.marker_risky.clone()),
                    MeshMaterial3d(shared.marker_risky_mat.clone()),
                    Transform::from_xyz(0.0, 1.4, 0.0),
                    Visibility::Hidden,
                    NotShadowCaster,
                ));
            });
    }
}

/// Re-tints creatures as the player grows; shows a ring (risky) or a spike
/// (dangerous) above them so the class never depends on color alone.
fn update_size_classes(
    balance: Res<Balance>,
    cache: Res<SpeciesMaterials>,
    shared: Res<SharedMeshes>,
    player: Query<&Radius, With<Player>>,
    mut agents: Query<
        (
            &Agent,
            &Radius,
            &Children,
            &mut ShownClass,
            &mut MeshMaterial3d<StandardMaterial>,
        ),
        Without<Player>,
    >,
    mut markers: Query<
        (
            &mut Visibility,
            &mut Mesh3d,
            &mut MeshMaterial3d<StandardMaterial>,
            &mut Transform,
        ),
        (With<ClassMarker>, Without<Agent>),
    >,
) {
    let Ok(pr) = player.single() else { return };
    let gate = &balance.core.size_gate;
    for (agent, r, children, mut shown, mut material) in &mut agents {
        let class = classify(r.0, pr.0, gate);
        if shown.0 == Some(class) {
            continue;
        }
        shown.0 = Some(class);
        if let Some(mats) = cache.by_class.get(agent.species as usize) {
            material.0 = mats[class_index(class)].clone();
        }
        for child in children.iter() {
            let Ok((mut vis, mut mesh, mut mat, mut t)) = markers.get_mut(child) else {
                continue;
            };
            match class {
                SizeClass::Edible => *vis = Visibility::Hidden,
                SizeClass::Risky => {
                    *vis = Visibility::Inherited;
                    mesh.0 = shared.marker_risky.clone();
                    mat.0 = shared.marker_risky_mat.clone();
                    *t = Transform::from_xyz(0.0, 0.0, 0.0)
                        .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2))
                        .with_scale(Vec3::splat(1.15));
                }
                SizeClass::Dangerous => {
                    *vis = Visibility::Inherited;
                    mesh.0 = shared.marker_danger.clone();
                    mat.0 = shared.marker_danger_mat.clone();
                    *t = Transform::from_xyz(0.0, 1.35, 0.0);
                }
            }
        }
    }
}

fn spawn_swallow_fx(
    mut commands: Commands,
    balance: Res<Balance>,
    info: Option<Res<RunInfo>>,
    shared: Res<SharedMeshes>,
    cache: Res<SpeciesMaterials>,
    mut consumed: MessageReader<Consumed>,
) {
    let Some(info) = info else {
        consumed.clear();
        return;
    };
    let Some(tier) = balance.tier(info.tier_index) else {
        return;
    };
    for c in consumed.read() {
        let Some(species) = tier.species.get(c.species as usize) else {
            continue;
        };
        let Some(mats) = cache.by_class.get(c.species as usize) else {
            continue;
        };
        let scale = shape_scale(species.shape) * c.radius;
        commands.spawn((
            Name::new("Swallow"),
            SwallowFx {
                t: 0.0,
                start: c.pos,
                scale,
            },
            Mesh3d(shared.smooth.clone()),
            MeshMaterial3d(mats[0].clone()),
            Transform::from_translation(c.pos).with_scale(scale),
            NotShadowCaster,
            DespawnOnExit(AppState::InRun),
        ));
    }
}

/// Prey is dragged into the body and shrinks away.
fn animate_swallow_fx(
    mut commands: Commands,
    time: Res<Time>,
    player: Query<&Transform, (With<Player>, Without<SwallowFx>)>,
    mut fx: Query<(Entity, &mut SwallowFx, &mut Transform)>,
) {
    let Ok(target) = player.single() else { return };
    for (e, mut s, mut t) in &mut fx {
        s.t += time.delta_secs() / SWALLOW_SECS;
        if s.t >= 1.0 {
            commands.entity(e).despawn();
            continue;
        }
        let k = s.t * s.t;
        t.translation = s.start.lerp(target.translation, k);
        t.scale = s.scale * (1.0 - k).max(0.05);
    }
}
