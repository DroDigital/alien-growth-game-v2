//! `--screenshot <scene> <preset> <out.png>`: renders the scene offscreen at
//! the preset's render scale, upscales to the output resolution exactly like
//! the windowed path, waits for shaders and TAA to settle, saves, exits.

use super::PipelineSettle;
use crate::boot::LaunchContext;
use crate::cli::Mode;
use crate::graphics::RenderTargets;
use crate::graphics::pipelines::PipelineStatus;
use crate::scenes::{LoadScene, SCENES, is_known_scene};
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use game_core::states::AppState;

pub struct ScreenshotModePlugin;

impl Plugin for ScreenshotModePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ShotProgress>()
            .add_systems(OnEnter(AppState::Tooling), start)
            .add_systems(
                Update,
                capture_when_ready.run_if(in_state(AppState::Tooling)),
            );
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
    mut load: MessageWriter<LoadScene>,
    mut exit: MessageWriter<AppExit>,
) {
    let Mode::Screenshot { scene, .. } = &ctx.mode else {
        return;
    };
    if !is_known_scene(scene) {
        error!("unknown scene '{scene}'; known scenes: {SCENES:?}");
        exit.write(AppExit::from_code(2));
        return;
    }
    load.write(LoadScene {
        name: scene.clone(),
        animate_camera: false,
    });
}

fn capture_when_ready(
    mut commands: Commands,
    ctx: Res<LaunchContext>,
    targets: Res<RenderTargets>,
    status: Res<PipelineStatus>,
    settle: Res<PipelineSettle>,
    mut progress: ResMut<ShotProgress>,
) {
    let Mode::Screenshot { out, preset, scene } = &ctx.mode else {
        return;
    };
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
