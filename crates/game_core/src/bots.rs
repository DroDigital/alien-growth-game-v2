//! Headless balance bots: a simple player policy plus a batch runner that
//! plays whole runs on `MinimalPlugins` at a fixed, accelerated timestep and
//! reports death-cause distribution, run length and completion rate.
//!
//! The bot uses the same `SimPlugin` as the game; it only writes
//! `PlayerIntent`, exactly like keyboard or gamepad input does.

use crate::balance::{Balance, Behavior};
use crate::rules::{SizeClass, classify, speed_for};
use crate::sim::{
    Agent, DEFAULT_STRAIN, Player, PlayerIntent, PlayerVitals, Pos, Radius, RunInfo, RunOutcome,
    SIM_HZ, SimPlugin, SimSet, Struggling, Vel, advance_tier, start_run,
};
use crate::states::{AppState, FlowStatesPlugin, RunState};
use bevy::ecs::schedule::SingleThreadedExecutor;
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy::time::TimeUpdateStrategy;
use rand::{RngExt, SeedableRng};
use rand_chacha::ChaCha8Rng;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::Duration;

/// Per-run bot personality, drawn from the run seed.
#[derive(Resource, Debug, Clone, Copy)]
pub struct BotProfile {
    /// Consume presses per second while struggling.
    pub mash_rate: f32,
    /// Multiplier on how early threats are noticed.
    pub caution: f32,
    /// Willingness to start struggles (0..=1).
    pub greed: f32,
    /// Reaction noise: chance per step to keep the previous intent.
    pub sloppiness: f32,
}

impl BotProfile {
    pub fn from_seed(seed: u64) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(seed ^ 0xB07_B07);
        Self {
            mash_rate: rng.random_range(4.0..10.0),
            caution: rng.random_range(0.6..1.4),
            greed: rng.random_range(0.0..1.0),
            sloppiness: rng.random_range(0.0..0.5),
        }
    }
}

#[derive(Resource, Default)]
struct BotState {
    mash_acc: f32,
    /// Seconds to avoid risky targets after losing a struggle.
    risky_cooldown: f32,
    was_struggling: bool,
    integrity_before: f32,
    rng: Option<ChaCha8Rng>,
    wander: Vec3,
    wander_timer: f32,
}

/// Drives `PlayerIntent` from a `BotProfile`. With `auto_advance`, tier
/// transitions happen instantly (no cinematic).
pub struct BotPlugin {
    pub auto_advance: bool,
}

impl Plugin for BotPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BotState>()
            .add_systems(FixedUpdate, bot_policy.in_set(SimSet::Intent));
        if self.auto_advance {
            app.add_systems(OnEnter(RunState::TierTransition), auto_advance);
        }
    }
}

fn auto_advance(world: &mut World) {
    advance_tier(world);
    world
        .resource_mut::<NextState<RunState>>()
        .set(RunState::Playing);
}

