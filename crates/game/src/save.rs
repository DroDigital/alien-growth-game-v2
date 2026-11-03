//! Versioned saves with migrations; run state and meta-progression stored separately; autosave at every tier.
//!
//! Stub: implemented in M1. The plugin is registered now so the app layout
//! matches docs/ARCHITECTURE.md from the start.

use bevy::prelude::*;

pub struct SavePlugin;

impl Plugin for SavePlugin {
    fn build(&self, _app: &mut App) {}
}
