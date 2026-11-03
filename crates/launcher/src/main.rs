//! Native launcher (eframe/egui). M0 skeleton: launches the game with the
//! chosen preset and arguments, remembers launch options, shows a basic
//! system scan and detects abnormal exits. Later milestones fill in the
//! remaining tabs (see docs/STATUS.md).
//!
//! `launcher --headless-launch [GAME ARGS...]` launches the game without any
//! UI, waits, and returns its exit code (CI and smoke tests).

mod process;
mod system;

use eframe::egui;
use game_core::config::{AfterLaunch, GraphicsBackend, QualityPreset, Settings};
use game_core::launch::{FLAG_BACKEND, FLAG_PRESET, FLAG_SAFE_MODE, split_args};
use game_core::locale::{FluentValue, Localizer};
use game_core::paths::{self, UserDirs};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--headless-launch") {
        return headless_launch(&args[1..]);
    }
    let force_ui = args.iter().any(|a| a == "--show");

    let dirs = UserDirs::resolve();
    let _ = dirs.ensure();
    let settings = Settings::load(&dirs.settings_file()).unwrap_or_default();
    let locale = Localizer::load(&paths::assets_dir().join("locale"), &settings.language)
        .unwrap_or_else(|_| Localizer::from_sources("en-US", &[], &[]));

    // "Launch directly next time": skip the UI unless the last session crashed.
    let crashed_last_time = dirs.running_marker().exists();
    if settings.launcher.launch_directly
        && !force_ui
        && !crashed_last_time
        && let Some(exe) = process::find_game_executable()
        && process::launch(&exe, &game_args(&settings, false)).is_ok()
    {
        return ExitCode::SUCCESS;
    }

    let title = locale.get("launcher-title");
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([880.0, 560.0])
            .with_min_inner_size([640.0, 420.0])
            .with_title(&title),
        ..Default::default()
    };
    let app = LauncherApp::new(settings, dirs, locale, crashed_last_time);
    match eframe::run_native(&title, options, Box::new(|_cc| Ok(Box::new(app)))) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("launcher error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn headless_launch(game_args: &[String]) -> ExitCode {
    let Some(exe) = process::find_game_executable() else {
        eprintln!("game executable not found next to the launcher");
        return ExitCode::from(127);
    };
    println!("launching {} {}", exe.display(), game_args.join(" "));
    match process::launch(&exe, game_args) {
        Ok(mut game) => match game.child.wait() {
            Ok(status) => {
                println!("game exited with {}", process::exit_code(&status));
                ExitCode::from(status.code().unwrap_or(1).clamp(0, 255) as u8)
            }
            Err(e) => {
                eprintln!("wait failed: {e}");
                ExitCode::FAILURE
            }
        },
        Err(e) => {
            eprintln!("launch failed: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Arguments passed to the game for a normal or safe-mode launch.
fn game_args(settings: &Settings, safe_mode: bool) -> Vec<String> {
    let mut args = vec![
        FLAG_PRESET.to_string(),
        settings.quality.preset.as_str().to_string(),
    ];
    let backend = match settings.launcher.backend {
        GraphicsBackend::Auto => None,
        GraphicsBackend::Dx12 => Some("dx12"),
        GraphicsBackend::Vulkan => Some("vulkan"),
        GraphicsBackend::Metal => Some("metal"),
    };
    if let Some(b) = backend {
        args.extend([FLAG_BACKEND.to_string(), b.to_string()]);
    }
    if safe_mode {
        args.push(FLAG_SAFE_MODE.to_string());
    }
    args.extend(split_args(&settings.launcher.custom_args));
    args
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Play,
    System,
    Settings,
    Benchmark,
    Mods,
    Diagnostics,
}

impl Tab {
    const ALL: [Tab; 6] = [
        Tab::Play,
        Tab::System,
        Tab::Settings,
        Tab::Benchmark,
        Tab::Mods,
        Tab::Diagnostics,
    ];

    fn key(self) -> &'static str {
        match self {
            Tab::Play => "launcher-tab-play",
            Tab::System => "launcher-tab-system",
            Tab::Settings => "launcher-tab-settings",
            Tab::Benchmark => "launcher-tab-benchmark",
            Tab::Mods => "launcher-tab-mods",
            Tab::Diagnostics => "launcher-tab-diagnostics",
        }
    }
}

struct LauncherApp {
    settings: Settings,
    dirs: UserDirs,
    locale: Localizer,
    tab: Tab,
    game_exe: Option<PathBuf>,
    running: Option<process::RunningGame>,
    status: String,
    crashed_last_time: bool,
    system: Option<system::SystemInfo>,
}

impl LauncherApp {
    fn new(settings: Settings, dirs: UserDirs, locale: Localizer, crashed: bool) -> Self {
        let game_exe = process::find_game_executable();
        let status = match &game_exe {
            Some(_) if crashed => locale.get_args(
                "launcher-exited-abnormally",
                &[("code", FluentValue::from("?"))],
            ),
            Some(_) => String::new(),
            None => locale.get_args(
                "launcher-not-found",
                &[(
                    "path",
                    FluentValue::from(game_core::launch::game_executable_name()),
                )],
            ),
        };
        Self {
            settings,
            dirs,
            locale,
            tab: Tab::Play,
            game_exe,
            running: None,
            status,
            crashed_last_time: crashed,
            system: None,
        }
    }

    fn t(&self, id: &str) -> String {
        self.locale.get(id)
    }

    fn save_settings(&mut self) {
        match self.settings.save(&self.dirs.settings_file()) {
            Ok(()) => self.status = self.t("launcher-saved"),
            Err(e) => self.status = e.to_string(),
        }
    }

    fn launch(&mut self, ctx: &egui::Context, safe_mode: bool) {
        let Some(exe) = self.game_exe.clone() else {
            return;
        };
        self.save_settings();
        match process::launch(&exe, &game_args(&self.settings, safe_mode)) {
            Ok(game) => {
                self.status = self.locale.get_args(
                    "launcher-running",
                    &[("pid", FluentValue::from(game.pid()))],
                );
                self.running = Some(game);
                self.crashed_last_time = false;
                match self.settings.launcher.after_launch {
                    AfterLaunch::StayOpen => {}
                    AfterLaunch::Minimize => {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true))
                    }
                    AfterLaunch::Close => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                }
            }
            Err(e) => {
                self.status = self.locale.get_args(
                    "launcher-launch-failed",
                    &[("error", FluentValue::from(e.to_string()))],
                );
            }
        }
    }

    fn poll_game(&mut self, ctx: &egui::Context) {
        let Some(game) = self.running.as_mut() else {
            return;
        };
        match game.poll() {
            Ok(Some(status)) => {
                let code = process::exit_code(&status);
                let abnormal = !status.success() || self.dirs.running_marker().exists();
                let key = if abnormal {
                    "launcher-exited-abnormally"
                } else {
                    "launcher-exited"
                };
                self.status = self
                    .locale
                    .get_args(key, &[("code", FluentValue::from(code))]);
                self.crashed_last_time = abnormal;
                self.running = None;
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            }
            Ok(None) => ctx.request_repaint_after(Duration::from_millis(250)),
            Err(e) => {
                self.status = e.to_string();
                self.running = None;
            }
        }
    }

    fn play_tab(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        ui.add_space(12.0);
        let can_launch = self.game_exe.is_some() && self.running.is_none();
        let play = egui::Button::new(egui::RichText::new(self.t("launcher-play")).size(28.0))
            .min_size(egui::vec2(220.0, 64.0));
        let play_response = ui.add_enabled(can_launch, play);
        if play_response.clicked() {
            self.launch(&ctx, false);
        }
        // Controller/keyboard friendly: Play has focus by default.
        if !ui.memory(|m| m.focused().is_some()) {
            play_response.request_focus();
        }
        if self.crashed_last_time
            && ui
                .add_enabled(can_launch, egui::Button::new(self.t("launcher-safe-mode")))
                .clicked()
        {
            self.launch(&ctx, true);
        }
        ui.add_space(16.0);

        egui::Grid::new("launch-options")
            .num_columns(2)
            .spacing([16.0, 10.0])
            .show(ui, |ui| {
                ui.label(self.t("launcher-preset"));
                egui::ComboBox::from_id_salt("preset")
                    .selected_text(format!("{:?}", self.settings.quality.preset))
                    .show_ui(ui, |ui| {
                        for p in QualityPreset::ALL {
                            ui.selectable_value(
                                &mut self.settings.quality.preset,
                                p,
                                format!("{p:?}"),
                            );
                        }
                    });
                ui.end_row();

                ui.label(self.t("launcher-backend"));
                let backends: &[GraphicsBackend] = if cfg!(target_os = "windows") {
                    &[
                        GraphicsBackend::Auto,
                        GraphicsBackend::Dx12,
                        GraphicsBackend::Vulkan,
                    ]
                } else if cfg!(target_os = "macos") {
                    &[GraphicsBackend::Auto, GraphicsBackend::Metal]
                } else {
                    &[GraphicsBackend::Auto, GraphicsBackend::Vulkan]
                };
                let auto = self.t("launcher-backend-auto");
                let name = |b: GraphicsBackend| match b {
                    GraphicsBackend::Auto => auto.clone(),
                    other => format!("{other:?}"),
                };
                egui::ComboBox::from_id_salt("backend")
                    .selected_text(name(self.settings.launcher.backend))
                    .show_ui(ui, |ui| {
                        for b in backends {
                            ui.selectable_value(&mut self.settings.launcher.backend, *b, name(*b));
                        }
                    });
                ui.end_row();

                ui.label(self.t("launcher-custom-args"));
                ui.text_edit_singleline(&mut self.settings.launcher.custom_args);
                ui.end_row();

                ui.label(self.t("launcher-after-launch"));
                let after = [
                    (AfterLaunch::StayOpen, self.t("launcher-after-stay")),
                    (AfterLaunch::Minimize, self.t("launcher-after-minimize")),
                    (AfterLaunch::Close, self.t("launcher-after-close")),
                ];
                let current = after
                    .iter()
                    .find(|(a, _)| *a == self.settings.launcher.after_launch)
                    .map(|(_, l)| l.clone())
                    .unwrap_or_default();
                egui::ComboBox::from_id_salt("after")
                    .selected_text(current)
                    .show_ui(ui, |ui| {
                        for (a, label) in after {
                            ui.selectable_value(&mut self.settings.launcher.after_launch, a, label);
                        }
                    });
                ui.end_row();

                ui.label("");
                let label = self.t("launcher-launch-directly");
                ui.checkbox(&mut self.settings.launcher.launch_directly, label);
                ui.end_row();
            });
        ui.add_space(8.0);
        if ui.button(self.t("launcher-save")).clicked() {
            self.save_settings();
        }
    }

    fn system_tab(&mut self, ui: &mut egui::Ui) {
        let info = self.system.get_or_insert_with(system::scan).clone();
        egui::Grid::new("system")
            .num_columns(2)
            .spacing([16.0, 8.0])
            .show(ui, |ui| {
                for (key, value) in [
                    ("launcher-sys-os", info.os),
                    ("launcher-sys-cpu", info.cpu),
                    ("launcher-sys-cores", info.cores.to_string()),
                    ("launcher-sys-ram", format!("{:.1} GiB", info.ram_gib)),
                    (
                        "launcher-sys-disk",
                        format!("{:.1} GiB", info.free_disk_gib),
                    ),
                ] {
                    ui.label(self.t(key));
                    ui.monospace(value);
                    ui.end_row();
                }
            });
        ui.add_space(8.0);
        ui.weak(self.t("launcher-coming-soon"));
    }
}

impl eframe::App for LauncherApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll_game(&ctx);

        egui::Panel::top("tabs").show(ui, |ui| {
            ui.horizontal(|ui| {
                for tab in Tab::ALL {
                    let label = self.t(tab.key());
                    ui.selectable_value(&mut self.tab, tab, label);
                }
            });
        });
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(&self.status);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.weak(format!("v{}", game_core::GAME_VERSION));
                });
            });
        });
        egui::CentralPanel::default_margins().show(ui, |ui| match self.tab {
            Tab::Play => self.play_tab(ui),
            Tab::System => self.system_tab(ui),
            _ => {
                ui.add_space(12.0);
                ui.weak(self.t("launcher-coming-soon"));
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_args_include_preset_backend_safe_mode_and_custom() {
        let mut s = Settings::default();
        s.quality.preset = QualityPreset::Medium;
        s.launcher.backend = GraphicsBackend::Vulkan;
        s.launcher.custom_args = "--skip-menu --seed 7".into();
        assert_eq!(
            game_args(&s, true),
            vec![
                "--preset",
                "medium",
                "--backend",
                "vulkan",
                "--safe-mode",
                "--skip-menu",
                "--seed",
                "7"
            ]
        );
    }
}
