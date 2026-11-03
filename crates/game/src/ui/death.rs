//! Death and victory screens: cause of death, run stats, timeline, a heatmap
//! of where the organism went in its final stratum, and restart options.
//! Genetic Memory (meta-progression) arrives in M3; the line is shown now so
//! the layout is final.

use super::theme::Theme;
use super::{MenuAction, menu_button, on_menu_activate};
use crate::boot::Locale;
use bevy::prelude::*;
use game_core::balance::Balance;
use game_core::locale::FluentValue;
use game_core::rules::DeathCause;
use game_core::sim::{Arena, LogKind, RunInfo, RunLog, RunOutcome};
use game_core::states::RunState;

pub struct DeathScreenPlugin;

impl Plugin for DeathScreenPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(RunState::Dead), spawn_end_screen)
            .add_systems(OnEnter(RunState::Victory), spawn_end_screen);
    }
}

const HEATMAP_PX: f32 = 180.0;
const TIMELINE_LINES: usize = 10;

fn species_name(balance: &Balance, locale: &Locale, id: &str) -> String {
    balance
        .strata
        .tiers
        .iter()
        .flat_map(|t| &t.species)
        .find(|s| s.id == id)
        .map(|s| locale.get(&s.name_key))
        .unwrap_or_else(|| id.to_string())
}

pub fn cause_text(cause: &DeathCause, balance: &Balance, locale: &Locale) -> String {
    match cause {
        DeathCause::Starvation => locale.get("death-starvation"),
        DeathCause::Predation(s) => locale.get_args(
            "death-predation",
            &[(
                "species",
                FluentValue::from(species_name(balance, locale, s)),
            )],
        ),
        DeathCause::StruggleInjury(s) => locale.get_args(
            "death-struggle",
            &[(
                "species",
                FluentValue::from(species_name(balance, locale, s)),
            )],
        ),
    }
}

fn log_text(kind: &LogKind, balance: &Balance, locale: &Locale) -> String {
    let name = |s: &str| FluentValue::from(species_name(balance, locale, s));
    match kind {
        LogKind::TierStarted(t) => locale.get_args(
            "log-tier-started",
            &[("tier", FluentValue::from(*t as u32))],
        ),
        LogKind::Meal { species, .. } => locale.get_args("log-meal", &[("species", name(species))]),
        LogKind::StruggleWon(s) => locale.get_args("log-struggle-won", &[("species", name(s))]),
        LogKind::StruggleLost(s) => locale.get_args("log-struggle-lost", &[("species", name(s))]),
        LogKind::Bitten { species, damage } => locale.get_args(
            "log-bitten",
            &[
                ("species", name(species)),
                ("damage", FluentValue::from(format!("{damage:.0}"))),
            ],
        ),
        LogKind::Starving => locale.get("log-starving"),
        LogKind::Gorged => locale.get("log-gorged"),
        LogKind::TierCompleted(t) => locale.get_args(
            "log-tier-completed",
            &[("tier", FluentValue::from(*t as u32))],
        ),
        LogKind::Died(c) => cause_text(c, balance, locale),
    }
}

fn clock(t: f32) -> String {
    let s = t.max(0.0) as u32;
    format!("{:02}:{:02}", s / 60, s % 60)
}

