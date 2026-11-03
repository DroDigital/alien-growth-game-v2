//! Versioned run saves. The run autosaves at the start of every tier and the
//! save is deleted on death (permanent death). Meta-progression (Genetic
//! Memory, M3) is stored in a separate file.

use crate::sim::RunCheckpoint;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Bump when the format changes, and add a step to [`migrate`].
pub const RUN_SAVE_VERSION: u32 = 1;

pub const RUN_SAVE_FILE: &str = "run.ron";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunSave {
    pub version: u32,
    pub game_version: String,
    pub checkpoint: RunCheckpoint,
}

#[derive(Debug, thiserror::Error)]
pub enum SaveError {
    #[error("I/O error on {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("save file {path} is corrupt: {message}")]
    Corrupt { path: String, message: String },
    #[error("save file version {0} is newer than this game supports")]
    TooNew(u32),
}

/// Upgrades an older save in place. Version 1 is the first format.
fn migrate(save: RunSave) -> Result<RunSave, SaveError> {
    if save.version > RUN_SAVE_VERSION {
        return Err(SaveError::TooNew(save.version));
    }
    Ok(save)
}

pub fn write(path: &Path, checkpoint: &RunCheckpoint) -> Result<(), SaveError> {
    let io = |source| SaveError::Io {
        path: path.display().to_string(),
        source,
    };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(io)?;
    }
    let save = RunSave {
        version: RUN_SAVE_VERSION,
        game_version: crate::GAME_VERSION.to_string(),
        checkpoint: checkpoint.clone(),
    };
    let text = ron::ser::to_string_pretty(&save, ron::ser::PrettyConfig::new()).map_err(|e| {
        SaveError::Corrupt {
            path: path.display().to_string(),
            message: e.to_string(),
        }
    })?;
    let tmp = path.with_extension("ron.tmp");
    std::fs::write(&tmp, text).map_err(io)?;
    std::fs::rename(&tmp, path).map_err(io)
}

/// Reads a save; `Ok(None)` if there is none.
pub fn read(path: &Path) -> Result<Option<RunSave>, SaveError> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(SaveError::Io {
                path: path.display().to_string(),
                source,
            });
        }
    };
    let save: RunSave = ron::from_str(&text).map_err(|e| SaveError::Corrupt {
        path: path.display().to_string(),
        message: e.to_string(),
    })?;
    migrate(save).map(Some)
}

pub fn delete(path: &Path) {
    let _ = std::fs::remove_file(path);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::balance::Balance;
    use crate::rules::Vitals;

    #[test]
    fn round_trip_and_delete() {
        let dir = std::env::temp_dir().join(format!("agg-save-{}", std::process::id()));
        let path = dir.join(RUN_SAVE_FILE);
        let met = Balance::builtin().core.metabolism;
        let cp = RunCheckpoint {
            seed: 42,
            strain: "devourer".into(),
            tier_index: 1,
            vitals: Vitals::new(150.0, &met),
            elapsed: 321.0,
            peak_biomass: 150.0,
            meals: 77,
        };
        assert!(read(&path).unwrap().is_none());
        write(&path, &cp).unwrap();
        let back = read(&path).unwrap().unwrap();
        assert_eq!(back.checkpoint, cp);
        assert_eq!(back.version, RUN_SAVE_VERSION);
        delete(&path);
        assert!(read(&path).unwrap().is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_newer_versions() {
        let mut save = RunSave {
            version: RUN_SAVE_VERSION + 1,
            game_version: "x".into(),
            checkpoint: RunCheckpoint {
                seed: 1,
                strain: "devourer".into(),
                tier_index: 0,
                vitals: Vitals::new(1.0, &Balance::builtin().core.metabolism),
                elapsed: 0.0,
                peak_biomass: 1.0,
                meals: 0,
            },
        };
        assert!(matches!(migrate(save.clone()), Err(SaveError::TooNew(_))));
        save.version = RUN_SAVE_VERSION;
        assert!(migrate(save).is_ok());
    }
}
