//! Reports render-pipeline compilation progress from the render world to the
//! main world. Screenshot, benchmark and `--warmup` wait on it so they never
//! capture a frame with missing materials or time shader compilation.

use bevy::prelude::*;
use bevy::render::render_resource::{CachedPipelineState, PipelineCache};
use bevy::render::{Render, RenderApp, RenderSystems};
use bevy::shader::ShaderCacheError;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

#[derive(Default, Debug)]
struct Counters {
    total: AtomicU32,
    ready: AtomicU32,
    pending: AtomicU32,
    failed: AtomicU32,
}

/// Snapshot of pipeline compilation, readable from the main world.
#[derive(Resource, Clone, Default, Debug)]
pub struct PipelineStatus(Arc<Counters>);

impl PipelineStatus {
    pub fn total(&self) -> u32 {
        self.0.total.load(Ordering::Relaxed)
    }
    pub fn ready(&self) -> u32 {
        self.0.ready.load(Ordering::Relaxed)
    }
    pub fn pending(&self) -> u32 {
        self.0.pending.load(Ordering::Relaxed)
    }
    pub fn failed(&self) -> u32 {
        self.0.failed.load(Ordering::Relaxed)
    }
}

pub struct PipelineStatusPlugin;

impl Plugin for PipelineStatusPlugin {
    fn build(&self, app: &mut App) {
        let status = PipelineStatus::default();
        app.insert_resource(status.clone());
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app
                .insert_resource(status)
                .add_systems(Render, count_pipelines.in_set(RenderSystems::Cleanup));
        }
    }
}

fn count_pipelines(cache: Res<PipelineCache>, status: Res<PipelineStatus>) {
    let (mut total, mut ready, mut pending, mut failed) = (0, 0, 0, 0);
    for p in cache.pipelines() {
        total += 1;
        match p.state {
            CachedPipelineState::Ok(_) => ready += 1,
            CachedPipelineState::Queued | CachedPipelineState::Creating(_) => pending += 1,
            // Waiting on a shader asset or import: Bevy retries these once it
            // loads, so they are still pending, not failed.
            CachedPipelineState::Err(
                ShaderCacheError::ShaderNotLoaded(_)
                | ShaderCacheError::ShaderImportNotYetAvailable,
            ) => pending += 1,
            CachedPipelineState::Err(_) => failed += 1,
        }
    }
    let c = &status.0;
    c.total.store(total, Ordering::Relaxed);
    c.ready.store(ready, Ordering::Relaxed);
    c.pending.store(pending, Ordering::Relaxed);
    c.failed.store(failed, Ordering::Relaxed);
}
