//! Bridges game_core's Lotka-Volterra food web to the world: aggregate populations, promotion of nearby agents to full entities, GPU boid swarms.
//!
//! Stub: implemented in M2. The plugin is registered now so the app layout
//! matches docs/ARCHITECTURE.md from the start.

use bevy::prelude::*;

pub struct EcosystemPlugin;

impl Plugin for EcosystemPlugin {
    fn build(&self, _app: &mut App) {}
}
