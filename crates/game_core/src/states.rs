//! Game flow states. Shared by the game and the headless balance bots.

use bevy::prelude::*;

/// Top-level application flow.
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AppState {
    /// Loading settings, presets, manifest and fallback assets.
    #[default]
    Boot,
    MainMenu,
    /// A run is in progress (see [`RunState`]).
    InRun,
    /// Non-interactive tooling modes (screenshot, benchmark, warm-up).
    Tooling,
}

/// Phases inside a run. Only exists while `AppState::InRun`.
#[derive(SubStates, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[source(AppState = AppState::InRun)]
pub enum RunState {
    #[default]
    Playing,
    Paused,
    /// Zoom-out cinematic while the next stratum loads.
    TierTransition,
    /// Death screen.
    Dead,
    /// The final available stratum was completed (end of the current slice).
    Victory,
}

/// Registers the flow states on an app (works with `MinimalPlugins` too).
pub struct FlowStatesPlugin;

impl Plugin for FlowStatesPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<AppState>().add_sub_state::<RunState>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::state::app::StatesPlugin;

    #[test]
    fn run_state_exists_only_in_run() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, FlowStatesPlugin));
        app.update();
        assert!(app.world().get_resource::<State<RunState>>().is_none());

        app.world_mut()
            .resource_mut::<NextState<AppState>>()
            .set(AppState::InRun);
        app.update();
        assert_eq!(
            *app.world().resource::<State<RunState>>().get(),
            RunState::Playing
        );

        app.world_mut()
            .resource_mut::<NextState<AppState>>()
            .set(AppState::MainMenu);
        app.update();
        assert!(app.world().get_resource::<State<RunState>>().is_none());
    }
}
