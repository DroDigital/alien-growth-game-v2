//! UI plugin: the themed widget layer, main menu and run overlays.
//! All strings come from Fluent via [`Locale`].

pub mod death;
pub mod hud;
pub mod theme;

use crate::boot::{Locale, PlayerSettings};
use crate::input::GameAction;
use crate::save::RunSaveSlot;
use crate::strata::PendingRun;
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui_widgets::{Activate, ActivateOnPress, Button};
use game_core::locale::FluentValue;
use game_core::sim::{DEFAULT_STRAIN, start_run};
use game_core::states::{AppState, RunState};
use leafwing_input_manager::prelude::ActionState;
use theme::Theme;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Theme>()
            .add_plugins((hud::HudPlugin, death::DeathScreenPlugin))
            .add_systems(Startup, apply_ui_scale)
            .add_systems(OnEnter(AppState::MainMenu), spawn_main_menu)
            .add_systems(OnEnter(RunState::Paused), spawn_pause_menu)
            .add_systems(Update, (theme::button_hover_style, toggle_pause));
    }
}

/// What a menu button does when activated.
#[derive(Component, Clone, Copy, Debug)]
pub(crate) enum MenuAction {
    Play,
    Continue,
    Settings,
    Quit,
    Resume,
    Abandon,
    /// Death/victory screen: a fresh run.
    Retry,
    /// Death/victory screen: back to the main menu.
    ToMenu,
}

fn apply_ui_scale(settings: Res<PlayerSettings>, mut scale: ResMut<UiScale>) {
    scale.0 = settings.accessibility.ui_scale.clamp(0.5, 2.5);
}

pub(crate) fn menu_button(theme: &Theme, label: String, action: MenuAction) -> impl Bundle {
    (
        Button,
        ActivateOnPress,
        Hovered::default(),
        action,
        Node {
            width: px(320),
            height: px(56),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(6)),
            ..default()
        },
        BorderColor::all(theme.accent.with_alpha(0.6)),
        BackgroundColor(theme.button),
        children![(Text::new(label), theme.font(22.0), TextColor(theme.text),)],
    )
}

fn spawn_main_menu(
    mut commands: Commands,
    theme: Res<Theme>,
    locale: Res<Locale>,
    slot: Res<RunSaveSlot>,
) {
    let version = locale.get_args(
        "menu-version",
        &[("version", FluentValue::from(game_core::GAME_VERSION))],
    );
    commands
        .spawn((
            Name::new("MainMenu"),
            DespawnOnExit(AppState::MainMenu),
            Node {
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::FlexStart,
                padding: UiRect::left(px(96)),
                row_gap: px(14),
                ..default()
            },
        ))
        .with_children(|root| {
            root.spawn((
                Text::new(locale.get("menu-title")),
                theme.font(64.0),
                TextColor(theme.accent),
            ));
            root.spawn((
                Text::new(locale.get("menu-subtitle")),
                theme.font(18.0),
                TextColor(theme.text_dim),
                Node {
                    margin: UiRect::bottom(px(28)),
                    ..default()
                },
            ));
            if let Some(save) = &slot.save {
                let label = locale.get_args(
                    "menu-continue",
                    &[(
                        "tier",
                        FluentValue::from(save.checkpoint.tier_index as u32 + 1),
                    )],
                );
                root.spawn(menu_button(&theme, label, MenuAction::Continue))
                    .observe(on_menu_activate);
            }
            root.spawn(menu_button(
                &theme,
                locale.get("menu-play"),
                MenuAction::Play,
            ))
            .observe(on_menu_activate);
            root.spawn(menu_button(
                &theme,
                locale.get("menu-settings"),
                MenuAction::Settings,
            ))
            .observe(on_menu_activate);
            root.spawn(menu_button(
                &theme,
                locale.get("menu-quit"),
                MenuAction::Quit,
            ))
            .observe(on_menu_activate);
            root.spawn((
                Text::new(format!("{version}  |  {}", locale.get("menu-stub-notice"))),
                theme.font(14.0),
                TextColor(theme.text_dim),
                Node {
                    position_type: PositionType::Absolute,
                    bottom: px(24),
                    left: px(96),
                    ..default()
                },
            ));
        });
}

