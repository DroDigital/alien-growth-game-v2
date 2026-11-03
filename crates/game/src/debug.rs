//! Debug tools (only with the `debug-ui` feature, i.e. dev builds):
//! - F1: world inspector (bevy-inspector-egui)
//! - F2: live balance panel (edits the `Balance` resource the simulation
//!   reads every tick; Reload re-reads `data/balance/*.ron`)
//! - F3: performance overlay (fps, frame-time graph, 1% low, entities, memory)

use crate::boot::LaunchContext;
use bevy::diagnostic::{
    DiagnosticPath, DiagnosticsStore, EntityCountDiagnosticsPlugin, FrameTimeDiagnosticsPlugin,
};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPlugin, EguiPrimaryContextPass, egui};
use bevy_inspector_egui::quick::WorldInspectorPlugin;
use game_core::balance::Balance;
use game_core::perf::FrameStats;
use game_core::sim::RunInfo;
use std::collections::VecDeque;

pub struct DebugPlugin;

#[derive(Resource, Default)]
struct DebugToggles {
    inspector: bool,
    balance: bool,
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
            balance: false,
            perf: true,
        })
        .init_resource::<FrameHistory>()
        .init_resource::<MemorySample>()
        .add_systems(Update, (toggle_debug, record_frame_time, sample_memory))
        .init_resource::<BalanceStatus>()
        .add_systems(EguiPrimaryContextPass, (perf_overlay, balance_panel));
    }
}

fn toggle_debug(keys: Res<ButtonInput<KeyCode>>, mut t: ResMut<DebugToggles>) {
    if keys.just_pressed(KeyCode::F1) {
        t.inspector = !t.inspector;
    }
    if keys.just_pressed(KeyCode::F2) {
        t.balance = !t.balance;
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

/// Last reload result shown in the balance panel.
#[derive(Resource, Default)]
struct BalanceStatus(String);

fn slider(ui: &mut egui::Ui, label: &str, v: &mut f32, range: std::ops::RangeInclusive<f32>) {
    ui.add(egui::Slider::new(v, range).text(label));
}

fn balance_panel(
    mut contexts: EguiContexts,
    toggles: Res<DebugToggles>,
    mut balance: ResMut<Balance>,
    mut status: ResMut<BalanceStatus>,
    info: Option<Res<RunInfo>>,
) -> Result {
    if !toggles.balance {
        return Ok(());
    }
    let ctx = contexts.ctx_mut()?;
    let tier_index = info.map_or(0, |i| i.tier_index);
    egui::Window::new("Balance (F2)")
        .default_pos(egui::pos2(12.0, 200.0))
        .default_width(340.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Reload data/balance").clicked() {
                    let dir = game_core::paths::data_dir().join("balance");
                    status.0 = match Balance::load(&dir) {
                        Ok(b) => {
                            *balance = b;
                            "reloaded".into()
                        }
                        Err(e) => format!("reload failed: {e}"),
                    };
                }
                if ui.button("Built-in").clicked() {
                    *balance = Balance::builtin();
                    status.0 = "reset to built-in".into();
                }
            });
            if !status.0.is_empty() {
                ui.small(&status.0);
            }
            ui.small("Live edits are not saved; copy values into data/balance/*.ron.");
            egui::ScrollArea::vertical()
                .max_height(520.0)
                .show(ui, |ui| {
                    let b = &mut *balance;
                    if let Some(tier) = b.strata.tiers.get_mut(tier_index) {
                        ui.collapsing(format!("Tier {} ({})", tier.tier, tier.stratum_key), |ui| {
                            slider(ui, "upkeep k", &mut tier.upkeep_k, 0.0..=2.0);
                            slider(
                                ui,
                                "complete biomass",
                                &mut tier.complete_biomass,
                                tier.start_biomass..=tier.start_biomass * 100.0,
                            );
                        });
                    }
                    ui.collapsing("Metabolism", |ui| {
                        let m = &mut b.core.metabolism;
                        slider(ui, "Kleiber exponent", &mut m.kleiber_exponent, 0.5..=1.0);
                        slider(ui, "digestion rate", &mut m.digestion_rate, 0.01..=0.5);
                        slider(
                            ui,
                            "energy / mass",
                            &mut m.energy_yield_per_mass,
                            1.0..=40.0,
                        );
                        slider(ui, "growth fraction", &mut m.growth_fraction, 0.0..=1.0);
                        slider(
                            ui,
                            "stomach / biomass",
                            &mut m.stomach_capacity_per_biomass,
                            0.1..=2.0,
                        );
                        slider(ui, "gorged threshold", &mut m.gorged_threshold, 1.0..=2.0);
                        slider(
                            ui,
                            "starvation dmg/s",
                            &mut m.starvation_damage_per_sec,
                            0.0..=30.0,
                        );
                        slider(ui, "regen/s", &mut m.integrity_regen_per_sec, 0.0..=10.0);
                    });
                    ui.collapsing("Size gate", |ui| {
                        let g = &mut b.core.size_gate;
                        slider(ui, "edible below", &mut g.safe_ratio, 0.3..=1.0);
                        slider(ui, "struggle below", &mut g.struggle_max_ratio, 1.0..=2.0);
                    });
                    ui.collapsing("Struggle", |ui| {
                        let st = &mut b.core.struggle;
                        slider(ui, "start progress", &mut st.start_progress, 0.0..=1.0);
                        slider(ui, "per press", &mut st.press_gain, 0.0..=0.3);
                        slider(ui, "resist/s", &mut st.resist_per_sec, 0.0..=2.0);
                        slider(ui, "ratio power", &mut st.resist_ratio_power, 0.0..=5.0);
                        slider(ui, "fail damage", &mut st.fail_damage, 0.0..=50.0);
                    });
                    ui.collapsing("Combat", |ui| {
                        let c = &mut b.core.combat;
                        slider(ui, "bite damage", &mut c.bite_damage, 0.0..=60.0);
                        slider(ui, "bite cooldown", &mut c.bite_cooldown_secs, 0.1..=5.0);
                        slider(ui, "hunt min ratio", &mut c.hunt_min_ratio, 0.0..=1.0);
                        slider(ui, "hunt stamina/s", &mut c.hunt_stamina_per_sec, 0.0..=1.0);
                    });
                    ui.collapsing("Locomotion", |ui| {
                        let l = &mut b.core.locomotion;
                        slider(ui, "base speed", &mut l.base_speed, 0.5..=10.0);
                        slider(ui, "speed exponent", &mut l.speed_exponent, 0.0..=1.5);
                    });
                });
            if let Err(e) = balance.validate() {
                ui.colored_label(egui::Color32::RED, e.to_string());
            }
        });
    Ok(())
}
