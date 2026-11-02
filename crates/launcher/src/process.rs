//! Locating and launching the game executable.

use game_core::launch::game_executable_name;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus};

/// Finds the game binary: next to the launcher (installed/packaged builds),
/// then in the cargo target directory (dev runs).
pub fn find_game_executable() -> Option<PathBuf> {
    let exe_name = game_executable_name();
    let launcher = std::env::current_exe().ok()?;
    let dir = launcher.parent()?;
    let candidates = [
        dir.join(exe_name),
        // macOS bundle: Contents/MacOS/{launcher,game}
        dir.join("../MacOS").join(exe_name),
    ];
    candidates.into_iter().find(|p| p.is_file())
}

pub struct RunningGame {
    pub child: Child,
}

impl RunningGame {
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Non-blocking check; `Some(status)` once the game has exited.
    pub fn poll(&mut self) -> std::io::Result<Option<ExitStatus>> {
        self.child.try_wait()
    }
}

pub fn launch(exe: &Path, args: &[String]) -> std::io::Result<RunningGame> {
    let mut cmd = Command::new(exe);
    cmd.args(args);
    if let Some(dir) = exe.parent() {
        cmd.current_dir(dir);
        // Dev builds link Bevy dynamically; the dylib sits next to the game.
        // (Windows finds DLLs next to the executable on its own.)
        let var = if cfg!(target_os = "macos") {
            Some("DYLD_FALLBACK_LIBRARY_PATH")
        } else if cfg!(target_os = "linux") {
            Some("LD_LIBRARY_PATH")
        } else {
            None
        };
        if let Some(var) = var {
            // `deps/` covers running straight from cargo's target directory.
            let mut paths = vec![dir.to_path_buf(), dir.join("deps")];
            if let Some(existing) = std::env::var_os(var) {
                paths.extend(std::env::split_paths(&existing));
            }
            if let Ok(joined) = std::env::join_paths(paths) {
                cmd.env(var, joined);
            }
        }
    }
    Ok(RunningGame {
        child: cmd.spawn()?,
    })
}

/// Human-readable exit code (signals on Unix show as `None`).
pub fn exit_code(status: &ExitStatus) -> String {
    match status.code() {
        Some(c) => c.to_string(),
        None => "signal".into(),
    }
}
