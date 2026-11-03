//! Pure game rules, data types and configuration.
//!
//! This crate never touches rendering, windows or audio. Everything here can be
//! unit-tested headless and is shared by the game, the launcher and the tools.
//! The `bevy` feature adds ECS integration (states, headless plugins); the
//! launcher builds without it.

pub mod balance;
pub mod config;
pub mod half;
pub mod launch;
pub mod locale;
pub mod manifest;
pub mod paths;
pub mod perf;
pub mod rules;
pub mod terrain;

#[cfg(feature = "bevy")]
pub mod bots;
#[cfg(feature = "bevy")]
pub mod save;
#[cfg(feature = "bevy")]
pub mod sim;
#[cfg(feature = "bevy")]
pub mod states;

/// Human-readable working title, used for window titles and directories.
pub const GAME_TITLE: &str = "Alien Growth";

/// Version string shared by the game and the launcher.
pub const GAME_VERSION: &str = env!("CARGO_PKG_VERSION");
