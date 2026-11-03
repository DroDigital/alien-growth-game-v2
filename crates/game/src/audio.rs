//! Adaptive layered score and buses via bevy_kira_audio (kira): underwater
//! low-pass, pressure rumble, Threat layers, synthesized fallbacks for
//! missing SFX.
//!
//! Stub: implemented in M1. The plugin is registered now so the app layout
//! matches docs/ARCHITECTURE.md from the start.

use crate::boot::LaunchContext;
use bevy::prelude::*;

pub struct AudioPlugin;

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        // Headless tooling runs (screenshot, warm-up, CI) have no audio device.
        if app.world().resource::<LaunchContext>().mode.is_headless() {
            return;
        }
        // Kira backend; Bevy's own audio is disabled at the feature level.
        app.add_plugins(bevy_kira_audio::AudioPlugin);
    }
}
