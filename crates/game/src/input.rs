//! Input actions via leafwing-input-manager. Every gameplay verb is an
//! action so the launcher and in-game menus can rebind it.

use bevy::prelude::*;
use leafwing_input_manager::prelude::*;

#[derive(Actionlike, PartialEq, Eq, Hash, Clone, Copy, Debug, Reflect)]
pub enum GameAction {
    #[actionlike(DualAxis)]
    Move,
    #[actionlike(DualAxis)]
    Look,
    Ascend,
    Descend,
    Dash,
    Consume,
    Ability1,
    Ability2,
    Pause,
    Confirm,
    Back,
}

impl GameAction {
    pub fn default_input_map() -> InputMap<Self> {
        InputMap::default()
            .with_dual_axis(Self::Move, VirtualDPad::wasd())
            .with_dual_axis(Self::Move, GamepadStick::LEFT)
            .with_dual_axis(Self::Look, MouseMove::default())
            .with_dual_axis(Self::Look, GamepadStick::RIGHT)
            .with(Self::Ascend, KeyCode::Space)
            .with(Self::Ascend, GamepadButton::RightTrigger)
            .with(Self::Descend, KeyCode::ControlLeft)
            .with(Self::Descend, GamepadButton::LeftTrigger)
            .with(Self::Dash, KeyCode::ShiftLeft)
            .with(Self::Dash, GamepadButton::South)
            .with(Self::Consume, MouseButton::Left)
            .with(Self::Consume, GamepadButton::RightTrigger2)
            .with(Self::Ability1, KeyCode::KeyQ)
            .with(Self::Ability1, GamepadButton::West)
            .with(Self::Ability2, KeyCode::KeyE)
            .with(Self::Ability2, GamepadButton::North)
            .with(Self::Pause, KeyCode::Escape)
            .with(Self::Pause, GamepadButton::Start)
            .with(Self::Confirm, KeyCode::Enter)
            .with(Self::Confirm, GamepadButton::South)
            .with(Self::Back, KeyCode::Backspace)
            .with(Self::Back, GamepadButton::East)
    }
}

/// The entity holding the local player's input map and action state.
#[derive(Component)]
pub struct PlayerInput;

pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(InputManagerPlugin::<GameAction>::default())
            .add_systems(PreStartup, spawn_player_input);
    }
}

fn spawn_player_input(mut commands: Commands) {
    // Rebinding (M2 settings editor) replaces this map from settings.ron.
    commands.spawn((
        Name::new("PlayerInput"),
        PlayerInput,
        GameAction::default_input_map(),
        ActionState::<GameAction>::default(),
    ));
}
