//! The run simulation: player, populations, AI, eating, struggles, damage,
//! metabolism and tier progression. Runs in `FixedUpdate` while
//! `RunState::Playing`, identically in the game and in the headless balance
//! bots (`MinimalPlugins`).
//!
//! Sim state lives in sim-only components ([`Pos`], [`Vel`], [`Radius`]...);
//! the game reads them to drive meshes, cameras, UI and audio, and writes only
//! [`PlayerIntent`]. Cross-plugin communication uses the messages below.

mod systems;

use crate::balance::{Balance, SpeciesDef, TierDef};
use crate::rules::{DeathCause, Struggle, Vitals, mass_for_radius};
use crate::states::{AppState, RunState};
use crate::terrain::Terrain;
use bevy::prelude::*;
use rand::{RngExt, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};

/// Simulation tick rate (Hz). Bots step at exactly this rate.
pub const SIM_HZ: f64 = 64.0;

/// The strain used when none is specified (M1 ships only this one).
pub const DEFAULT_STRAIN: &str = "devourer";

// --- Components ----------------------------------------------------------------

/// Everything spawned by the simulation; despawned when a run (re)starts.
#[derive(Component)]
pub struct SimEntity;

/// Simulation position (world units).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Pos(pub Vec3);

/// Position at the start of the current fixed step (for render interpolation).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct PrevPos(pub Vec3);

#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Vel(pub Vec3);

#[derive(Component, Clone, Copy, Debug)]
pub struct Radius(pub f32);

/// The player organism.
#[derive(Component, Debug, Default)]
pub struct Player {
    pub dash_timer: f32,
    pub dash_cooldown: f32,
    /// Seconds of invulnerability left after a bite.
    pub hurt_timer: f32,
    pub was_starving: bool,
    pub was_gorged: bool,
}

#[derive(Component, Debug, Clone)]
pub struct PlayerVitals(pub Vitals);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AgentMode {
    #[default]
    Wander,
    Flee,
    Hunt,
}

/// A non-player creature.
#[derive(Component, Debug, Clone)]
pub struct Agent {
    /// Index into the current tier's species list.
    pub species: u16,
    pub mode: AgentMode,
    pub wander_dir: Vec3,
    pub wander_timer: f32,
    /// 0..=1; fleeing burns it, wandering restores it.
    pub stamina: f32,
    pub bite_cooldown: f32,
}

/// The player is in a struggle with `target`.
#[derive(Component, Debug, Clone)]
pub struct Struggling {
    pub target: Entity,
    pub species: u16,
    pub struggle: Struggle,
}

/// This agent is held in a struggle and does not move on its own.
#[derive(Component, Debug)]
pub struct Grappled;

// --- Resources -----------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RunOutcome {
    Died(DeathCause),
    /// The last available tier was completed.
    Completed,
}

/// The current run.
#[derive(Resource, Debug, Clone)]
pub struct RunInfo {
    pub seed: u64,
    pub strain: String,
    pub tier_index: usize,
    /// Seconds since the run started.
    pub elapsed: f32,
    pub tier_elapsed: f32,
    pub peak_biomass: f32,
    pub meals: u32,
    pub outcome: Option<RunOutcome>,
}

impl RunInfo {
    pub fn tier<'a>(&self, balance: &'a Balance) -> &'a TierDef {
        balance
            .tier(self.tier_index)
            .expect("tier index within the strata table")
    }
}

#[derive(Resource)]
pub struct SimRng(pub ChaCha8Rng);

/// The playable volume of the current tier.
#[derive(Resource, Debug, Clone)]
pub struct Arena {
    pub radius: f32,
    pub ceiling: f32,
    pub terrain: Terrain,
}

impl Arena {
    pub fn floor(&self, x: f32, z: f32) -> f32 {
        self.terrain.height(x, z)
    }
}

