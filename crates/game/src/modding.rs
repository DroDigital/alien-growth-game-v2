//! Data (RON/JSON) and asset-folder mods with load order and conflict
//! detection. Disabled by --safe-mode/--no-mods.
//!
//! Stub: implemented in M6. The plugin is registered now so the app layout
//! matches docs/ARCHITECTURE.md from the start.

use crate::boot::LaunchContext;
use bevy::prelude::*;

pub struct ModPlugin;

impl Plugin for ModPlugin {
    fn build(&self, app: &mut App) {
        if !app.world().resource::<LaunchContext>().mods_enabled {
            info!("mods disabled (safe mode or --no-mods)");
        }
    }
}
