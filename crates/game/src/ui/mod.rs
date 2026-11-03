//! UI plugin: the themed widget layer, main menu and run overlays.
//! All strings come from Fluent via [`Locale`].

pub mod theme;

use crate::boot::{Locale, PlayerSettings};
use crate::input::GameAction;
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui_widgets::{Activate, ActivateOnPress, Button};
use game_core::locale::FluentValue;
use game_core::states::{AppState, RunState};
use leafwing_input_manager::prelude::ActionState;
use theme::Theme;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Theme>()
            .add_systems(Startup, apply_ui_scale)
            .add_systems(OnEnter(AppState::MainMenu), spawn_main_menu)
            .add_systems(OnEnter(AppState::InRun), spawn_run_overlay)
            .add_systems(OnEnter(RunState::Paused), spawn_pause_menu)
            .add_systems(Update, (theme::button_hover_style, toggle_pause));
    }
}

/// What a menu button does when activated.
#[derive(Component, Clone, Copy, Debug)]
enum MenuAction {
    Play,
    Settings,
    Quit,
    Resume,
    Abandon,
}

fn apply_ui_scale(settings: Res<PlayerSettings>, mut scale: ResMut<UiScale>) {
    scale.0 = settings.accessibility.ui_scale.clamp(0.5, 2.5);
}

fn menu_button(theme: &Theme, label: String, action: MenuAction) -> impl Bundle {
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

fn spawn_main_menu(mut commands: Commands, theme: Res<Theme>, locale: Res<Locale>) {
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

fn spawn_run_overlay(mut commands: Commands, theme: Res<Theme>, locale: Res<Locale>) {
    commands.spawn((
        Name::new("RunOverlay"),
        DespawnOnExit(AppState::InRun),
        Text::new(locale.get("menu-stub-notice")),
        theme.font(16.0),
        TextColor(theme.text_dim),
        Node {
            position_type: PositionType::Absolute,
            top: px(16),
            left: px(16),
            ..default()
        },
    ));
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

fn on_menu_activate(
    activate: On<Activate>,
    actions: Query<&MenuAction>,
    mut app_state: ResMut<NextState<AppState>>,
    mut run_state: ResMut<NextState<RunState>>,
    mut exit: MessageWriter<AppExit>,
) {
    let Ok(action) = actions.get(activate.entity) else {
        return;
    };
    match action {
        MenuAction::Play => app_state.set(AppState::InRun),
        MenuAction::Settings => {
            info!("settings menu arrives with the launcher settings editor (M2)")
        }
        MenuAction::Quit => {
            exit.write(AppExit::Success);
        }
        MenuAction::Resume => run_state.set(RunState::Playing),
        MenuAction::Abandon => app_state.set(AppState::MainMenu),
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
