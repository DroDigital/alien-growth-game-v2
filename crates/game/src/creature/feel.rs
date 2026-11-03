//! Game feel: hit-stop on big meals and bites, camera trauma (shake), and
//! gamepad rumble during struggles and impacts. Shake and rumble respect the
//! accessibility settings.

use crate::boot::PlayerSettings;
use bevy::input::gamepad::{GamepadRumbleIntensity, GamepadRumbleRequest};
use bevy::prelude::*;
use game_core::sim::{Consumed, PlayerBitten, StruggleEnded, StruggleStarted, TierCompleted};
use std::time::Duration;

/// Camera shake amount (0..=1); the follow camera reads and decays it.
#[derive(Resource, Debug, Default)]
pub struct Trauma(pub f32);

impl Trauma {
    pub fn add(&mut self, amount: f32) {
        self.0 = (self.0 + amount).min(1.0);
    }
}

/// Real-time seconds of frozen game time left.
#[derive(Resource, Debug, Default)]
struct HitStop(f32);

pub struct FeelPlugin;

impl Plugin for FeelPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Trauma>()
            .init_resource::<HitStop>()
            .add_systems(Update, (react_to_sim, run_hit_stop).chain());
    }
}

fn rumble(
    gamepads: &Query<Entity, With<Gamepad>>,
    out: &mut MessageWriter<GamepadRumbleRequest>,
    strong: f32,
    weak: f32,
    ms: u64,
) {
    for gamepad in gamepads {
        out.write(GamepadRumbleRequest::Add {
            gamepad,
            duration: Duration::from_millis(ms),
            intensity: GamepadRumbleIntensity {
                strong_motor: strong,
                weak_motor: weak,
            },
        });
    }
}

fn react_to_sim(
    settings: Res<PlayerSettings>,
    mut trauma: ResMut<Trauma>,
    mut hit_stop: ResMut<HitStop>,
    mut consumed: MessageReader<Consumed>,
    mut bitten: MessageReader<PlayerBitten>,
    mut started: MessageReader<StruggleStarted>,
    mut ended: MessageReader<StruggleEnded>,
    mut completed: MessageReader<TierCompleted>,
    gamepads: Query<Entity, With<Gamepad>>,
    mut rumble_out: MessageWriter<GamepadRumbleRequest>,
) {
    let shake = settings.accessibility.camera_shake;
    let rumble_on = settings.accessibility.controller_rumble;
    for c in consumed.read() {
        // Bigger mouthfuls hit harder.
        let weight = (c.ratio / 0.8).clamp(0.0, 1.5);
        if weight > 0.45 {
            hit_stop.0 = hit_stop.0.max(0.03 + 0.05 * weight);
            if shake {
                trauma.add(0.12 * weight);
            }
            if rumble_on {
                rumble(&gamepads, &mut rumble_out, 0.2 * weight, 0.4 * weight, 120);
            }
        }
    }
    for b in bitten.read() {
        hit_stop.0 = hit_stop.0.max(0.07);
        if shake {
            trauma.add((b.damage / 30.0).clamp(0.25, 0.8));
        }
        if rumble_on {
            rumble(&gamepads, &mut rumble_out, 0.8, 0.5, 250);
        }
    }
    for _ in started.read() {
        if rumble_on {
            rumble(&gamepads, &mut rumble_out, 0.35, 0.6, 400);
        }
    }
    for e in ended.read() {
        if e.won {
            hit_stop.0 = hit_stop.0.max(0.09);
        }
        if shake {
            trauma.add(if e.won { 0.3 } else { 0.45 });
        }
        if rumble_on {
            rumble(
                &gamepads,
                &mut rumble_out,
                if e.won { 0.5 } else { 0.9 },
                0.6,
                220,
            );
        }
    }
    for _ in completed.read() {
        if rumble_on {
            rumble(&gamepads, &mut rumble_out, 0.6, 0.3, 900);
        }
    }
}

/// Pauses virtual time (and so the fixed-step simulation) for a few frames.
fn run_hit_stop(
    real: Res<Time<Real>>,
    mut virt: ResMut<Time<Virtual>>,
    mut hit_stop: ResMut<HitStop>,
) {
    if hit_stop.0 > 0.0 {
        if !virt.is_paused() {
            virt.pause();
        }
        hit_stop.0 -= real.delta_secs();
        if hit_stop.0 <= 0.0 {
            hit_stop.0 = 0.0;
            virt.unpause();
        }
    }
}
