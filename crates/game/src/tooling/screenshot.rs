//! `--screenshot <scene> <preset> <out.png>`: renders the scene offscreen at
//! the preset's render scale, upscales to the output resolution exactly like
//! the windowed path, waits for shaders and TAA to settle, saves, exits.
//!
//! Run scenes (`run-tier1`, `run-tier2`, `death`) start a real run driven by
//! the balance bot at an accelerated fixed clock, then freeze the world and
//! capture once the scene's condition is met.

use super::PipelineSettle;
use crate::boot::LaunchContext;
use crate::cli::Mode;
use crate::creature::AutoPilot;
use crate::graphics::RenderTargets;
use crate::graphics::pipelines::PipelineStatus;
use crate::scenes::{LoadScene, SCENES, is_known_scene};
use crate::strata::PendingRun;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use bevy::time::TimeUpdateStrategy;
use game_core::balance::Balance;
use game_core::bots::BotPlugin;
use game_core::sim::RunInfo;
use game_core::states::{AppState, RunState};
use std::time::Duration;

/// Screenshot scenes that simulate a run.
pub const RUN_SCENES: &[&str] = &["run-tier1", "run-tier2", "death"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RunShot {
    /// Mid-way through tier 1.
    Tier1,
    /// Shortly after arriving in tier 2.
    Tier2,
    /// The death screen of a bot run.
    Death,
}

impl RunShot {
    fn parse(scene: &str) -> Option<Self> {
        match scene {
            "run-tier1" => Some(Self::Tier1),
            "run-tier2" => Some(Self::Tier2),
            "death" => Some(Self::Death),
            _ => None,
        }
    }

    /// Fixed seeds so every capture shows the same moment.
    fn seed(self) -> u64 {
        match self {
            Self::Tier1 => 8,
            Self::Tier2 => 8,
            Self::Death => 6,
        }
    }
}

/// Simulated seconds advanced per rendered frame (16 fixed ticks; the most
/// `Time<Virtual>` accepts per frame). Software rasterizers render about one
/// frame per second, so this keeps run shots within the tooling watchdog.
const SHOT_STEP_SECS: f32 = 0.25;

/// Simulated seconds after which a death shot gives up waiting.
const DEATH_SHOT_TIMEOUT: f32 = 900.0;

/// The run shot being prepared, and whether its world is frozen yet.
#[derive(Resource, Debug)]
struct RunShotState {
    shot: RunShot,
    frozen: bool,
}

pub struct ScreenshotModePlugin;

impl Plugin for ScreenshotModePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ShotProgress>()
            .add_systems(OnEnter(AppState::Tooling), start)
            .add_systems(
                Update,
                (advance_run_shot, capture_when_ready)
                    .chain()
                    .run_if(in_state(AppState::Tooling).or_else(in_state(AppState::InRun))),
            );
        let ctx = app.world().resource::<LaunchContext>();
        if let Mode::Screenshot { scene, .. } = &ctx.mode
            && let Some(shot) = RunShot::parse(scene)
        {
            app.add_plugins(BotPlugin {
                auto_advance: false,
            })
            .insert_resource(AutoPilot)
            .insert_resource(RunShotState {
                shot,
                frozen: false,
            })
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
                SHOT_STEP_SECS,
            )));
        }
    }
}

#[derive(Resource, Default)]
struct ShotProgress {
    frames: u32,
    requested: bool,
}

/// Consecutive frames without pipeline compilation before capturing.
const STABLE_FRAMES: u32 = 10;

fn start(
    ctx: Res<LaunchContext>,
    balance: Res<Balance>,
    run_shot: Option<Res<RunShotState>>,
    mut pending: ResMut<PendingRun>,
    mut next: ResMut<NextState<AppState>>,
    mut load: MessageWriter<LoadScene>,
    mut exit: MessageWriter<AppExit>,
) {
    let Mode::Screenshot { scene, .. } = &ctx.mode else {
        return;
    };
    if let Some(rs) = run_shot {
        let seed = rs.shot.seed();
        pending.seed = Some(seed);
        if rs.shot == RunShot::Tier2 {
            pending.resume = super::tier2_arrival(&balance, seed);
        }
        next.set(AppState::InRun);
        return;
    }
    if !is_known_scene(scene) {
        error!("unknown scene '{scene}'; known scenes: {SCENES:?} and {RUN_SCENES:?}");
        exit.write(AppExit::from_code(2));
        return;
    }
    load.write(LoadScene {
        name: scene.clone(),
        animate_camera: false,
    });
}