fn bot_policy(
    time: Res<Time>,
    balance: Res<Balance>,
    info: Res<RunInfo>,
    profile: Option<Res<BotProfile>>,
    mut state: ResMut<BotState>,
    mut intent: ResMut<PlayerIntent>,
    player: Query<(&Pos, &Radius, &PlayerVitals, Has<Struggling>), With<Player>>,
    agents: Query<(&Agent, &Pos, &Vel, &Radius)>,
) {
    let dt = time.delta_secs();
    let profile = profile
        .map(|p| *p)
        .unwrap_or(BotProfile::from_seed(info.seed));
    let rng = state
        .rng
        .get_or_insert_with(|| ChaCha8Rng::seed_from_u64(info.seed ^ 0x5EED));
    if rng.random::<f32>() < profile.sloppiness * 0.2 {
        return; // keep last intent this step
    }
    let Ok((ppos, pr, vitals, struggling)) = player.single() else {
        return;
    };
    state.risky_cooldown = (state.risky_cooldown - dt).max(0.0);
    if state.was_struggling && !struggling && vitals.0.integrity < state.integrity_before {
        // Lost a struggle: like a player, be wary of that size class for a while.
        state.risky_cooldown = 45.0;
    }
    state.was_struggling = struggling;
    if !struggling {
        state.integrity_before = vitals.0.integrity;
    }
    if struggling {
        state.mash_acc += profile.mash_rate * dt;
        let presses = state.mash_acc.floor();
        state.mash_acc -= presses;
        intent.consume_presses += presses as u32;
        intent.move_dir = Vec3::ZERO;
        return;
    }

    let tier = info.tier(&balance);
    let gate = &balance.core.size_gate;
    let met = &balance.core.metabolism;
    let hungry = vitals.0.can_ingest(met);

    // Threats: hunters larger than us, weighted by proximity.
    let mut flee = Vec3::ZERO;
    let mut nearest_threat = f32::MAX;
    // Food: best mass-per-distance target.
    let mut best: Option<(f32, Vec3)> = None;
    for (agent, apos, avel, ar) in &agents {
        let species = &tier.species[agent.species as usize];
        let to = apos.0 - ppos.0;
        let d = to.length().max(1e-3);
        match classify(ar.0, pr.0, gate) {
            SizeClass::Dangerous => {
                if species.behavior == Behavior::Hunter {
                    let range = (species.sense * ar.0 + pr.0) * profile.caution;
                    if d < range {
                        flee -= to / d * (1.0 - d / range);
                        nearest_threat = nearest_threat.min(d - ar.0 - pr.0);
                    }
                }
            }
            class => {
                if !hungry {
                    continue;
                }
                let risky = class == SizeClass::Risky;
                let skilled = profile.mash_rate >= 6.5;
                if risky
                    && (profile.greed < 0.5
                        || !skilled
                        || state.risky_cooldown > 0.0
                        || vitals.0.integrity < 60.0
                        || ar.0 / pr.0 > 1.05)
                {
                    continue;
                }
                let mass = (ar.0 / pr.0).powi(3);
                let score = mass / (d / pr.0 + 2.0).powf(1.6) * if risky { 0.6 } else { 1.0 };
                if best.is_none_or(|(s, _)| score > s) {
                    // Lead the target a little.
                    let lead = apos.0
                        + avel.0 * (d / speed_for(pr.0, 1.0, &balance.core.locomotion)).min(1.0);
                    best = Some((score, lead));
                }
            }
        }
    }

    intent.dash = false;
    // Like a player: bolt when a threat is close or pressing; otherwise keep
    // feeding while steering away from it.
    let urgent = flee.length() > 0.6 || nearest_threat < 2.0 + 4.0 * pr.0;
    if flee.length_squared() > 0.0 && (urgent || best.is_none()) {
        intent.move_dir = flee.normalize_or_zero();
        if nearest_threat < 3.0 * pr.0 && vitals.0.energy > 15.0 {
            intent.dash = true;
        }
        return;
    }
    if let Some((_, target)) = best {
        let food = (target - ppos.0).normalize_or_zero();
        intent.move_dir = (food + flee * 1.5).normalize_or_zero();
        return;
    }
    state.wander_timer -= dt;
    if state.wander_timer <= 0.0 {
        let rng = state.rng.as_mut().expect("initialised above");
        let a = rng.random_range(0.0..std::f32::consts::TAU);
        state.wander = Vec3::new(a.cos(), 0.0, a.sin());
        state.wander_timer = 3.0;
    }
    intent.move_dir = state.wander;
}

/// How one bot run ended.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BotEnd {
    Completed,
    Died(String),
    Timeout,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BotResult {
    pub seed: u64,
    pub end: BotEnd,
    pub sim_secs: f32,
    pub tier_reached: u8,
    pub peak_biomass: f32,
    pub meals: u32,
}

/// Builds a headless app that runs the simulation one fixed step per update.
pub fn headless_app(balance: &Balance) -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, StatesPlugin))
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
            1.0 / SIM_HZ,
        )))
        .insert_resource(Time::<Fixed>::from_hz(SIM_HZ))
        .insert_resource(balance.clone())
        .add_plugins((
            FlowStatesPlugin,
            SimPlugin,
            BotPlugin { auto_advance: true },
        ));
    // Many runs execute in parallel; per-app multithreading only adds overhead.
    for (_, schedule) in app.world_mut().resource_mut::<Schedules>().iter_mut() {
        schedule.set_executor(SingleThreadedExecutor::new());
    }
    app
}

