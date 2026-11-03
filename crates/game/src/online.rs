//! Platform services behind a trait (Steam via steamworks, local fallback): achievements, leaderboards, cloud saves; lightyear netcode (stretch).
//!
//! Stub: implemented in M6. The plugin is registered now so the app layout
//! matches docs/ARCHITECTURE.md from the start.

use bevy::prelude::*;

pub struct OnlinePlugin;

impl Plugin for OnlinePlugin {
    fn build(&self, _app: &mut App) {}
}
