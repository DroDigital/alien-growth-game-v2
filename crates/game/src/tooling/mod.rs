//! Non-interactive modes: `--screenshot`, `--benchmark`, `--warmup`.
//! All of them wait for render pipelines to finish compiling first.

mod benchmark;
mod screenshot;
mod warmup;

use crate::boot::LaunchContext;
use crate::cli::Mode;
use crate::graphics::pipelines::PipelineStatus;
use bevy::prelude::*;

pub struct ToolingPlugin;

impl Plugin for ToolingPlugin {
    fn build(&self, app: &mut App) {
        let mode = app.world().resource::<LaunchContext>().mode.clone();
        app.init_resource::<PipelineSettle>()
            .add_systems(Update, track_pipeline_settle);
        match mode {
            Mode::Screenshot { .. } => {
                app.add_plugins(screenshot::ScreenshotModePlugin);
            }
            Mode::Benchmark { .. } => {
                app.add_plugins(benchmark::BenchmarkModePlugin);
            }
            Mode::Warmup => {
                app.add_plugins(warmup::WarmupModePlugin);
            }
            Mode::Play => {}
        }
        // Tooling modes must never hang: hard timeout. The benchmark only
        // starts its clock after warm-up, so it gets its duration on top.
        if !matches!(mode, Mode::Play) {
            let limit = watchdog_limit_secs(&mode);
            app.insert_resource(WatchdogLimit(limit))
                .add_systems(Update, watchdog);
        }
    }
}

/// A checkpoint that arrives in Tier 2 as if Tier 1 had just been completed
/// (run screenshots and the run benchmark start here).
pub fn tier2_arrival(
    balance: &game_core::balance::Balance,
    seed: u64,
) -> Option<game_core::sim::RunCheckpoint> {
    let tier = balance.tier(1)?;
    let mut vitals = game_core::rules::Vitals::new(tier.start_biomass, &balance.core.metabolism);
    vitals.energy = 80.0;
    Some(game_core::sim::RunCheckpoint {
        seed,
        strain: game_core::sim::DEFAULT_STRAIN.into(),
        tier_index: 1,
        vitals,
        elapsed: 300.0,
        peak_biomass: tier.start_biomass,
        meals: 60,
    })
}

/// Resident memory of this process in MiB (None if the OS query fails).
pub fn process_memory_mib() -> Option<f64> {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};
    let pid = sysinfo::get_current_pid().ok()?;
    let mut sys = System::new().ok()?;
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::nothing().with_memory(),
    );
    sys.process(pid)
        .map(|p| p.memory() as f64 / (1024.0 * 1024.0))
}

/// Counts consecutive frames with no pipeline compiling and a stable total.
#[derive(Resource, Debug, Default)]
pub struct PipelineSettle {
    pub stable_frames: u32,
    last_total: u32,
}

impl PipelineSettle {
    pub fn reset(&mut self) {
        self.stable_frames = 0;
    }
}

fn track_pipeline_settle(status: Res<PipelineStatus>, mut settle: ResMut<PipelineSettle>) {
    let total = status.total();
    if status.pending() == 0 && total == settle.last_total && total > 0 {
        settle.stable_frames += 1;
    } else {
        settle.stable_frames = 0;
    }
    settle.last_total = total;
}

/// Seconds of startup and shader warm-up a tooling mode may take before it
/// gives up (software rasterizers are slow).
const WATCHDOG_SECS: f64 = 600.0;

#[derive(Resource, Debug, Clone, Copy)]
struct WatchdogLimit(f64);

/// Total time allowed for a tooling mode: the warm-up allowance plus, for the
/// benchmark, the requested measurement duration.
pub fn watchdog_limit_secs(mode: &Mode) -> f64 {
    match mode {
        Mode::Benchmark { secs, .. } => WATCHDOG_SECS + secs,
        _ => WATCHDOG_SECS,
    }
}

fn watchdog(time: Res<Time<Real>>, limit: Res<WatchdogLimit>, mut exit: MessageWriter<AppExit>) {
    if time.elapsed_secs_f64() > limit.0 {
        error!("tooling mode timed out after {:.0}s", limit.0);
        exit.write(AppExit::from_code(3));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn benchmark_watchdog_includes_measurement_time() {
        let bench = Mode::Benchmark {
            secs: 600.0,
            out: None,
        };
        assert!(watchdog_limit_secs(&bench) > 600.0 + WATCHDOG_SECS - 1.0);
        assert_eq!(watchdog_limit_secs(&Mode::Warmup), WATCHDOG_SECS);
    }
}
