//! Run autosave: written at the start of every tier, deleted on death,
//! victory or abandoning the run (permanent death). The main menu offers
//! "Continue" while a save exists. Format and migrations: `game_core::save`.

use crate::boot::LaunchContext;
use bevy::prelude::*;
use game_core::save::{self, RUN_SAVE_FILE, RunSave};
use game_core::sim::{PlayerDied, PlayerVitals, RunInfo, TierCompleted, TierEntered};
use game_core::states::AppState;
use std::path::PathBuf;

/// The run save on disk, if any.
#[derive(Resource, Debug, Clone, Default)]
pub struct RunSaveSlot {
    pub path: PathBuf,
    pub save: Option<RunSave>,
}

impl RunSaveSlot {
    pub fn delete(&mut self) {
        save::delete(&self.path);
        self.save = None;
    }
}

pub struct SavePlugin;

impl Plugin for SavePlugin {
    fn build(&self, app: &mut App) {
        let path = app
            .world()
            .resource::<LaunchContext>()
            .user_dirs
            .saves
            .join(RUN_SAVE_FILE);
        let save = match save::read(&path) {
            Ok(s) => s,
            Err(e) => {
                warn!("ignoring unreadable run save: {e}");
                None
            }
        };
        app.insert_resource(RunSaveSlot { path, save }).add_systems(
            Update,
            (autosave_on_tier, delete_on_run_end).run_if(in_state(AppState::InRun)),
        );
    }
}

fn autosave_on_tier(
    mut entered: MessageReader<TierEntered>,
    ctx: Res<LaunchContext>,
    info: Option<Res<RunInfo>>,
    player: Query<&PlayerVitals>,
    mut slot: ResMut<RunSaveSlot>,
) {
    if entered.read().last().is_none() || ctx.mode.is_headless() {
        return;
    }
    let (Some(info), Ok(vitals)) = (info, player.single()) else {
        return;
    };
    let cp = crate::strata::checkpoint(&info, &vitals.0);
    match save::write(&slot.path, &cp) {
        Ok(()) => {
            slot.save = save::read(&slot.path).ok().flatten();
            info!("autosaved at tier {}", info.tier_index + 1);
        }
        Err(e) => warn!("autosave failed: {e}"),
    }
}

fn delete_on_run_end(
    mut died: MessageReader<PlayerDied>,
    mut completed: MessageReader<TierCompleted>,
    mut slot: ResMut<RunSaveSlot>,
) {
    let dead = died.read().count() > 0;
    let won = completed.read().any(|c| c.last);
    if dead || won {
        slot.delete();
    }
}
