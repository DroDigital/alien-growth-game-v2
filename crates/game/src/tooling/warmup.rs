//! `--warmup`: builds render pipelines offscreen before first play by
//! loading every scene under every quality preset (each preset toggles
//! different post-process pipelines). M0 compiles and reports; persisting a
//! wgpu pipeline cache to disk is part of launcher v1 (M2).

use super::PipelineSettle;
use crate::boot::{ActiveRenderSettings, Presets};
use crate::graphics::pipelines::PipelineStatus;
use crate::scenes::{ActiveScene, LoadScene};
use bevy::prelude::*;
use game_core::config::{QualityPreset, QualitySettings};
use game_core::states::AppState;

/// Scenes compiled by warm-up. The menu shares the testbed's materials.
const WARMUP_SCENES: &[&str] = &["testbed"];

pub struct WarmupModePlugin;

impl Plugin for WarmupModePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WarmupProgress>()
            .add_systems(Update, step.run_if(in_state(AppState::Tooling)));
    }
}

#[derive(Resource, Default)]
struct WarmupProgress {
    step: usize,
    started: bool,
}

fn steps() -> Vec<(&'static str, QualityPreset)> {
    WARMUP_SCENES
        .iter()
        .flat_map(|s| QualityPreset::ALL.iter().map(move |p| (*s, *p)))
        .collect()
}

fn step(
    mut progress: ResMut<WarmupProgress>,
    mut settle: ResMut<PipelineSettle>,
    status: Res<PipelineStatus>,
    presets: Res<Presets>,
    mut active_settings: ResMut<ActiveRenderSettings>,
    mut active_scene: ResMut<ActiveScene>,
    mut load: MessageWriter<LoadScene>,
    mut exit: MessageWriter<AppExit>,
) {
    let plan = steps();
    if !progress.started || settle.stable_frames >= 10 {
        if progress.started {
            info!(
                "warm-up {}/{}: {} pipelines ready, {} failed",
                progress.step + 1,
                plan.len(),
                status.ready(),
                status.failed()
            );
            progress.step += 1;
        }
        progress.started = true;
        let Some((scene, preset)) = plan.get(progress.step).copied() else {
            println!(
                "WARMUP done: {} pipelines compiled, {} failed",
                status.ready(),
                status.failed()
            );
            exit.write(if status.failed() > 0 {
                AppExit::from_code(1)
            } else {
                AppExit::Success
            });
            return;
        };
        active_settings.set_if_neq(ActiveRenderSettings(presets.resolve(&QualitySettings {
            preset,
            ..default()
        })));
        // Force a reload so scene content picks up the new preset.
        active_scene.name = None;
        load.write(LoadScene {
            name: scene.into(),
            animate_camera: false,
        });
        settle.reset();
    }
}
