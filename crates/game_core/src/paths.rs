//! Filesystem locations shared by the game, launcher and tools.
//!
//! Read-only content (assets/, data/, shaders/) lives in the install root.
//! Per-user files (settings, saves, logs, caches) live in OS-specific
//! directories from the `directories` crate.

use directories::ProjectDirs;
use std::path::{Path, PathBuf};

/// Environment variable that overrides the install/content root. Cargo sets
/// it (see `.cargo/config.toml`) so dev runs find the workspace content.
pub const CONTENT_ROOT_ENV: &str = "BEVY_ASSET_ROOT";

/// Environment variable that overrides the per-user directory (tests, CI,
/// portable installs).
pub const USER_DIR_ENV: &str = "AGG_USER_DIR";

/// Root of read-only content: contains `assets/`, `data/`, `shaders/`.
pub fn content_root() -> PathBuf {
    if let Ok(root) = std::env::var(CONTENT_ROOT_ENV)
        && !root.is_empty()
    {
        return PathBuf::from(root);
    }
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."));
    // macOS app bundle: Contents/MacOS/<exe> with content in Contents/Resources.
    let bundle_resources = exe_dir.join("../Resources");
    if bundle_resources.join("data").is_dir() {
        return bundle_resources;
    }
    exe_dir
}

pub fn data_dir() -> PathBuf {
    content_root().join("data")
}

pub fn assets_dir() -> PathBuf {
    content_root().join("assets")
}

/// Per-user writable locations.
#[derive(Debug, Clone)]
pub struct UserDirs {
    pub config: PathBuf,
    pub saves: PathBuf,
    pub logs: PathBuf,
    pub cache: PathBuf,
}

impl UserDirs {
    pub fn resolve() -> Self {
        if let Ok(dir) = std::env::var(USER_DIR_ENV)
            && !dir.is_empty()
        {
            return Self::under(Path::new(&dir));
        }
        match ProjectDirs::from("dev", "drodigital", "alien-growth") {
            Some(p) => Self {
                config: p.config_dir().to_path_buf(),
                saves: p.data_dir().join("saves"),
                logs: p.data_local_dir().join("logs"),
                cache: p.cache_dir().to_path_buf(),
            },
            None => Self::under(Path::new(".user")),
        }
    }

    fn under(root: &Path) -> Self {
        Self {
            config: root.join("config"),
            saves: root.join("saves"),
            logs: root.join("logs"),
            cache: root.join("cache"),
        }
    }

    pub fn settings_file(&self) -> PathBuf {
        self.config.join("settings.ron")
    }

    /// Marker written while the game runs; if it survives a launch the
    /// previous session exited abnormally (used by the launcher's crash flow).
    pub fn running_marker(&self) -> PathBuf {
        self.cache.join("game.running")
    }

    pub fn pipeline_cache_dir(&self) -> PathBuf {
        self.cache.join("pipelines")
    }

    pub fn benchmark_file(&self) -> PathBuf {
        self.cache.join("benchmark.json")
    }

    pub fn ensure(&self) -> std::io::Result<()> {
        for d in [&self.config, &self.saves, &self.logs, &self.cache] {
            std::fs::create_dir_all(d)?;
        }
        Ok(())
    }
}