/// Plays one run to completion, death or `max_secs` of sim time.
pub fn run_bot(seed: u64, balance: &Balance, max_secs: f32) -> BotResult {
    let mut app = headless_app(balance);
    app.insert_resource(BotProfile::from_seed(seed));
    app.world_mut()
        .resource_mut::<NextState<AppState>>()
        .set(AppState::InRun);
    app.update();
    start_run(app.world_mut(), seed, DEFAULT_STRAIN, None);
    let steps = (max_secs as f64 * SIM_HZ) as u64;
    for _ in 0..steps {
        app.update();
        if app.world().resource::<RunInfo>().outcome.is_some() {
            break;
        }
    }
    let info = app.world().resource::<RunInfo>().clone();
    let end = match info.outcome.clone() {
        Some(RunOutcome::Completed) => BotEnd::Completed,
        Some(RunOutcome::Died(cause)) => BotEnd::Died(cause.key()),
        None => BotEnd::Timeout,
    };
    BotResult {
        seed,
        end,
        sim_secs: info.elapsed,
        tier_reached: info.tier(balance).tier,
        peak_biomass: info.peak_biomass,
        meals: info.meals,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BatchReport {
    pub runs: usize,
    pub completed: usize,
    pub timeouts: usize,
    pub deaths: usize,
    /// Death cause key -> count.
    pub death_causes: BTreeMap<String, usize>,
    /// Largest single cause as a fraction of all deaths.
    pub max_cause_share: f32,
    pub max_cause: String,
    pub mean_sim_secs: f32,
    pub reached_tier: BTreeMap<u8, usize>,
    pub mean_peak_biomass: f32,
    /// Mean sim time of completed runs.
    pub mean_completion_secs: f32,
}

impl BatchReport {
    pub fn from_results(results: &[BotResult]) -> Self {
        let mut causes: BTreeMap<String, usize> = BTreeMap::new();
        let mut reached: BTreeMap<u8, usize> = BTreeMap::new();
        let (mut completed, mut timeouts) = (0, 0);
        for r in results {
            *reached.entry(r.tier_reached).or_default() += 1;
            match &r.end {
                BotEnd::Completed => completed += 1,
                BotEnd::Timeout => timeouts += 1,
                BotEnd::Died(c) => *causes.entry(c.clone()).or_default() += 1,
            }
        }
        let deaths: usize = causes.values().sum();
        let (max_cause, max_n) = causes
            .iter()
            .max_by_key(|(_, n)| **n)
            .map(|(c, n)| (c.clone(), *n))
            .unwrap_or_default();
        let n = results.len().max(1) as f32;
        let done: Vec<f32> = results
            .iter()
            .filter(|r| r.end == BotEnd::Completed)
            .map(|r| r.sim_secs)
            .collect();
        Self {
            mean_completion_secs: done.iter().sum::<f32>() / done.len().max(1) as f32,
            runs: results.len(),
            completed,
            timeouts,
            deaths,
            death_causes: causes,
            max_cause_share: if deaths > 0 {
                max_n as f32 / deaths as f32
            } else {
                0.0
            },
            max_cause,
            mean_sim_secs: results.iter().map(|r| r.sim_secs).sum::<f32>() / n,
            reached_tier: reached,
            mean_peak_biomass: results.iter().map(|r| r.peak_biomass).sum::<f32>() / n,
        }
    }

    pub fn print(&self) {
        println!(
            "runs {}  completed {} ({:.1}%)  deaths {}  timeouts {}  mean run {:.0}s  mean peak biomass {:.0}",
            self.runs,
            self.completed,
            100.0 * self.completed as f32 / self.runs.max(1) as f32,
            self.deaths,
            self.timeouts,
            self.mean_sim_secs,
            self.mean_peak_biomass
        );
        println!("mean time to complete: {:.0}s", self.mean_completion_secs);
        println!("death causes (share of deaths):");
        for (cause, n) in &self.death_causes {
            println!(
                "  {cause:<32} {n:>6}  {:>5.1}%",
                100.0 * *n as f32 / self.deaths.max(1) as f32
            );
        }
        println!("furthest tier reached:");
        for (tier, n) in &self.reached_tier {
            println!("  tier {tier}: {n}");
        }
    }
}

/// Runs `runs` bots in parallel with seeds `first_seed..`.
pub fn run_batch(balance: &Balance, runs: usize, first_seed: u64, max_secs: f32) -> Vec<BotResult> {
    (0..runs as u64)
        .into_par_iter()
        .map(|i| run_bot(first_seed + i, balance, max_secs))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bot_runs_are_deterministic_and_progress() {
        let balance = Balance::builtin();
        let a = run_bot(3, &balance, 120.0);
        let b = run_bot(3, &balance, 120.0);
        assert_eq!(a, b, "same seed, same run");
        assert!(a.meals > 0, "the bot eats something: {a:?}");
    }

    #[test]
    fn report_counts_causes() {
        let mk = |end| BotResult {
            seed: 0,
            end,
            sim_secs: 10.0,
            tier_reached: 1,
            peak_biomass: 5.0,
            meals: 1,
        };
        let r = BatchReport::from_results(&[
            mk(BotEnd::Died("starvation".into())),
            mk(BotEnd::Died("predation:x".into())),
            mk(BotEnd::Died("predation:x".into())),
            mk(BotEnd::Completed),
        ]);
        assert_eq!(r.deaths, 3);
        assert_eq!(r.completed, 1);
        assert_eq!(r.max_cause, "predation:x");
        assert!((r.max_cause_share - 2.0 / 3.0).abs() < 1e-6);
    }
}