fn spawn_pause_menu(mut commands: Commands, theme: Res<Theme>, locale: Res<Locale>) {
    commands
        .spawn((
            Name::new("PauseMenu"),
            DespawnOnExit(RunState::Paused),
            Node {
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                row_gap: px(14),
                ..default()
            },
            BackgroundColor(Color::BLACK.with_alpha(0.55)),
        ))
        .with_children(|root| {
            root.spawn((
                Text::new(locale.get("run-paused")),
                theme.font(48.0),
                TextColor(theme.accent),
            ));
            root.spawn(menu_button(
                &theme,
                locale.get("run-resume"),
                MenuAction::Resume,
            ))
            .observe(on_menu_activate);
            root.spawn(menu_button(
                &theme,
                locale.get("run-abandon"),
                MenuAction::Abandon,
            ))
            .observe(on_menu_activate);
        });
}

pub(crate) fn on_menu_activate(
    activate: On<Activate>,
    mut commands: Commands,
    actions: Query<&MenuAction>,
    mut slot: ResMut<RunSaveSlot>,
    mut pending: ResMut<PendingRun>,
    mut app_state: ResMut<NextState<AppState>>,
    mut run_state: ResMut<NextState<RunState>>,
    mut exit: MessageWriter<AppExit>,
) {
    let Ok(action) = actions.get(activate.entity) else {
        return;
    };
    match action {
        MenuAction::Play => {
            // A new run abandons any saved one (permanent death).
            slot.delete();
            *pending = PendingRun::default();
            app_state.set(AppState::InRun);
        }
        MenuAction::Continue => {
            *pending = PendingRun {
                seed: None,
                resume: slot.save.as_ref().map(|s| s.checkpoint.clone()),
            };
            app_state.set(AppState::InRun);
        }
        MenuAction::Retry => {
            commands.queue(|world: &mut World| {
                start_run(world, rand::random(), DEFAULT_STRAIN, None);
            });
            run_state.set(RunState::Playing);
        }
        MenuAction::ToMenu => app_state.set(AppState::MainMenu),
        MenuAction::Settings => {
            info!("settings menu arrives with the launcher settings editor (M2)")
        }
        MenuAction::Quit => {
            exit.write(AppExit::Success);
        }
        MenuAction::Resume => run_state.set(RunState::Playing),
        MenuAction::Abandon => {
            slot.delete();
            app_state.set(AppState::MainMenu);
        }
    }
}

fn toggle_pause(
    actions: Single<&ActionState<GameAction>, With<crate::input::PlayerInput>>,
    state: Option<Res<State<RunState>>>,
    mut next: ResMut<NextState<RunState>>,
) {
    let Some(state) = state else { return };
    if actions.just_pressed(&GameAction::Pause) {
        match state.get() {
            RunState::Playing => next.set(RunState::Paused),
            RunState::Paused => next.set(RunState::Playing),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use game_core::balance::Balance;
    use game_core::locale::Localizer;
    use std::path::Path;

    /// String-literal keys passed to `locale.get*` / captions in this crate.
    fn keys_in_source(dir: &Path, out: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                keys_in_source(&path, out);
                continue;
            }
            if path.extension().is_none_or(|e| e != "rs") {
                continue;
            }
            let src = std::fs::read_to_string(&path).unwrap();
            for marker in ["get(\"", "get_args(\"", "caption(\"", "get_args(\n"] {
                for (i, _) in src.match_indices(marker) {
                    let rest = src[i + marker.len()..].trim_start().trim_start_matches('"');
                    let key: String = rest
                        .chars()
                        .take_while(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-')
                        .collect();
                    if key.contains('-') {
                        out.push(key);
                    }
                }
            }
        }
    }

    #[test]
    fn every_used_string_exists_in_english() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let english = Localizer::load(&root.join("assets/locale"), "en-US").unwrap();
        let mut keys = Vec::new();
        keys_in_source(&root.join("crates/game/src"), &mut keys);
        let balance = Balance::builtin();
        for s in balance.strains.strains.values() {
            keys.push(s.name_key.clone());
        }
        for t in &balance.strata.tiers {
            keys.push(t.stratum_key.clone());
            keys.push(t.biome_key.clone());
            keys.extend(t.species.iter().map(|s| s.name_key.clone()));
        }
        assert!(keys.len() > 50, "key scan found only {} keys", keys.len());
        let missing: Vec<_> = keys.iter().filter(|k| !english.has(k)).collect();
        assert!(missing.is_empty(), "missing en-US strings: {missing:?}");
    }
}
