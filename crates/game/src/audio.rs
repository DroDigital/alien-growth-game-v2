//! Audio via bevy_kira_audio (kira). M1: synthesized sound effects (the
//! procedural fallback every SFX needs) pitched by the organism's size, and
//! SFX captions for accessibility. The adaptive layered score, buses,
//! underwater filtering and Threat layers arrive in M2.

use crate::boot::{LaunchContext, Locale, PlayerSettings};
use crate::ui::theme::Theme;
use bevy::prelude::*;
use bevy_kira_audio::prelude::{
    Audio, AudioControl, AudioSource, Decibels, Frame, StaticSoundData, StaticSoundSettings,
};
use game_core::balance::Balance;
use game_core::sim::{
    Consumed, Player, PlayerBitten, PlayerDied, Radius, StruggleEnded, StruggleStarted,
    TierCompleted,
};
use game_core::states::AppState;
use std::sync::Arc;

const RATE: u32 = 44_100;

pub struct AudioPlugin;

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        // Headless tooling runs (screenshot, warm-up, CI) have no audio device.
        if app.world().resource::<LaunchContext>().mode.is_headless() {
            return;
        }
        // Kira backend; Bevy's own audio is disabled at the feature level.
        app.add_plugins(bevy_kira_audio::AudioPlugin)
            .add_systems(Startup, synthesize_bank)
            .add_systems(
                Update,
                (play_sim_sounds, fade_captions).run_if(in_state(AppState::InRun)),
            );
    }
}

/// Synthesized effects.
#[derive(Resource)]
struct SfxBank {
    gulp: Handle<AudioSource>,
    crunch: Handle<AudioSource>,
    bite: Handle<AudioSource>,
    grapple: Handle<AudioSource>,
    rise: Handle<AudioSource>,
    death: Handle<AudioSource>,
}

fn source(samples: Vec<f32>) -> AudioSource {
    let frames: Arc<[Frame]> = samples.into_iter().map(Frame::from_mono).collect();
    AudioSource {
        sound: StaticSoundData {
            sample_rate: RATE,
            frames,
            settings: StaticSoundSettings::default(),
            slice: None,
        },
    }
}

/// Deterministic white noise.
fn noise(i: usize) -> f32 {
    let mut x = (i as u32)
        .wrapping_mul(747_796_405)
        .wrapping_add(2_891_336_453);
    x = ((x >> ((x >> 28) + 4)) ^ x).wrapping_mul(277_803_737);
    ((x >> 22) ^ x) as f32 / u32::MAX as f32 * 2.0 - 1.0
}

/// A tone sweeping from `f0` to `f1` Hz with an exponential decay envelope.
fn sweep(secs: f32, f0: f32, f1: f32, decay: f32, noise_mix: f32) -> Vec<f32> {
    let n = (secs * RATE as f32) as usize;
    let mut phase = 0.0f32;
    (0..n)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let k = t / secs;
            let f = f0 + (f1 - f0) * k;
            phase += f / RATE as f32 * std::f32::consts::TAU;
            let attack = (t / 0.008).min(1.0);
            let env = attack * (-decay * t).exp();
            ((1.0 - noise_mix) * phase.sin() + noise_mix * noise(i)) * env * 0.6
        })
        .collect()
}

fn synthesize_bank(mut commands: Commands, mut sources: ResMut<Assets<AudioSource>>) {
    let rise = {
        // A swelling three-note chord for tier-ups.
        let n = (2.2 * RATE as f32) as usize;
        (0..n)
            .map(|i| {
                let t = i as f32 / RATE as f32;
                let env = (t / 0.8).min(1.0) * (1.0 - t / 2.2).max(0.0);
                let tone = [220.0, 277.2, 329.6]
                    .iter()
                    .map(|f| (t * f * (1.0 + 0.05 * t) * std::f32::consts::TAU).sin())
                    .sum::<f32>()
                    / 3.0;
                tone * env * 0.5
            })
            .collect()
    };
    commands.insert_resource(SfxBank {
        gulp: sources.add(source(sweep(0.16, 420.0, 140.0, 14.0, 0.15))),
        crunch: sources.add(source(sweep(0.28, 260.0, 60.0, 9.0, 0.55))),
        bite: sources.add(source(sweep(0.32, 120.0, 45.0, 8.0, 0.7))),
        grapple: sources.add(source(sweep(0.4, 90.0, 140.0, 4.0, 0.35))),
        rise: sources.add(source(rise)),
        death: sources.add(source(sweep(1.6, 300.0, 40.0, 1.6, 0.25))),
    });
}