/// Watches a run shot until its moment arrives, then freezes the world
/// (virtual time at speed 0) so TAA and the frame counters can settle.
fn advance_run_shot(
    state: Option<ResMut<RunShotState>>,
    info: Option<Res<RunInfo>>,
    run_state: Option<Res<State<RunState>>>,
    mut virt: ResMut<Time<Virtual>>,
    mut progress: ResMut<ShotProgress>,
    mut settle: ResMut<PipelineSettle>,
    mut exit: MessageWriter<AppExit>,
    mut last_log: Local<f32>,
) {
    let Some(mut state) = state else { return };
    if state.frozen {
        return;
    }
    let (Some(info), Some(run_state)) = (info, run_state) else {
        return;
    };
    let ready = match state.shot {
        RunShot::Tier1 => info.tier_index == 0 && info.tier_elapsed >= 45.0,
        RunShot::Tier2 => info.tier_index == 1 && info.tier_elapsed >= 10.0,
        RunShot::Death => *run_state.get() == RunState::Dead,
    };
    if !ready {
        let early_death = state.shot != RunShot::Death
            && matches!(run_state.get(), RunState::Dead | RunState::Victory);
        if early_death || info.elapsed > DEATH_SHOT_TIMEOUT {
            error!(
                "run shot {:?} (seed {}) never reached its moment; outcome {:?}",
                state.shot, info.seed, info.outcome
            );
            exit.write(AppExit::from_code(5));
        }
        if info.elapsed - *last_log >= 30.0 {
            *last_log = info.elapsed;
            info!(
                "run shot {:?}: sim {:.0}s, tier {}, {} meals",
                state.shot,
                info.elapsed,
                info.tier_index + 1,
                info.meals
            );
        }
        // Do not let the capture begin before the moment arrives.
        progress.frames = 0;
        return;
    }
    info!(
        "run shot {:?} ready at sim {:.0}s (tier {}, {} meals)",
        state.shot,
        info.elapsed,
        info.tier_index + 1,
        info.meals
    );
    virt.set_relative_speed(0.0);
    state.frozen = true;
    progress.frames = 0;
    settle.reset();
}

fn capture_when_ready(
    mut commands: Commands,
    ctx: Res<LaunchContext>,
    targets: Res<RenderTargets>,
    status: Res<PipelineStatus>,
    settle: Res<PipelineSettle>,
    run_shot: Option<Res<RunShotState>>,
    mut progress: ResMut<ShotProgress>,
) {
    let Mode::Screenshot { out, preset, scene } = &ctx.mode else {
        return;
    };
    if run_shot.is_some_and(|s| !s.frozen) {
        return;
    }
    progress.frames += 1;
    if progress.requested
        || progress.frames < ctx.cli.settle_frames
        || settle.stable_frames < STABLE_FRAMES
    {
        return;
    }
    let Some(output) = targets.output.clone() else {
        error!("screenshot mode has no offscreen output target");
        return;
    };
    if let Some(dir) = out.parent().filter(|d| !d.as_os_str().is_empty()) {
        let _ = std::fs::create_dir_all(dir);
    }
    info!(
        "capturing {scene} @ {preset:?}: {}x{} output, {}x{} scene, {} pipelines ({} failed), frame {}",
        targets.output_size.x,
        targets.output_size.y,
        targets.scene_size.x,
        targets.scene_size.y,
        status.total(),
        status.failed(),
        progress.frames
    );
    let failed = status.failed();
    let path = out.clone();
    commands.spawn(Screenshot::image(output)).observe(
        move |captured: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
            // The capture event only means GPU readback finished; the exit
            // code must also reflect whether the file was written.
            if let Err(e) = save_png(&captured.image, &path) {
                error!("cannot save screenshot to {}: {e}", path.display());
                exit.write(AppExit::from_code(4));
                return;
            }
            info!("screenshot saved to {}", path.display());
            if failed > 0 {
                error!("{failed} render pipelines failed to compile");
                exit.write(AppExit::from_code(1));
            } else {
                exit.write(AppExit::Success);
            }
        },
    );
    progress.requested = true;
}

/// Encodes the captured (8-bit sRGB) frame as opaque RGB and writes it.
fn save_png(image: &Image, path: &std::path::Path) -> Result<(), String> {
    let dynamic = image
        .clone()
        .try_into_dynamic()
        .map_err(|e| e.to_string())?;
    dynamic.to_rgb8().save(path).map_err(|e| e.to_string())
}
