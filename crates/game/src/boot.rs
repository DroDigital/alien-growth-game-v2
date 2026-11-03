//! Core plugin: loads settings, quality presets, localization and the asset
//! manifest, then moves the app from `Boot` to the requested mode.

use crate::cli::{Cli, Mode};
use bevy::prelude::*;
use game_core::config::{PresetTable, RenderSettings, Settings};
use game_core::locale::Localizer;
use game_core::manifest::{AssetManifest, AssetStatus};
use game_core::paths::{self, UserDirs};
use game_core::states::{AppState, FlowStatesPlugin};

/// How the process was launched. Inserted before the app runs.
#[derive(Resource, Debug, Clone)]
pub struct LaunchContext {
    pub mode: Mode,
    pub cli: Cli,
    pub user_dirs: UserDirs,
    pub mods_enabled: bool,
}

/// The player's settings file contents (after CLI overrides).
#[derive(Resource, Debug, Clone, Deref, DerefMut)]
pub struct PlayerSettings(pub Settings);

/// The loaded preset table from `data/presets.ron`.
#[derive(Resource, Debug, Clone, Deref)]
pub struct Presets(pub PresetTable);

/// Effective render settings (preset + overrides). Graphics systems react to
/// changes of this resource.
#[derive(Resource, Debug, Clone, Deref, DerefMut, PartialEq)]
pub struct ActiveRenderSettings(pub RenderSettings);

/// Localized strings.
#[derive(Resource, Deref)]
pub struct Locale(pub Localizer);

/// Asset manifest plus which entries are usable on this install.
#[derive(Resource, Debug, Clone, Default)]
pub struct AssetAvailability {
    pub manifest: Option<AssetManifest>,
    /// Ids of assets present on disk (hash OK or not yet recorded).
    pub available: Vec<String>,
    /// Ids that will use their procedural fallback.
    pub fallbacks: Vec<String>,
}

impl AssetAvailability {
    pub fn is_available(&self, id: &str) -> bool {
        self.available.iter().any(|a| a == id)
    }

    /// Path (relative to assets/) of an available asset.
    pub fn path_of(&self, id: &str) -> Option<String> {
        if !self.is_available(id) {
            return None;
        }
        self.manifest.as_ref()?.get(id).map(|e| e.path.clone())
    }
}

pub struct CorePlugin;

impl Plugin for CorePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FlowStatesPlugin);

        let settings = app.world().resource::<PlayerSettings>().0.clone();
        let presets = load_presets();
        let active = presets.resolve(&settings.quality);
        info!(
            "quality preset {:?}: render scale {:.2}, AA {:?}",
            settings.quality.preset, active.render_scale, active.anti_aliasing
        );

        let locale_root = paths::assets_dir().join("locale");
        let localizer = Localizer::load(&locale_root, &settings.language).unwrap_or_else(|e| {
            error!("localization failed to load ({e}); strings will show their ids");
            Localizer::from_sources(game_core::locale::DEFAULT_LANGUAGE, &[], &[])
        });

        app.insert_resource(load_balance())
            .add_plugins(game_core::sim::SimPlugin)
            .insert_resource(Presets(presets))
            .insert_resource(ActiveRenderSettings(active))
            .insert_resource(Locale(localizer))
            .insert_resource(scan_assets())
            .add_systems(Startup, finish_boot);
    }
}

/// Balance tables from `data/balance`; falls back to the copy compiled into
/// the binary if the shipped files are broken (logged as an error).
fn load_balance() -> game_core::balance::Balance {
    let dir = paths::data_dir().join("balance");
    game_core::balance::Balance::load(&dir).unwrap_or_else(|e| {
        error!("{e}; using built-in balance");
        game_core::balance::Balance::builtin()
    })
}

fn load_presets() -> PresetTable {
    let path = paths::data_dir().join("presets.ron");
    match PresetTable::load(&path) {
        Ok(t) => t,
        Err(e) => {
            // The table ships with the game; a broken file is a packaging bug,
            // but we can still run on the copy compiled into the binary.
            error!("{e}; using built-in presets");
            PresetTable::from_ron_str(include_str!("../../../data/presets.ron"), "built-in")
                .expect("built-in presets are valid")
        }
    }
}

fn scan_assets() -> AssetAvailability {
    let dir = paths::assets_dir();
    let manifest = match AssetManifest::load(&dir.join("manifest.json")) {
        Ok(m) => m,
        Err(e) => {
            warn!("asset manifest unreadable ({e}); every asset uses its fallback");
            return AssetAvailability::default();
        }
    };
    let mut out = AssetAvailability::default();
    for entry in &manifest.assets {
        match entry.status(&dir) {
            AssetStatus::Ok | AssetStatus::Unverified => out.available.push(entry.id.clone()),
            AssetStatus::Missing => out.fallbacks.push(entry.id.clone()),
            AssetStatus::HashMismatch { expected, actual } => {
                warn!(
                    "asset {} hash mismatch (expected {expected}, got {actual}); using fallback",
                    entry.id
                );
                out.fallbacks.push(entry.id.clone());
            }
        }
    }
    if !out.fallbacks.is_empty() {
        info!(
            "{} of {} assets missing, using procedural fallbacks (run tools/fetch_assets.py)",
            out.fallbacks.len(),
            manifest.assets.len()
        );
    }
    out.manifest = Some(manifest);
    out
}

fn finish_boot(ctx: Res<LaunchContext>, mut next: ResMut<NextState<AppState>>) {
    let target = match &ctx.mode {
        Mode::Play if ctx.cli.skip_menu => AppState::InRun,
        Mode::Play => AppState::MainMenu,
        _ => AppState::Tooling,
    };
    next.set(target);
}