/// Bigger organisms sound deeper: playback rate falls with radius.
fn pitch_for(radius: f32, base_radius: f32) -> f64 {
    (radius / base_radius).max(1.0).powf(-0.18).clamp(0.35, 1.5) as f64
}

#[derive(Component)]
struct Caption(f32);

#[allow(clippy::too_many_arguments)]
fn play_sim_sounds(
    mut commands: Commands,
    audio: Res<Audio>,
    bank: Option<Res<SfxBank>>,
    balance: Res<Balance>,
    settings: Res<PlayerSettings>,
    theme: Res<Theme>,
    locale: Res<Locale>,
    player: Query<&Radius, With<Player>>,
    mut consumed: MessageReader<Consumed>,
    mut bitten: MessageReader<PlayerBitten>,
    mut started: MessageReader<StruggleStarted>,
    mut ended: MessageReader<StruggleEnded>,
    mut completed: MessageReader<TierCompleted>,
    mut died: MessageReader<PlayerDied>,
) {
    let Some(bank) = bank else { return };
    let base = balance.core.metabolism.base_radius;
    let pitch = player.single().map(|r| pitch_for(r.0, base)).unwrap_or(1.0);
    let master = settings.audio.master * settings.audio.sfx;
    let volume = Decibels(20.0 * master.max(1e-4).log10());
    let mut caption = |key: &str| {
        if settings.accessibility.sfx_captions {
            commands.spawn((
                Caption(1.6),
                DespawnOnExit(AppState::InRun),
                Text::new(locale.get(key)),
                theme.font(16.0),
                TextColor(theme.text),
                Node {
                    position_type: PositionType::Absolute,
                    bottom: px(48),
                    width: percent(100),
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                TextLayout::justify(Justify::Center),
            ));
        }
    };
    for c in consumed.read() {
        let p = pitch * (1.0 + (0.6 - c.ratio as f64).clamp(-0.3, 0.5));
        audio
            .play(bank.gulp.clone())
            .with_playback_rate(p)
            .with_volume(volume);
        if c.ratio > 0.45 {
            caption("caption-gulp");
        }
    }
    for _ in bitten.read() {
        audio
            .play(bank.bite.clone())
            .with_playback_rate(pitch)
            .with_volume(volume);
        caption("caption-bite");
    }
    for _ in started.read() {
        audio
            .play(bank.grapple.clone())
            .with_playback_rate(pitch)
            .with_volume(volume);
        caption("caption-struggle");
    }
    for e in ended.read() {
        if e.won {
            audio
                .play(bank.crunch.clone())
                .with_playback_rate(pitch)
                .with_volume(volume);
            caption("caption-crunch");
        }
    }
    for _ in completed.read() {
        audio.play(bank.rise.clone()).with_volume(volume);
        caption("caption-rise");
    }
    for _ in died.read() {
        audio
            .play(bank.death.clone())
            .with_playback_rate(pitch)
            .with_volume(volume);
        caption("caption-death");
    }
}

fn fade_captions(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut q: Query<(Entity, &mut Caption)>,
) {
    for (e, mut c) in &mut q {
        c.0 -= time.delta_secs();
        if c.0 <= 0.0 {
            commands.entity(e).despawn();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bigger_is_deeper() {
        assert!(pitch_for(2.0, 0.25) < pitch_for(0.5, 0.25));
        assert_eq!(pitch_for(0.1, 0.25), 1.0);
    }

    #[test]
    fn synthesized_sounds_are_bounded() {
        let s = sweep(0.2, 400.0, 100.0, 10.0, 0.3);
        assert_eq!(s.len(), (0.2 * RATE as f32) as usize);
        assert!(s.iter().all(|v| v.abs() <= 1.0));
    }
}
