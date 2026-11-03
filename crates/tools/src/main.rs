//! `agtool`: build-time tooling.
//!
//! - `texconv`        convert PNG/JPG/HDR to KTX2 (BC4/BC5/BC6H/BC7 + mips + zstd)
//! - `validate-data`  parse every shipped data file (presets, Fluent strings)
//! - `verify-assets`  lint the asset manifest and check files against hashes
//! - `credits`        generate CREDITS.md from the manifest (`--check` for CI)

mod credits;
mod ktx2;
mod texconv;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use game_core::config::PresetTable;
use game_core::locale::{Localizer, available_languages};
use game_core::manifest::{AssetManifest, AssetStatus};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "agtool", about = "Alien Growth build tools", version)]
struct Cli {
    /// Workspace/content root (contains assets/ and data/).
    #[arg(long, global = true, default_value = ".")]
    root: PathBuf,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Convert an image to KTX2.
    Texconv {
        input: PathBuf,
        output: PathBuf,
        #[arg(long, value_enum)]
        kind: texconv::Kind,
        /// Largest side in texels (downscales by halving). Must be at least 1.
        #[arg(long, value_parser = clap::value_parser!(u32).range(1..))]
        max_size: Option<u32>,
        /// Zstandard level for supercompression (0 disables).
        #[arg(long, default_value_t = 19)]
        zstd: i32,
    },
    /// Parse every shipped data file.
    ValidateData,
    /// Lint the asset manifest and verify downloaded files.
    VerifyAssets {
        /// Fail if any base-set asset is missing (release builds).
        #[arg(long)]
        require_all: bool,
    },
    /// Generate CREDITS.md from assets/manifest.json.
    Credits {
        /// Exit non-zero if CREDITS.md is out of date instead of writing it.
        #[arg(long)]
        check: bool,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    let root = cli.root;
    match cli.cmd {
        Cmd::Texconv {
            input,
            output,
            kind,
            max_size,
            zstd,
        } => {
            let bytes = texconv::convert(
                &input,
                &texconv::Options {
                    kind,
                    max_size,
                    zstd_level: (zstd > 0).then_some(zstd),
                },
            )?;
            if let Some(dir) = output.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(&output, &bytes)
                .with_context(|| format!("writing {}", output.display()))?;
            println!(
                "{} -> {} ({} bytes)",
                input.display(),
                output.display(),
                bytes.len()
            );
            Ok(())
        }
        Cmd::ValidateData => validate_data(&root),
        Cmd::VerifyAssets { require_all } => verify_assets(&root, require_all),
        Cmd::Credits { check } => {
            let manifest = AssetManifest::load(&root.join("assets/manifest.json"))
                .context("reading assets/manifest.json")?;
            let text = credits::render(&manifest);
            let path = root.join("CREDITS.md");
            if check {
                let current = std::fs::read_to_string(&path).unwrap_or_default();
                if current != text {
                    bail!("CREDITS.md is out of date; run `cargo agtool credits`");
                }
                println!("CREDITS.md is up to date");
            } else {
                std::fs::write(&path, text)?;
                println!("wrote {}", path.display());
            }
            Ok(())
        }
    }
}

fn validate_data(root: &std::path::Path) -> Result<()> {
    let presets = root.join("data/presets.ron");
    PresetTable::load(&presets).with_context(|| format!("{}", presets.display()))?;
    println!("ok  {}", presets.display());

    // Every other RON file must at least be well-formed until it has a schema.
    for entry in walk(&root.join("data"))? {
        if entry.extension().is_some_and(|e| e == "ron") && entry != presets {
            let src = std::fs::read_to_string(&entry)?;
            ron::from_str::<ron::Value>(&src).with_context(|| format!("{}", entry.display()))?;
            println!("ok  {}", entry.display());
        }
    }

    let locale_root = root.join("assets/locale");
    let langs = available_languages(&locale_root);
    if langs.is_empty() {
        bail!("no locales under {}", locale_root.display());
    }
    let english = Localizer::load(&locale_root, game_core::locale::DEFAULT_LANGUAGE)?;
    for lang in langs {
        let l = Localizer::load(&locale_root, &lang).with_context(|| format!("locale {lang}"))?;
        println!("ok  locale {} ({})", lang, l.language());
    }
    let _ = english;
    Ok(())
}

fn walk(dir: &std::path::Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).with_context(|| format!("{}", d.display()))? {
            let p = e?.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.push(p);
            }
        }
    }
    out.sort();
    Ok(out)
}

fn verify_assets(root: &std::path::Path, require_all: bool) -> Result<()> {
    let manifest = AssetManifest::load(&root.join("assets/manifest.json"))
        .context("reading assets/manifest.json")?;
    let problems = manifest.lint();
    for p in &problems {
        eprintln!("manifest: {}: {}", p.id, p.problem);
    }
    let assets_dir = root.join("assets");
    let (mut ok, mut missing, mut bad) = (0, 0, 0);
    for entry in &manifest.assets {
        match entry.status(&assets_dir) {
            AssetStatus::Ok => ok += 1,
            AssetStatus::Unverified => {
                eprintln!(
                    "unverified: {} (no hash recorded; run fetch_assets.py)",
                    entry.id
                );
                ok += 1;
            }
            AssetStatus::Missing => {
                if require_all && !entry.ultra_pack {
                    eprintln!("missing: {} ({})", entry.id, entry.path);
                }
                missing += 1;
            }
            AssetStatus::HashMismatch { expected, actual } => {
                eprintln!(
                    "hash mismatch: {} expected {expected} got {actual}",
                    entry.id
                );
                bad += 1;
            }
        }
    }
    println!(
        "{} assets: {ok} present, {missing} missing (procedural fallback), {bad} corrupt",
        manifest.assets.len()
    );
    let required_missing = require_all
        && manifest
            .assets
            .iter()
            .any(|e| !e.ultra_pack && e.status(&assets_dir) == AssetStatus::Missing);
    if !problems.is_empty() || bad > 0 || required_missing {
        bail!("asset verification failed");
    }
    Ok(())
}
