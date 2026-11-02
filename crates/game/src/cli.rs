//! Command-line interface. Flag names are shared with the launcher through
//! `game_core::launch`.

use clap::Parser;
use game_core::config::QualityPreset;
use std::path::PathBuf;

#[derive(Parser, Debug, Clone, Default)]
#[command(name = "game", version, about = "Alien Growth")]
pub struct Cli {
    /// Render one frame of a scene offscreen and save it, then exit.
    /// Usage: --screenshot <scene> <preset> <out.png>
    #[arg(long, num_args = 3, value_names = ["SCENE", "PRESET", "OUT"])]
    pub screenshot: Option<Vec<String>>,

    /// Run the scripted benchmark flythrough and write results JSON.
    /// Optional value: duration in seconds (default 20).
    #[arg(long, num_args = 0..=1, default_missing_value = "20", value_name = "SECS")]
    pub benchmark: Option<f64>,

    /// Where `--benchmark` writes its JSON (default: user cache dir).
    #[arg(long, value_name = "FILE")]
    pub benchmark_out: Option<PathBuf>,

    /// Build every render pipeline offscreen, then exit.
    #[arg(long)]
    pub warmup: bool,

    /// Override the quality preset from settings.ron.
    #[arg(long)]
    pub preset: Option<QualityPreset>,

    /// Safe mode: Low preset, mods disabled, settings file ignored.
    #[arg(long)]
    pub safe_mode: bool,

    /// Disable data/asset mods.
    #[arg(long)]
    pub no_mods: bool,

    /// Graphics backend: auto, dx12, vulkan, metal, gl.
    #[arg(long)]
    pub backend: Option<String>,

    /// Skip the main menu and load a scene directly (dev).
    #[arg(long)]
    pub scene: Option<String>,

    /// Output resolution for offscreen modes, e.g. 1920x1080.
    #[arg(long, value_parser = parse_resolution)]
    pub resolution: Option<(u32, u32)>,

    /// Frames to render before capturing a screenshot (lets TAA and
    /// GPU-filtered environment maps converge).
    #[arg(long, default_value_t = 30)]
    pub settle_frames: u32,
}

pub fn parse_resolution(s: &str) -> Result<(u32, u32), String> {
    let (w, h) = s
        .split_once(['x', 'X'])
        .ok_or_else(|| format!("expected WIDTHxHEIGHT, got '{s}'"))?;
    let w: u32 = w.trim().parse().map_err(|e| format!("bad width: {e}"))?;
    let h: u32 = h.trim().parse().map_err(|e| format!("bad height: {e}"))?;
    if w == 0 || h == 0 || w > 8192 || h > 8192 {
        return Err(format!("resolution out of range: {w}x{h}"));
    }
    Ok((w, h))
}

/// What the process was launched to do.
#[derive(Debug, Clone, PartialEq)]
pub enum Mode {
    Play,
    Screenshot {
        scene: String,
        preset: QualityPreset,
        out: PathBuf,
    },
    Benchmark {
        secs: f64,
        out: Option<PathBuf>,
    },
    Warmup,
}

impl Mode {
    pub fn is_headless(&self) -> bool {
        matches!(self, Mode::Screenshot { .. } | Mode::Warmup)
    }
}

impl Cli {
    pub fn mode(&self) -> Result<Mode, String> {
        if let Some(v) = &self.screenshot {
            let [scene, preset, out] = v.as_slice() else {
                return Err("--screenshot needs <scene> <preset> <out.png>".into());
            };
            return Ok(Mode::Screenshot {
                scene: scene.clone(),
                preset: preset.parse()?,
                out: PathBuf::from(out),
            });
        }
        if let Some(secs) = self.benchmark {
            if !(1.0..=600.0).contains(&secs) {
                return Err(format!("benchmark duration out of range: {secs}"));
            }
            return Ok(Mode::Benchmark {
                secs,
                out: self.benchmark_out.clone(),
            });
        }
        if self.warmup {
            return Ok(Mode::Warmup);
        }
        Ok(Mode::Play)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_screenshot_mode() {
        let cli =
            Cli::try_parse_from(["game", "--screenshot", "testbed", "low", "out.png"]).unwrap();
        assert_eq!(
            cli.mode().unwrap(),
            Mode::Screenshot {
                scene: "testbed".into(),
                preset: QualityPreset::Low,
                out: "out.png".into()
            }
        );
    }

    #[test]
    fn parses_benchmark_default_duration() {
        let cli = Cli::try_parse_from(["game", "--benchmark"]).unwrap();
        assert!(matches!(cli.mode().unwrap(), Mode::Benchmark { secs, .. } if secs == 20.0));
    }

    #[test]
    fn rejects_bad_preset_and_resolution() {
        let cli = Cli::try_parse_from(["game", "--screenshot", "a", "potato", "o.png"]).unwrap();
        assert!(cli.mode().is_err());
        assert!(parse_resolution("1920").is_err());
        assert_eq!(parse_resolution("1280x720"), Ok((1280, 720)));
    }
}
