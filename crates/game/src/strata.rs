//! Strata: starting runs, building each tier's world when the simulation
//! enters it, the skippable zoom-out cinematic between tiers, and the stratum
//! title card.

use crate::boot::{LaunchContext, Locale};
use crate::graphics::WorldCamera;
use crate::graphics::follow::FollowCamera;
use crate::input::{GameAction, PlayerInput};
use crate::scenes::worlds::{WorldSpawner, WorldSpec};
use crate::scenes::{ActiveScene, SceneMember};
use crate::ui::theme::Theme;
use bevy::prelude::*;
use game_core::balance::Balance;
use game_core::sim::{
    Arena, DEFAULT_STRAIN, RunCheckpoint, RunInfo, TierEntered, advance_tier, start_run,
};
use game_core::states::{AppState, RunState};
use leafwing_input_manager::prelude::ActionState;

/// How the next run should start. Set by the menu (or CLI) before entering
/// `AppState::InRun`; consumed on entry.
#[derive(Resource, Debug, Clone, Default)]
pub struct PendingRun {
    pub seed: Option<u64>,
    pub resume: Option<RunCheckpoint>,
}

/// Length of the tier-up cinematic (seconds); the world swaps at 60%.
const TRANSITION_SECS: f32 = 3.6;
const SWAP_AT: f32 = 0.6;
const TITLE_SECS: f32 = 4.5;

pub struct StrataPlugin;

impl Plugin for StrataPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PendingRun>()
            .add_systems(OnEnter(AppState::InRun), begin_run)
            .add_systems(
                Update,
                (build_tier_world, fade_title_card).run_if(in_state(AppState::InRun)),
            )
            .add_systems(OnEnter(RunState::TierTransition), start_transition)
            .add_systems(
                Update,
                run_transition.run_if(in_state(RunState::TierTransition)),
            );
    }
}

fn begin_run(mut commands: Commands, ctx: Res<LaunchContext>, mut pending: ResMut<PendingRun>) {
    let p = std::mem::take(&mut *pending);
    let seed = p
        .resume
        .as_ref()
        .map(|c| c.seed)
        .or(p.seed)
        .or(ctx.cli.seed)
        .unwrap_or_else(rand::random);
    info!(
        "run starting: seed {seed}{}",
        if p.resume.is_some() { " (resumed)" } else { "" }
    );
    commands.queue(move |world: &mut World| {
        start_run(world, seed, DEFAULT_STRAIN, p.resume);
    });
}

#[derive(Component)]
struct TitleCard {
    age: f32,
}

fn build_tier_world(
    mut commands: Commands,
    mut entered: MessageReader<TierEntered>,
    balance: Res<Balance>,
    arena: Option<Res<Arena>>,
    theme: Res<Theme>,
    locale: Res<Locale>,
    mut active: ResMut<ActiveScene>,
    members: Query<Entity, With<SceneMember>>,
    cards: Query<Entity, With<TitleCard>>,
    camera: Query<Entity, With<WorldCamera>>,
    mut spawner: WorldSpawner,
) {
    let Some(ev) = entered.read().last().cloned() else {
        return;
    };
    let (Some(arena), Ok(cam)) = (arena, camera.single()) else {
        return;
    };
    let Some(tier) = balance.tier(ev.tier_index) else {
        return;
    };
    for e in &members {
        commands.entity(e).despawn();
    }
    spawner.build(&WorldSpec {
        biome: tier.biome,
        terrain: arena.terrain.clone(),
        radius: arena.radius,
        camera: cam,
        shot: None,
        stand_in_organism: false,
    });
    active.name = Some(format!("run-tier{}", tier.tier));

    for e in &cards {
        commands.entity(e).despawn();
    }
    let heading = locale.get_args(
        "stratum-heading",
        &[(
            "tier",
            game_core::locale::FluentValue::from(tier.tier as u32),
        )],
    );
    commands.spawn((
        TitleCard { age: 0.0 },
        DespawnOnExit(AppState::InRun),
        Node {
            position_type: PositionType::Absolute,
            top: percent(16),
            width: percent(100),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: px(6),
            ..default()
        },
        children![
            (
                Text::new(heading),
                theme.font(18.0),
                TextColor(theme.text_dim)
            ),
            (
                Text::new(locale.get(&tier.stratum_key)),
                theme.font(46.0),
                TextColor(theme.accent),
            ),
            (
                Text::new(locale.get(&tier.biome_key)),
                theme.font(20.0),
                TextColor(theme.text)
            ),
        ],
    ));
}

