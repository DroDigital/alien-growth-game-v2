//! Command-line contract between the launcher and the game.
//!
//! The game parses these flags with clap; the launcher builds them with the
//! helpers below so both sides agree on spelling.

pub const FLAG_PRESET: &str = "--preset";
pub const FLAG_SAFE_MODE: &str = "--safe-mode";
pub const FLAG_BENCHMARK: &str = "--benchmark";
pub const FLAG_WARMUP: &str = "--warmup";
pub const FLAG_SCREENSHOT: &str = "--screenshot";
pub const FLAG_BACKEND: &str = "--backend";
pub const FLAG_NO_MODS: &str = "--no-mods";

/// Name of the game executable next to the launcher.
pub fn game_executable_name() -> &'static str {
    if cfg!(windows) { "game.exe" } else { "game" }
}

/// Splits a user-entered "custom arguments" string, honoring double quotes.
pub fn split_args(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    let mut has_token = false;
    for c in s.chars() {
        match c {
            '"' => {
                in_quotes = !in_quotes;
                has_token = true;
            }
            c if c.is_whitespace() && !in_quotes => {
                if has_token {
                    out.push(std::mem::take(&mut cur));
                    has_token = false;
                }
            }
            c => {
                cur.push(c);
                has_token = true;
            }
        }
    }
    if has_token {
        out.push(cur);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_args_handles_quotes() {
        assert_eq!(
            split_args(r#"--preset low  --screenshot "my scene" x"#),
            vec!["--preset", "low", "--screenshot", "my scene", "x"]
        );
        assert!(split_args("   ").is_empty());
        assert_eq!(split_args(r#""""#), vec![""]);
    }
}