/// What the player wants to do this step. Written by input (game) or a bot.
#[derive(Resource, Debug, Clone, Default)]
pub struct PlayerIntent {
    /// World-space desired direction; length <= 1.
    pub move_dir: Vec3,
    pub dash: bool,
    /// Consume presses since the last fixed step (struggle mashing).
    pub consume_presses: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LogKind {
    TierStarted(u8),
    Meal { species: String, mass: f32 },
    StruggleWon(String),
    StruggleLost(String),
    Bitten { species: String, damage: f32 },
    Starving,
    Gorged,
    TierCompleted(u8),
    Died(DeathCause),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogEntry {
    pub t: f32,
    pub kind: LogKind,
}

/// Run timeline and position samples (death screen timeline and heatmap).
#[derive(Resource, Debug, Clone, Default)]
pub struct RunLog {
    pub entries: Vec<LogEntry>,
    /// (time, tier index, position) every [`SAMPLE_SECS`].
    pub samples: Vec<(f32, usize, Vec3)>,
}

/// Seconds between heatmap position samples.
pub const SAMPLE_SECS: f32 = 2.0;
/// Meals are numerous; only every Nth is logged after the first few.
const MEAL_LOG_EVERY: u32 = 10;

impl RunLog {
    pub fn push(&mut self, t: f32, kind: LogKind) {
        self.entries.push(LogEntry { t, kind });
    }
}

/// Species waiting to respawn: (species index, seconds left).
#[derive(Resource, Debug, Default)]
pub struct PendingRespawns(pub Vec<(u16, f32)>);

// --- Messages ------------------------------------------------------------------

#[derive(Message, Debug, Clone)]
pub struct Consumed {
    pub species: u16,
    pub pos: Vec3,
    pub radius: f32,
    pub mass: f32,
    /// Prey size / player size at the moment of eating.
    pub ratio: f32,
}

#[derive(Message, Debug, Clone)]
pub struct StruggleStarted {
    pub species: u16,
    pub target: Entity,
}

#[derive(Message, Debug, Clone)]
pub struct StruggleEnded {
    pub species: u16,
    pub won: bool,
}

#[derive(Message, Debug, Clone)]
pub struct PlayerBitten {
    pub species: u16,
    pub damage: f32,
    pub from: Vec3,
}

#[derive(Message, Debug, Clone)]
pub struct TierCompleted {
    pub tier: u8,
    /// No further tier exists: the run is won.
    pub last: bool,
}

#[derive(Message, Debug, Clone)]
pub struct PlayerDied {
    pub cause: DeathCause,
}

/// A run started or a new tier was entered (presentation rebuilds the world).
#[derive(Message, Debug, Clone)]
pub struct TierEntered {
    pub tier_index: usize,
}

// --- Plugin --------------------------------------------------------------------

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum SimSet {
    /// Bots and input fill `PlayerIntent` here.
    Intent,
    Ai,
    Movement,
    Interactions,
    Metabolism,
    Progress,
}

pub struct SimPlugin;

impl Plugin for SimPlugin {
    fn build(&self, app: &mut App) {
        if !app.world().contains_resource::<Balance>() {
            app.insert_resource(Balance::builtin());
        }
        app.init_resource::<PlayerIntent>()
            .init_resource::<RunLog>()
            .init_resource::<PendingRespawns>()
            .add_message::<Consumed>()
            .add_message::<StruggleStarted>()
            .add_message::<StruggleEnded>()
            .add_message::<PlayerBitten>()
            .add_message::<TierCompleted>()
            .add_message::<PlayerDied>()
            .add_message::<TierEntered>()
            .configure_sets(
                FixedUpdate,
                (
                    SimSet::Intent,
                    SimSet::Ai,
                    SimSet::Movement,
                    SimSet::Interactions,
                    SimSet::Metabolism,
                    SimSet::Progress,
                )
                    .chain()
                    .run_if(in_state(RunState::Playing).and_then(resource_exists::<RunInfo>)),
            )
            .add_systems(
                FixedUpdate,
                (
                    (systems::record_prev_pos, systems::agent_ai)
                        .chain()
                        .in_set(SimSet::Ai),
                    (systems::player_motion, systems::integrate)
                        .chain()
                        .in_set(SimSet::Movement),
                    (systems::contacts, systems::struggle_step)
                        .chain()
                        .in_set(SimSet::Interactions),
                    systems::metabolism.in_set(SimSet::Metabolism),
                    (systems::progress, systems::respawn, systems::clear_intent)
                        .chain()
                        .in_set(SimSet::Progress),
                ),
            )
            .add_systems(OnExit(AppState::InRun), end_run);
    }
}

// --- Run lifecycle (call through `Commands::queue` or with `&mut World`) -------

/// Saved state to resume a run at the start of a tier.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunCheckpoint {
    pub seed: u64,
    pub strain: String,
    pub tier_index: usize,
    pub vitals: Vitals,
    pub elapsed: f32,
    pub peak_biomass: f32,
    pub meals: u32,
}

/// Starts a fresh run (or resumes from a checkpoint) on `world`.
pub fn start_run(world: &mut World, seed: u64, strain: &str, resume: Option<RunCheckpoint>) {
    despawn_sim(world);
    let balance = world.resource::<Balance>().clone();
    let strain = if balance.strain(strain).is_some() {
        strain.to_string()
    } else {
        DEFAULT_STRAIN.to_string()
    };
    let (tier_index, vitals, elapsed, peak, meals) = match &resume {
        Some(c) => (
            c.tier_index.min(balance.last_tier_index()),
            c.vitals.clone(),
            c.elapsed,
            c.peak_biomass,
            c.meals,
        ),
        None => {
            let b = balance.tier(0).expect("tier 0").start_biomass;
            (0, Vitals::new(b, &balance.core.metabolism), 0.0, b, 0)
        }
    };
    world.insert_resource(RunInfo {
        seed,
        strain,
        tier_index,
        elapsed,
        tier_elapsed: 0.0,
        peak_biomass: peak,
        meals,
        outcome: None,
    });
    world.insert_resource(RunLog::default());
    world.insert_resource(PlayerIntent::default());
    let met = &balance.core.metabolism;
    let radius = vitals.radius(met);
    world.spawn((
        SimEntity,
        Player::default(),
        PlayerVitals(vitals),
        Pos(Vec3::ZERO),
        PrevPos(Vec3::ZERO),
        Vel(Vec3::ZERO),
        Radius(radius),
    ));
    enter_tier(world, tier_index);
}

/// Moves the run to the next tier. Returns false if there is none.
pub fn advance_tier(world: &mut World) -> bool {
    let next = world.resource::<RunInfo>().tier_index + 1;
    if world.resource::<Balance>().tier(next).is_none() {
        return false;
    }
    enter_tier(world, next);
    true
}

fn tier_seed(seed: u64, tier_index: usize) -> u64 {
    seed ^ (0x9E37_79B9_7F4A_7C15u64.wrapping_mul(tier_index as u64 + 1))
}

fn enter_tier(world: &mut World, tier_index: usize) {
    // Remove the previous tier's population.
    let agents: Vec<Entity> = world
        .query_filtered::<Entity, With<Agent>>()
        .iter(world)
        .collect();
    for e in agents {
        world.despawn(e);
    }
    let balance = world.resource::<Balance>().clone();
    let tier = balance.tier(tier_index).expect("valid tier").clone();
    let (seed, elapsed) = {
        let mut info = world.resource_mut::<RunInfo>();
        info.tier_index = tier_index;
        info.tier_elapsed = 0.0;
        (info.seed, info.elapsed)
    };
    let mut rng = ChaCha8Rng::seed_from_u64(tier_seed(seed, tier_index));
    let terrain = Terrain::new(tier.biome, rng.random::<u32>());
    let arena = Arena {
        radius: tier.arena_radius,
        ceiling: tier.ceiling,
        terrain,
    };

    // Player: at least the tier's starting biomass, placed near the centre.
    let met = balance.core.metabolism.clone();
    let player_pos = {
        let x = 1.6;
        let z = 1.2;
        Vec3::new(x, arena.floor(x, z) + 1.5, z)
    };
    let mut player_radius = 0.0;
    let mut q = world.query::<(
        &mut PlayerVitals,
        &mut Pos,
        &mut PrevPos,
        &mut Vel,
        &mut Radius,
        &mut Player,
    )>();
    for (mut v, mut p, mut pp, mut vel, mut r, mut pl) in q.iter_mut(world) {
        v.0.biomass = v.0.biomass.max(tier.start_biomass);
        r.0 = v.0.radius(&met);
        let pos = player_pos + Vec3::Y * r.0;
        p.0 = pos;
        pp.0 = pos;
        vel.0 = Vec3::ZERO;
        *pl = Player::default();
        player_radius = r.0;
    }
    let strugglers: Vec<Entity> = world
        .query_filtered::<Entity, With<Struggling>>()
        .iter(world)
        .collect();
    for e in strugglers {
        world.entity_mut(e).remove::<Struggling>();
    }

    // Population.
    for (i, species) in tier.species.iter().enumerate() {
        for _ in 0..species.count {
            spawn_agent(
                world,
                &mut rng,
                &arena,
                i as u16,
                species,
                player_pos,
                player_radius,
            );
        }
    }
    world.insert_resource(arena);
    world.insert_resource(SimRng(rng));
    world.insert_resource(PendingRespawns::default());
    world
        .resource_mut::<RunLog>()
        .push(elapsed, LogKind::TierStarted(tier.tier));
    world.write_message(TierEntered { tier_index });
}

/// Spawns one agent somewhere in the arena, away from the player.
pub(crate) fn spawn_agent(
    world: &mut World,
    rng: &mut ChaCha8Rng,
    arena: &Arena,
    species_index: u16,
    species: &SpeciesDef,
    player_pos: Vec3,
    player_radius: f32,
) {
    let radius = rng.random_range(species.radius.0..=species.radius.1);
    let min_dist = 6.0 + 8.0 * player_radius + 2.0 * radius;
    let mut pos = Vec3::ZERO;
    for _ in 0..16 {
        let a = rng.random_range(0.0..std::f32::consts::TAU);
        let d = arena.radius * rng.random::<f32>().sqrt() * 0.95;
        let (x, z) = (a.cos() * d, a.sin() * d);
        let floor = arena.floor(x, z);
        let span = (arena.ceiling - floor - 2.0 * radius).max(0.0);
        let y = floor + radius + rng.random::<f32>().powi(2) * span * 0.7;
        pos = Vec3::new(x, y, z);
        if pos.distance(player_pos) > min_dist {
            break;
        }
    }
    let dir = random_dir(rng);
    world.spawn((
        SimEntity,
        Agent {
            species: species_index,
            mode: AgentMode::Wander,
            wander_dir: dir,
            wander_timer: rng.random_range(0.5..4.0),
            stamina: 1.0,
            bite_cooldown: 0.0,
        },
        Pos(pos),
        PrevPos(pos),
        Vel(Vec3::ZERO),
        Radius(radius),
    ));
}

pub(crate) fn random_dir(rng: &mut ChaCha8Rng) -> Vec3 {
    let a = rng.random_range(0.0..std::f32::consts::TAU);
    let y = rng.random_range(-0.25..0.25);
    Vec3::new(a.cos(), y, a.sin()).normalize()
}

/// Mass a creature of this radius yields when eaten.
pub fn prey_mass(radius: f32, balance: &Balance) -> f32 {
    mass_for_radius(radius, balance.core.metabolism.base_radius)
}

/// Removes every simulation entity and run resource.
pub fn despawn_sim(world: &mut World) {
    let ents: Vec<Entity> = world
        .query_filtered::<Entity, With<SimEntity>>()
        .iter(world)
        .collect();
    for e in ents {
        world.despawn(e);
    }
    world.remove_resource::<RunInfo>();
    world.remove_resource::<Arena>();
}

fn end_run(mut commands: Commands) {
    commands.queue(despawn_sim);
}

#[cfg(test)]
mod tests;
