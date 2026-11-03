use super::*;
use crate::bots::headless_app;
use crate::rules::Vitals;

fn app_with_run(seed: u64) -> App {
    let mut app = headless_app(&Balance::builtin());
    app.world_mut()
        .resource_mut::<NextState<AppState>>()
        .set(AppState::InRun);
    app.update();
    start_run(app.world_mut(), seed, DEFAULT_STRAIN, None);
    app
}

fn player_vitals(app: &mut App) -> Vitals {
    let w = app.world_mut();
    w.query::<&PlayerVitals>().single(w).unwrap().0.clone()
}

#[test]
fn start_run_spawns_player_and_population() {
    let mut app = app_with_run(1);
    let w = app.world_mut();
    let agents = w.query::<&Agent>().iter(w).count();
    let expected: u32 = Balance::builtin()
        .tier(0)
        .unwrap()
        .species
        .iter()
        .map(|s| s.count)
        .sum();
    assert_eq!(agents as u32, expected);
    assert_eq!(w.query::<&Player>().iter(w).count(), 1);
}

#[test]
fn idle_player_starves_to_death() {
    let mut app = app_with_run(2);
    // Remove all creatures: nothing to eat, nothing to bite.
    let w = app.world_mut();
    let agents: Vec<Entity> = w.query_filtered::<Entity, With<Agent>>().iter(w).collect();
    for e in agents {
        w.despawn(e);
    }
    w.resource_mut::<PendingRespawns>().0.clear();
    // Keep the bot from doing anything useful.
    w.remove_resource::<crate::bots::BotProfile>();
    for _ in 0..(SIM_HZ as usize * 600) {
        app.update();
        if app.world().resource::<RunInfo>().outcome.is_some() {
            break;
        }
    }
    let info = app.world().resource::<RunInfo>();
    assert_eq!(info.outcome, Some(RunOutcome::Died(DeathCause::Starvation)));
    let log = app.world().resource::<RunLog>();
    assert!(log.entries.iter().any(|e| e.kind == LogKind::Starving));
}

#[test]
fn reaching_the_goal_completes_tier_and_advances() {
    let mut app = app_with_run(3);
    let goal = Balance::builtin().tier(0).unwrap().complete_biomass;
    {
        let w = app.world_mut();
        let mut v = w.query::<&mut PlayerVitals>().single_mut(w).unwrap();
        v.0.biomass = goal + 1.0;
    }
    for _ in 0..4 {
        app.update();
    }
    let info = app.world().resource::<RunInfo>();
    assert_eq!(info.tier_index, 1, "bot plugin auto-advances");
    assert!(player_vitals(&mut app).biomass >= goal);
}

#[test]
fn checkpoint_resume_restores_tier_and_vitals() {
    let balance = Balance::builtin();
    let mut app = headless_app(&balance);
    app.world_mut()
        .resource_mut::<NextState<AppState>>()
        .set(AppState::InRun);
    app.update();
    let mut vitals = Vitals::new(300.0, &balance.core.metabolism);
    vitals.energy = 42.0;
    let cp = RunCheckpoint {
        seed: 9,
        strain: DEFAULT_STRAIN.into(),
        tier_index: 1,
        vitals: vitals.clone(),
        elapsed: 100.0,
        peak_biomass: 300.0,
        meals: 12,
    };
    start_run(app.world_mut(), 9, DEFAULT_STRAIN, Some(cp));
    let info = app.world().resource::<RunInfo>().clone();
    assert_eq!(info.tier_index, 1);
    assert_eq!(info.meals, 12);
    assert_eq!(player_vitals(&mut app).energy, 42.0);
}
