//! Non-interactive modes: `--screenshot`, `--benchmark`, `--warmup`.
//! All of them wait for render pipelines to finish compiling first.

mod benchmark;
mod screenshot;
mod warmup;

use crate::boot::LaunchContext;
use crate::cli::Mode;
use crate::graphics::pipelines::PipelineStatus;
use bevy::prelude::*;
use game_core::states::AppState;

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
        // Tooling modes must never hang: hard timeout.
        if !matches!(mode, Mode::Play) {
            app.add_systems(Update, watchdog.run_if(in_state(AppState::Tooling)));
        }
    }
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

/// Seconds before a tooling mode gives up (software rasterizers are slow).
const WATCHDOG_SECS: f32 = 600.0;

fn watchdog(time: Res<Time<Real>>, mut exit: MessageWriter<AppExit>) {
    if time.elapsed_secs() > WATCHDOG_SECS {
        error!("tooling mode timed out after {WATCHDOG_SECS}s");
        exit.write(AppExit::from_code(3));
    }
}
