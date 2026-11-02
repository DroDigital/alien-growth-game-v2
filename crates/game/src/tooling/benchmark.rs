//! `--benchmark [secs]`: scripted flythrough of the testbed in a real window
//! (vsync off), measuring frame times after shader warm-up. Writes a
//! `BenchmarkReport` JSON for the launcher and prints a summary.

use super::PipelineSettle;
use crate::boot::{LaunchContext, PlayerSettings};
use crate::cli::Mode;
use crate::graphics::RenderTargets;
use crate::scenes::{LoadScene, OrbitShot};
use bevy::prelude::*;
use bevy::render::renderer::RenderAdapterInfo;
use game_core::config::QualityPreset;
use game_core::perf::{BenchmarkReport, FrameStats};
use game_core::states::AppState;

pub struct BenchmarkModePlugin;

impl Plugin for BenchmarkModePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BenchState>()
            .add_systems(OnEnter(AppState::Tooling), start)
            .add_systems(
                Update,
                (speed_up_flythrough, record).run_if(in_state(AppState::Tooling)),
            );
    }
}

#[derive(Resource, Default)]
struct BenchState {
    warmup_frames: usize,
    measuring: bool,
    elapsed: f64,
    frame_times_ms: Vec<f64>,
    done: bool,
}

fn start(mut load: MessageWriter<LoadScene>) {
    load.write(LoadScene {
        name: "testbed".into(),
        animate_camera: true,
    });
}

/// The flythrough covers the whole crater: faster orbit with height changes.
fn speed_up_flythrough(mut q: Query<&mut OrbitShot, Added<OrbitShot>>) {
    for mut shot in &mut q {
        shot.speed = 0.35;
        shot.bob = 2.0;
        shot.radius = 14.0;
    }
}

fn record(
    time: Res<Time<Real>>,
    ctx: Res<LaunchContext>,
    settings: Res<PlayerSettings>,
    settle: Res<PipelineSettle>,
    targets: Res<RenderTargets>,
    adapter: Option<Res<RenderAdapterInfo>>,
    mut state: ResMut<BenchState>,
    mut exit: MessageWriter<AppExit>,
) {
    let Mode::Benchmark { secs, out } = &ctx.mode else {
        return;
    };
    if state.done {
        return;
    }
    if !state.measuring {
        state.warmup_frames += 1;
        // Start once shaders are compiled and the scene has rendered a while.
        if settle.stable_frames >= 30 {
            state.measuring = true;
            info!(
                "benchmark: warm-up done after {} frames",
                state.warmup_frames
            );
        }
        return;
    }
    let dt = time.delta_secs_f64();
    state.elapsed += dt;
    state.frame_times_ms.push(dt * 1000.0);
    if state.elapsed < *secs {
        return;
    }
    state.done = true;

    let Some(stats) = FrameStats::from_frame_times_ms(&state.frame_times_ms) else {
        exit.write(AppExit::from_code(1));
        return;
    };
    let memory_mib = super::process_memory_mib();
    let preset = settings.quality.preset;
    let report = BenchmarkReport {
        game_version: game_core::GAME_VERSION.into(),
        scene: "testbed".into(),
        preset: preset.as_str().into(),
        output_resolution: (targets.output_size.x, targets.output_size.y),
        render_resolution: (targets.scene_size.x, targets.scene_size.y),
        adapter: adapter.as_ref().map(|a| a.name.clone()).unwrap_or_default(),
        backend: adapter
            .as_ref()
            .map(|a| format!("{:?}", a.backend))
            .unwrap_or_default(),
        duration_secs: state.elapsed,
        warmup_frames: state.warmup_frames,
        stats,
        memory_mib,
        recommended_preset: recommend(preset, &stats).as_str().into(),
    };
    let path = out
        .clone()
        .unwrap_or_else(|| ctx.user_dirs.benchmark_file());
    match serde_json::to_string_pretty(&report) {
        Ok(json) => {
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if let Err(e) = std::fs::write(&path, json) {
                error!("cannot write benchmark report {}: {e}", path.display());
            }
        }
        Err(e) => error!("cannot serialize benchmark report: {e}"),
    }
    println!(
        "BENCHMARK {} {}x{} (render {}x{}) on {} [{}]: avg {:.1} fps, 1% low {:.1} fps, max frame {:.1} ms, spikes>50ms {}, mem {}",
        report.preset,
        report.output_resolution.0,
        report.output_resolution.1,
        report.render_resolution.0,
        report.render_resolution.1,
        report.adapter,
        report.backend,
        stats.avg_fps,
        stats.low_1pct_fps,
        stats.max_frame_ms,
        stats.spikes_over_50ms,
        memory_mib.map_or("n/a".into(), |m| format!("{m:.0} MiB")),
    );
    println!("report written to {}", path.display());
    exit.write(AppExit::Success);
}

/// Recommends a preset from measured performance at `current`.
pub fn recommend(current: QualityPreset, stats: &FrameStats) -> QualityPreset {
    let idx = QualityPreset::ALL
        .iter()
        .position(|p| *p == current)
        .unwrap_or(0);
    let new_idx = if stats.low_1pct_fps < 45.0 || stats.avg_fps < 55.0 {
        idx.saturating_sub(1)
    } else if stats.low_1pct_fps > 90.0 {
        (idx + 1).min(QualityPreset::ALL.len() - 1)
    } else {
        idx
    };
    QualityPreset::ALL[new_idx]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats(avg: f64, low: f64) -> FrameStats {
        FrameStats {
            frames: 100,
            avg_fps: avg,
            low_1pct_fps: low,
            avg_frame_ms: 1000.0 / avg,
            max_frame_ms: 1000.0 / low,
            spikes_over_50ms: 0,
        }
    }

    #[test]
    fn recommendation_moves_one_step() {
        use QualityPreset::*;
        assert_eq!(recommend(Medium, &stats(40.0, 30.0)), Low);
        assert_eq!(recommend(Low, &stats(40.0, 30.0)), Low);
        assert_eq!(recommend(Medium, &stats(70.0, 60.0)), Medium);
        assert_eq!(recommend(Medium, &stats(140.0, 110.0)), High);
        assert_eq!(recommend(Ultra, &stats(140.0, 110.0)), Ultra);
    }
}
