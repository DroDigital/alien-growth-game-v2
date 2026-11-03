//! Scene registry. A scene is a named, seeded setup that spawns entities
//! tagged with [`SceneMember`]. Static scenes (menu background, testbed, reef
//! preview) load through [`LoadScene`]; during runs the strata plugin builds
//! each tier's world with the same [`worlds::WorldSpawner`].

pub mod worlds;

use crate::graphics::WorldCamera;
use bevy::prelude::*;
use game_core::balance::Biome;
use game_core::states::AppState;
use game_core::terrain::Terrain;
use worlds::{Shot, WorldSpec};

/// Static scenes accepted by `--screenshot <scene>`. The tooling also accepts
/// run scenes (`run-tier1`, `run-tier2`, `death`), see `tooling/screenshot.rs`.
pub const SCENES: &[&str] = &["testbed", "menu", "reef"];

/// Fixed seed: static scenes must render identically every time.
const STATIC_SEED: u32 = 0x5EAF_100D;

/// Every entity spawned by a scene; despawned when the scene changes.
#[derive(Component)]
pub struct SceneMember;

/// Request to (re)load a scene by name.
#[derive(Message, Debug, Clone)]
pub struct LoadScene {
    pub name: String,
    /// Animate the camera (false for deterministic screenshots).
    pub animate_camera: bool,
}

#[derive(Resource, Debug, Default)]
pub struct ActiveScene {
    pub name: Option<String>,
}

/// Simple scripted camera used by scenes, the menu and the benchmark.
#[derive(Component, Debug, Clone)]
pub struct OrbitShot {
    pub center: Vec3,
    pub radius: f32,
    pub height: f32,
    /// Radians.
    pub angle: f32,
    /// Radians per second (0 = static).
    pub speed: f32,
    /// Vertical bob amplitude for flythroughs.
    pub bob: f32,
}

pub struct ScenesPlugin;

impl Plugin for ScenesPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<LoadScene>()
            .init_resource::<ActiveScene>()
            .add_plugins(worlds::WorldsPlugin)
            .add_systems(
                OnEnter(AppState::MainMenu),
                |mut w: MessageWriter<LoadScene>| {
                    w.write(LoadScene {
                        name: "menu".into(),
                        animate_camera: true,
                    });
                },
            )
            .add_systems(Update, (handle_load_scene, drive_orbit_camera).chain());
    }
}

pub fn is_known_scene(name: &str) -> bool {
    SCENES.contains(&name)
}

fn handle_load_scene(
    mut commands: Commands,
    mut requests: MessageReader<LoadScene>,
    mut active: ResMut<ActiveScene>,
    members: Query<Entity, With<SceneMember>>,
    mut spawn: worlds::WorldSpawner,
    camera: Query<Entity, With<WorldCamera>>,
) {
    let Some(req) = requests.read().last().cloned() else {
        return;
    };
    if active.name.as_deref() == Some(req.name.as_str()) {
        return;
    }
    for e in &members {
        commands.entity(e).despawn();
    }
    let Ok(cam) = camera.single() else {
        error!("no world camera; cannot load scene {}", req.name);
        return;
    };
    let (biome, radius, shot) = match req.name.as_str() {
        "testbed" => (Biome::CraterSeafloor, 30.0, Shot::Hero),
        "menu" => (Biome::CraterSeafloor, 30.0, Shot::Menu),
        "reef" => (Biome::CoralReef, 120.0, Shot::Hero),
        other => {
            error!("unknown scene '{other}' (known: {SCENES:?})");
            return;
        }
    };
    spawn.build(&WorldSpec {
        biome,
        terrain: Terrain::new(biome, STATIC_SEED),
        radius,
        camera: cam,
        shot: Some((shot, req.animate_camera)),
        stand_in_organism: biome == Biome::CraterSeafloor,
    });
    info!("scene loaded: {}", req.name);
    active.name = Some(req.name);
}

fn drive_orbit_camera(time: Res<Time>, mut q: Query<(&mut Transform, &mut OrbitShot)>) {
    for (mut t, mut shot) in &mut q {
        shot.angle += shot.speed * time.delta_secs();
        let bob = shot.bob * (shot.angle * 0.7).sin();
        let pos = shot.center
            + Vec3::new(
                shot.angle.cos() * shot.radius,
                shot.height + bob,
                shot.angle.sin() * shot.radius,
            );
        *t = Transform::from_translation(pos).looking_at(shot.center, Vec3::Y);
    }
}
