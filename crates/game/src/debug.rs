//! Debug tools (only with the `debug-ui` feature, i.e. dev builds):
//! - F1: world inspector (bevy-inspector-egui)
//! - F3: performance overlay (fps, frame-time graph, 1% low, entities, memory)
//!
//! The live balance panel and console arrive with the systems they tune (M1+).

use crate::boot::LaunchContext;
use bevy::diagnostic::{
    DiagnosticPath, DiagnosticsStore, EntityCountDiagnosticsPlugin, FrameTimeDiagnosticsPlugin,
};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPlugin, EguiPrimaryContextPass, egui};
use bevy_inspector_egui::quick::WorldInspectorPlugin;
use game_core::perf::FrameStats;
use std::collections::VecDeque;

pub struct DebugPlugin;

#[derive(Resource, Default)]
struct DebugToggles {
    inspector: bool,
    perf: bool,
}

/// Process memory, sampled twice per second (MiB).
#[derive(Resource, Default)]
struct MemorySample(Option<f64>, f32);

fn sample_memory(time: Res<Time<Real>>, mut m: ResMut<MemorySample>) {
    m.1 -= time.delta_secs();
    if m.1 <= 0.0 {
        m.0 = crate::tooling::process_memory_mib();
        m.1 = 0.5;
    }
}

/// Rolling window of recent frame times (ms).
#[derive(Resource, Default)]
struct FrameHistory(VecDeque<f64>);

const HISTORY: usize = 240;

impl Plugin for DebugPlugin {
    fn build(&self, app: &mut App) {
        if app.world().resource::<LaunchContext>().mode.is_headless() {
            return;
        }
        app.add_plugins((
            EguiPlugin::default(),
            WorldInspectorPlugin::new().run_if(|t: Res<DebugToggles>| t.inspector),
            FrameTimeDiagnosticsPlugin::default(),
            EntityCountDiagnosticsPlugin::default(),
        ));
        app.insert_resource(DebugToggles {
            inspector: false,
            perf: true,
        })
        .init_resource::<FrameHistory>()
        .init_resource::<MemorySample>()
        .add_systems(Update, (toggle_debug, record_frame_time, sample_memory))
        .add_systems(EguiPrimaryContextPass, perf_overlay);
    }
}

fn toggle_debug(keys: Res<ButtonInput<KeyCode>>, mut t: ResMut<DebugToggles>) {
    if keys.just_pressed(KeyCode::F1) {
        t.inspector = !t.inspector;
    }
    if keys.just_pressed(KeyCode::F3) {
        t.perf = !t.perf;
    }
}

fn record_frame_time(time: Res<Time<Real>>, mut h: ResMut<FrameHistory>) {
    h.0.push_back(time.delta_secs_f64() * 1000.0);
    while h.0.len() > HISTORY {
        h.0.pop_front();
    }
}

fn perf_overlay(
    mut contexts: EguiContexts,
    toggles: Res<DebugToggles>,
    history: Res<FrameHistory>,
    memory: Res<MemorySample>,
    diagnostics: Res<DiagnosticsStore>,
) -> Result {
    if !toggles.perf {
        return Ok(());
    }
    let ctx = contexts.ctx_mut()?;
    let times: Vec<f64> = history.0.iter().copied().collect();
    let stats = FrameStats::from_frame_times_ms(&times);
    let value = |path: &DiagnosticPath| -> Option<f64> { diagnostics.get(path)?.smoothed() };
    egui::Window::new("Performance (F3)")
        .default_pos(egui::pos2(12.0, 12.0))
        .resizable(false)
        .show(ctx, |ui| {
            if let Some(s) = stats {
                ui.monospace(format!(
                    "avg {:6.1} fps   1% low {:6.1} fps\nframe {:5.2} ms   max {:5.1} ms",
                    s.avg_fps, s.low_1pct_fps, s.avg_frame_ms, s.max_frame_ms
                ));
            }
            if let Some(n) = value(&EntityCountDiagnosticsPlugin::ENTITY_COUNT) {
                ui.monospace(format!("entities {n:.0}"));
            }
            if let Some(mib) = memory.0 {
                ui.monospace(format!("memory {mib:.0} MiB"));
            }
            // Frame-time graph: 33 ms tall, red above the 50 ms spike line.
            let (rect, _) =
                ui.allocate_exact_size(egui::vec2(HISTORY as f32, 60.0), egui::Sense::hover());
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 2.0, egui::Color32::from_black_alpha(160));
            let scale = rect.height() / 50.0;
            for (i, t) in times.iter().enumerate() {
                let x = rect.left() + i as f32;
                let h = (*t as f32 * scale).min(rect.height());
                let color = if *t > 50.0 {
                    egui::Color32::RED
                } else if *t > 16.7 {
                    egui::Color32::YELLOW
                } else {
                    egui::Color32::from_rgb(90, 220, 180)
                };
                painter.line_segment(
                    [
                        egui::pos2(x, rect.bottom()),
                        egui::pos2(x, rect.bottom() - h),
                    ],
                    egui::Stroke::new(1.0, color),
                );
            }
            let y60 = rect.bottom() - 16.7 * scale;
            painter.line_segment(
                [egui::pos2(rect.left(), y60), egui::pos2(rect.right(), y60)],
                egui::Stroke::new(1.0, egui::Color32::from_white_alpha(60)),
            );
        });
    Ok(())
}