fn spawn_end_screen(
    mut commands: Commands,
    theme: Res<Theme>,
    locale: Res<Locale>,
    balance: Res<Balance>,
    info: Option<Res<RunInfo>>,
    log: Res<RunLog>,
    arena: Option<Res<Arena>>,
) {
    let Some(info) = info else { return };
    let victory = matches!(info.outcome, Some(RunOutcome::Completed));
    let (title, subtitle) = match &info.outcome {
        Some(RunOutcome::Died(cause)) => (
            locale.get("death-title"),
            cause_text(cause, &balance, &locale),
        ),
        _ => (locale.get("victory-title"), locale.get("victory-subtitle")),
    };
    let tier = info.tier(&balance);
    let stats = locale.get_args(
        "death-stats",
        &[
            ("time", FluentValue::from(clock(info.elapsed))),
            (
                "biomass",
                FluentValue::from(format!("{:.0}", info.peak_biomass)),
            ),
            ("meals", FluentValue::from(info.meals)),
            ("tier", FluentValue::from(tier.tier as u32)),
        ],
    );
    let state = if victory {
        RunState::Victory
    } else {
        RunState::Dead
    };

    let root = commands
        .spawn((
            Name::new("EndScreen"),
            DespawnOnExit(state),
            Node {
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                row_gap: px(8),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.02, 0.04, 0.78)),
            GlobalZIndex(40),
        ))
        .id();
    commands.entity(root).with_children(|root| {
        root.spawn((
            Text::new(title),
            theme.font(40.0),
            TextColor(if victory {
                theme.accent
            } else {
                Color::srgb(1.0, 0.45, 0.4)
            }),
        ));
        root.spawn((Text::new(subtitle), theme.font(20.0), TextColor(theme.text)));
        root.spawn((
            Text::new(stats),
            theme.font(16.0),
            TextColor(theme.text_dim),
        ));
        root.spawn((
            Text::new(locale.get_args("death-memory", &[("points", FluentValue::from(0))])),
            theme.font(14.0),
            TextColor(theme.text_dim),
        ));

        // Timeline and heatmap side by side.
        root.spawn(Node {
            flex_direction: FlexDirection::Row,
            column_gap: px(32),
            margin: UiRect::vertical(px(4)),
            ..default()
        })
        .with_children(|row| {
            row.spawn(Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(3),
                width: px(420),
                ..default()
            })
            .with_children(|col| {
                col.spawn((
                    Text::new(locale.get("death-timeline")),
                    theme.font(15.0),
                    TextColor(theme.accent),
                ));
                let skip = log.entries.len().saturating_sub(TIMELINE_LINES);
                for e in log.entries.iter().skip(skip) {
                    col.spawn((
                        Text::new(format!(
                            "{}  {}",
                            clock(e.t),
                            log_text(&e.kind, &balance, &locale)
                        )),
                        theme.font(13.0),
                        TextColor(theme.text),
                    ));
                }
            });
            row.spawn(Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(4),
                ..default()
            })
            .with_children(|col| {
                col.spawn((
                    Text::new(locale.get("death-heatmap")),
                    theme.font(15.0),
                    TextColor(theme.accent),
                ));
                col.spawn((
                    Node {
                        width: px(HEATMAP_PX),
                        height: px(HEATMAP_PX),
                        border: UiRect::all(px(1)),
                        border_radius: BorderRadius::all(px(HEATMAP_PX / 2.0)),
                        ..default()
                    },
                    BorderColor::all(theme.accent.with_alpha(0.5)),
                    BackgroundColor(Color::BLACK.with_alpha(0.5)),
                ))
                .with_children(|map| {
                    let radius = arena.as_ref().map_or(30.0, |a| a.radius);
                    let samples: Vec<_> = log
                        .samples
                        .iter()
                        .filter(|(_, t, _)| *t == info.tier_index)
                        .collect();
                    let n = samples.len().max(1) as f32;
                    let to_px = |p: Vec3| {
                        (
                            (p.x / radius * 0.5 + 0.5) * HEATMAP_PX,
                            (p.z / radius * 0.5 + 0.5) * HEATMAP_PX,
                        )
                    };
                    for (i, (_, _, p)) in samples.iter().enumerate() {
                        let (x, y) = to_px(*p);
                        // Older samples fade toward blue, recent ones glow.
                        let k = i as f32 / n;
                        map.spawn((
                            Node {
                                position_type: PositionType::Absolute,
                                left: px(x - 3.0),
                                top: px(y - 3.0),
                                width: px(6),
                                height: px(6),
                                border_radius: BorderRadius::all(px(3)),
                                ..default()
                            },
                            BackgroundColor(Color::srgba(0.2 + 0.8 * k, 0.6, 1.0 - 0.6 * k, 0.55)),
                        ));
                    }
                    if let Some((_, _, last)) = samples.last() {
                        let (x, y) = to_px(*last);
                        map.spawn((
                            Text::new("X"),
                            theme.font(18.0),
                            TextColor(if victory {
                                theme.accent
                            } else {
                                Color::srgb(1.0, 0.3, 0.3)
                            }),
                            Node {
                                position_type: PositionType::Absolute,
                                left: px(x - 6.0),
                                top: px(y - 11.0),
                                ..default()
                            },
                        ));
                    }
                });
            });
        });

        root.spawn(Node {
            flex_direction: FlexDirection::Row,
            column_gap: px(16),
            ..default()
        })
        .with_children(|row| {
            row.spawn(menu_button(
                &theme,
                locale.get("death-retry"),
                MenuAction::Retry,
            ))
            .observe(on_menu_activate);
            row.spawn(menu_button(
                &theme,
                locale.get("death-menu"),
                MenuAction::ToMenu,
            ))
            .observe(on_menu_activate);
        });
    });
}