fn fade_title_card(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut cards: Query<(Entity, &mut TitleCard, &Children)>,
    mut texts: Query<&mut TextColor>,
) {
    for (e, mut card, children) in &mut cards {
        card.age += time.delta_secs();
        if card.age > TITLE_SECS {
            commands.entity(e).despawn();
            continue;
        }
        let alpha = ((TITLE_SECS - card.age) / 1.2).clamp(0.0, 1.0) * (card.age / 0.4).min(1.0);
        for c in children.iter() {
            if let Ok(mut color) = texts.get_mut(c) {
                color.0 = color.0.with_alpha(alpha);
            }
        }
    }
}

#[derive(Component)]
struct TransitionOverlay {
    t: f32,
    swapped: bool,
}

fn start_transition(mut commands: Commands) {
    commands.spawn((
        Name::new("TierTransition"),
        TransitionOverlay {
            t: 0.0,
            swapped: false,
        },
        DespawnOnExit(RunState::TierTransition),
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            ..default()
        },
        BackgroundColor(Color::BLACK.with_alpha(0.0)),
        GlobalZIndex(50),
    ));
}

/// Zoom-out cinematic: the camera pulls far back while the screen fades;
/// the next tier is built behind the fade; Confirm/Back skips.
fn run_transition(
    mut commands: Commands,
    time: Res<Time<Real>>,
    actions: Query<&ActionState<GameAction>, With<PlayerInput>>,
    mut overlay: Query<(&mut TransitionOverlay, &mut BackgroundColor)>,
    mut cams: Query<&mut FollowCamera>,
    mut next: ResMut<NextState<RunState>>,
) {
    let Ok((mut o, mut bg)) = overlay.single_mut() else {
        return;
    };
    let skip = actions
        .single()
        .is_ok_and(|a| a.just_pressed(&GameAction::Confirm) || a.just_pressed(&GameAction::Back));
    o.t += time.delta_secs() / TRANSITION_SECS;
    if skip {
        o.t = 1.0;
    }
    let t = o.t.min(1.0);
    // Zoom out (ease-in) until the swap, then settle back from far away.
    let zoom = if t < SWAP_AT {
        1.0 + 9.0 * (t / SWAP_AT).powi(2)
    } else {
        1.0 + 4.0 * (1.0 - (t - SWAP_AT) / (1.0 - SWAP_AT)).powi(2)
    };
    for mut cam in &mut cams {
        cam.zoom_out = zoom;
    }
    let fade = if t < SWAP_AT {
        ((t - 0.35) / (SWAP_AT - 0.35)).clamp(0.0, 1.0)
    } else {
        1.0 - ((t - SWAP_AT) / 0.3).clamp(0.0, 1.0)
    };
    bg.0 = Color::BLACK.with_alpha(fade);
    if (t >= SWAP_AT || skip) && !o.swapped {
        o.swapped = true;
        commands.queue(|world: &mut World| {
            if !advance_tier(world) {
                warn!("tier transition without a next tier");
            }
        });
    }
    if t >= 1.0 && o.swapped {
        for mut cam in &mut cams {
            cam.zoom_out = 1.0;
        }
        next.set(RunState::Playing);
    }
}

/// Checkpoint of the current run (for autosave).
pub fn checkpoint(info: &RunInfo, vitals: &game_core::rules::Vitals) -> RunCheckpoint {
    RunCheckpoint {
        seed: info.seed,
        strain: info.strain.clone(),
        tier_index: info.tier_index,
        vitals: vitals.clone(),
        elapsed: info.elapsed,
        peak_biomass: info.peak_biomass,
        meals: info.meals,
    }
}
