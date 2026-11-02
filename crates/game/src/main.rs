//! Alien Growth: the playable app.
//!
//! See docs/ARCHITECTURE.md for the plugin layout. Game rules live in
//! `game_core`; this crate wires them to rendering, input, audio and UI.

mod ai;
mod audio;
mod boot;
mod cli;
mod crash;
mod creature;
#[cfg(feature = "debug-ui")]
mod debug;
mod ecosystem;
mod graphics;
mod input;
mod modding;
mod mutation;
mod online;
mod save;
mod scenes;
mod strata;
mod threat;
mod tooling;
mod ui;

use bevy::prelude::*;
use bevy::window::{ExitCondition, PresentMode, WindowResolution};
use bevy::winit::WinitPlugin;
use clap::Parser;
use cli::{Cli, Mode};
use game_core::config::{QualityPreset, Settings, WindowMode as CfgWindowMode};
use game_core::paths::UserDirs;
use std::time::Duration;

fn main() -> AppExit {
    let cli = Cli::parse();
    let mode = match cli.mode() {
        Ok(m) => m,
        Err(e) => {
            eprintln!("error: {e}");
            return AppExit::from_code(2);
        }
    };

    let user_dirs = UserDirs::resolve();
    if let Err(e) = user_dirs.ensure() {
        eprintln!("warning: cannot create user directories: {e}");
    }
    crash::install_panic_hook(&user_dirs);

    let mut settings = if cli.safe_mode {
        Settings::default()
    } else {
        Settings::load(&user_dirs.settings_file()).unwrap_or_else(|e| {
            eprintln!("warning: settings unreadable ({e}); using defaults");
            Settings::default()
        })
    };
    match (&mode, cli.preset) {
        (Mode::Screenshot { preset, .. }, _) => settings.quality.preset = *preset,
        (_, Some(p)) => settings.quality.preset = p,
        _ => {}
    }
    if cli.safe_mode {
        settings.quality.preset = QualityPreset::Low;
    }
    if let Some(res) = cli.resolution {
        settings.display.resolution = Some(res);
    }
    if matches!(mode, Mode::Benchmark { .. }) {
        // Measure the GPU, not the display refresh rate, at a fixed 1080p
        // window so results compare across machines.
        settings.display.vsync = false;
        settings.display.mode = CfgWindowMode::Windowed;
        settings.display.resolution.get_or_insert((1920, 1080));
    }

    let mut app = App::new();
    // Shaders live in <content root>/shaders and load as `shaders://name.wesl`.
    // Must be registered before AssetPlugin builds.
    let shader_dir = game_core::paths::content_root().join("shaders");
    app.register_asset_source(
        "shaders",
        bevy::asset::io::AssetSourceBuilder::platform_default(&shader_dir.to_string_lossy(), None),
    );
    app.add_plugins(default_plugins(&cli, &mode, &settings));
    if mode.is_headless() {
        // No window and no winit event loop: drive frames ourselves.
        app.add_plugins(bevy::app::ScheduleRunnerPlugin::run_loop(
            Duration::from_secs_f64(1.0 / 240.0),
        ));
    }

    app.insert_resource(boot::LaunchContext {
        mode: mode.clone(),
        cli: cli.clone(),
        user_dirs,
        mods_enabled: !(cli.safe_mode || cli.no_mods),
    })
    .insert_resource(boot::PlayerSettings(settings))
    .add_plugins((
        boot::CorePlugin,
        input::InputPlugin,
        graphics::GraphicsPlugin,
        scenes::ScenesPlugin,
        ui::UiPlugin,
        tooling::ToolingPlugin,
    ))
    .add_plugins((
        creature::CreaturePlugin,
        ecosystem::EcosystemPlugin,
        strata::StrataPlugin,
        mutation::MutationPlugin,
        threat::ThreatPlugin,
        ai::AiPlugin,
        audio::AudioPlugin,
        save::SavePlugin,
        modding::ModPlugin,
        online::OnlinePlugin,
    ));

    #[cfg(feature = "debug-ui")]
    app.add_plugins(debug::DebugPlugin);

    let exit = app.run();
    crash::clear_running_marker(&UserDirs::resolve());
    exit
}

fn default_plugins(cli: &Cli, mode: &Mode, settings: &Settings) -> bevy::app::PluginGroupBuilder {
    let window_plugin = if mode.is_headless() {
        WindowPlugin {
            primary_window: None,
            exit_condition: ExitCondition::DontExit,
            ..default()
        }
    } else {
        let d = &settings.display;
        let resolution = match d.resolution {
            Some((w, h)) => WindowResolution::new(w, h),
            None => WindowResolution::default(),
        };
        let monitor = d
            .monitor
            .map(bevy::window::MonitorSelection::Index)
            .unwrap_or(bevy::window::MonitorSelection::Primary);
        WindowPlugin {
            primary_window: Some(Window {
                title: format!("{} {}", game_core::GAME_TITLE, game_core::GAME_VERSION),
                resolution,
                present_mode: if d.vsync {
                    PresentMode::AutoVsync
                } else {
                    PresentMode::AutoNoVsync
                },
                mode: match d.mode {
                    CfgWindowMode::Windowed => bevy::window::WindowMode::Windowed,
                    CfgWindowMode::BorderlessFullscreen => {
                        bevy::window::WindowMode::BorderlessFullscreen(monitor)
                    }
                    CfgWindowMode::Fullscreen => bevy::window::WindowMode::Fullscreen(
                        monitor,
                        bevy::window::VideoModeSelection::Current,
                    ),
                },
                ..default()
            }),
            ..default()
        }
    };

    let mut plugins = DefaultPlugins
        .set(window_plugin)
        .set(graphics::render_plugin(cli.backend.as_deref()))
        .set(AssetPlugin {
            // Absolute path so dev runs, release folders and macOS bundles all
            // resolve content the same way (see game_core::paths).
            file_path: game_core::paths::assets_dir()
                .to_string_lossy()
                .into_owned(),
            meta_check: bevy::asset::AssetMetaCheck::Never,
            // Shipped content must exist on disk; never fetch at runtime.
            unapproved_path_mode: bevy::asset::UnapprovedPathMode::Forbid,
            ..default()
        })
        .set(bevy::log::LogPlugin {
            filter: "wgpu=error,naga=warn,bevy_render=info,game=info".into(),
            ..default()
        });
    if mode.is_headless() {
        plugins = plugins.disable::<WinitPlugin>();
    }
    plugins
}
