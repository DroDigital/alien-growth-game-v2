//! Per-stratum Threat meter, telegraphed escalation events, Global Alert from Tier 5.
//!
//! Stub: implemented in M2. The plugin is registered now so the app layout
//! matches docs/ARCHITECTURE.md from the start.

use bevy::prelude::*;

pub struct ThreatPlugin;

impl Plugin for ThreatPlugin {
    fn build(&self, _app: &mut App) {}
}
