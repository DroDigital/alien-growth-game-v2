//! In-run HUD: biomass and tier progress, energy, integrity, stomach fill
//! (with the Gorged threshold), status tags and the struggle meter.
//! States are shown with text tags as well as color.

use super::theme::Theme;
use crate::boot::Locale;
use bevy::prelude::*;
use game_core::balance::Balance;
use game_core::locale::FluentValue;
use game_core::sim::{Player, PlayerVitals, RunInfo, Struggling};
use game_core::states::{AppState, RunState};

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InRun), spawn_hud)
            .add_systems(
                Update,
                (update_hud, show_hud_while_playing).run_if(in_state(AppState::InRun)),
            );
    }
}

#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum HudBar {
    Progress,
    Energy,
    Integrity,
    Stomach,
    Struggle,
}

#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum HudText {
    Biomass,
    Tier,
    Status,
    StrugglePrompt,
}

#[derive(Component)]
struct StrugglePanel;

#[derive(Component)]
struct HudRoot;

/// The HUD steps aside for the tier cinematic and the end screens.
fn show_hud_while_playing(
    state: Res<State<RunState>>,
    mut hud: Query<&mut Visibility, With<HudRoot>>,
) {
    if !state.is_changed() {
        return;
    }
    let shown = matches!(state.get(), RunState::Playing | RunState::Paused);
    for mut vis in &mut hud {
        *vis = if shown {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

const BAR_W: f32 = 260.0;

fn bar(theme: &Theme, kind: HudBar, label: String, fill: Color, width: f32) -> impl Bundle {
    (
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: px(3),
            ..default()
        },
        children![
            (
                Text::new(label),
                theme.font(13.0),
                TextColor(theme.text_dim)
            ),
            (
                Node {
                    width: px(width),
                    height: px(12),
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::all(px(3)),
                    ..default()
                },
                BorderColor::all(theme.accent.with_alpha(0.5)),
                BackgroundColor(Color::BLACK.with_alpha(0.45)),
                children![(
                    kind,
                    Node {
                        width: percent(50),
                        height: percent(100),
                        ..default()
                    },
                    BackgroundColor(fill),
                )],
            ),
        ],
    )
}

fn spawn_hud(mut commands: Commands, theme: Res<Theme>, locale: Res<Locale>) {
    commands.spawn((
        Name::new("Hud"),
        HudRoot,
        DespawnOnExit(AppState::InRun),
        Node {
            position_type: PositionType::Absolute,
            left: px(12),
            top: px(10),
            padding: UiRect::axes(px(10), px(8)),
            flex_direction: FlexDirection::Column,
            row_gap: px(8),
            border_radius: BorderRadius::all(px(6)),
            ..default()
        },
        // Keeps labels legible over bright sand and close-up prey.
        BackgroundColor(Color::srgba(0.0, 0.03, 0.05, 0.55)),
        children![
            (
                HudText::Tier,
                Text::new(""),
                theme.font(14.0),
                TextColor(theme.text_dim)
            ),
            (
                HudText::Biomass,
                Text::new(""),
                theme.font(24.0),
                TextColor(theme.accent)
            ),
            bar(
                &theme,
                HudBar::Progress,
                locale.get("hud-progress"),
                theme.accent.with_alpha(0.8),
                BAR_W
            ),
            bar(
                &theme,
                HudBar::Energy,
                locale.get("hud-energy"),
                Color::srgb(0.95, 0.8, 0.25),
                BAR_W
            ),
            bar(
                &theme,
                HudBar::Integrity,
                locale.get("hud-integrity"),
                Color::srgb(0.3, 0.85, 0.55),
                BAR_W
            ),
            bar(
                &theme,
                HudBar::Stomach,
                locale.get("hud-stomach"),
                Color::srgb(0.85, 0.5, 0.8),
                BAR_W
            ),
            (
                HudText::Status,
                Text::new(""),
                theme.font(15.0),
                TextColor(Color::srgb(1.0, 0.55, 0.4))
            ),
        ],
    ));
    commands.spawn((
        Name::new("StrugglePanel"),
        StrugglePanel,
        DespawnOnExit(AppState::InRun),
        Visibility::Hidden,
        Node {
            position_type: PositionType::Absolute,
            bottom: percent(22),
            width: percent(100),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: px(6),
            ..default()
        },
        children![
            (
                HudText::StrugglePrompt,
                Text::new(locale.get("hud-struggle")),
                theme.font(22.0),
                TextColor(theme.text),
            ),
            bar(&theme, HudBar::Struggle, String::new(), theme.accent, 360.0),
        ],
    ));
}

fn update_hud(
    balance: Res<Balance>,
    locale: Res<Locale>,
    info: Option<Res<RunInfo>>,
    player: Query<(&PlayerVitals, Option<&Struggling>), With<Player>>,
    mut bars: Query<(&HudBar, &mut Node)>,
    mut texts: Query<(&HudText, &mut Text)>,
    mut panel: Query<&mut Visibility, With<StrugglePanel>>,
) {
    let (Some(info), Ok((vitals, struggling))) = (info, player.single()) else {
        return;
    };
    let v = &vitals.0;
    let met = &balance.core.metabolism;
    let tier = info.tier(&balance);
    let span = (tier.complete_biomass - tier.start_biomass).max(1e-3);
    let progress = ((v.biomass - tier.start_biomass) / span).clamp(0.0, 1.0);
    let stomach = (v.stomach_fill(met) / met.max_stomach_fill).clamp(0.0, 1.0);
    for (kind, mut node) in &mut bars {
        let value = match kind {
            HudBar::Progress => progress,
            HudBar::Energy => v.energy / 100.0,
            HudBar::Integrity => v.integrity / 100.0,
            HudBar::Stomach => stomach,
            HudBar::Struggle => struggling.map_or(0.0, |s| s.struggle.progress.clamp(0.0, 1.0)),
        };
        node.width = percent(value * 100.0);
    }
    let mut tags = Vec::new();
    if v.energy <= 0.0 {
        tags.push(locale.get("hud-starving"));
    } else if v.energy < 20.0 {
        tags.push(locale.get("hud-hungry"));
    }
    if v.is_gorged(met) {
        tags.push(locale.get("hud-gorged"));
    }
    if v.integrity < 30.0 {
        tags.push(locale.get("hud-critical"));
    }
    for (kind, mut text) in &mut texts {
        match kind {
            HudText::Biomass => {
                text.0 = locale.get_args(
                    "hud-biomass",
                    &[("value", FluentValue::from(format!("{:.1}", v.biomass)))],
                );
            }
            HudText::Tier => {
                text.0 = format!(
                    "{}  |  {}",
                    locale.get_args(
                        "stratum-heading",
                        &[("tier", FluentValue::from(tier.tier as u32))]
                    ),
                    locale.get(&tier.stratum_key)
                );
            }
            HudText::Status => text.0 = tags.join("   "),
            HudText::StrugglePrompt => {}
        }
    }
    for mut vis in &mut panel {
        *vis = if struggling.is_some() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}
