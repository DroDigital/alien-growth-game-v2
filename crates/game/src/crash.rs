//! Crash handling (M0 part): a panic hook that writes a crash report to the
//! logs directory, and a "running" marker file the launcher uses to detect
//! abnormal exits. Minidumps and the opt-in report upload come in M6.

use game_core::paths::UserDirs;
use std::io::Write;

pub fn install_panic_hook(dirs: &UserDirs) {
    let logs = dirs.logs.clone();
    if let Err(e) = std::fs::write(dirs.running_marker(), std::process::id().to_string()) {
        eprintln!("warning: cannot write running marker: {e}");
    }
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or_default();
        let path = logs.join(format!("crash-{stamp}.txt"));
        if let Ok(mut f) = std::fs::File::create(&path) {
            let _ = writeln!(f, "{} {}", game_core::GAME_TITLE, game_core::GAME_VERSION);
            let _ = writeln!(f, "os: {} {}", std::env::consts::OS, std::env::consts::ARCH);
            let _ = writeln!(f, "{info}");
            let _ = writeln!(f, "{}", std::backtrace::Backtrace::force_capture());
            eprintln!("crash report written to {}", path.display());
        }
        default_hook(info);
    }));
}

/// Called after the app returns normally. If the process dies before this,
/// the marker survives and the launcher offers Safe Mode on next start.
pub fn clear_running_marker(dirs: &UserDirs) {
    let _ = std::fs::remove_file(dirs.running_marker());
}
