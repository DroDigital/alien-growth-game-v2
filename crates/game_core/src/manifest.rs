//! The asset manifest (`assets/manifest.json`).
//!
//! Every third-party asset is listed with its source, author, license and
//! SHA-256. `tools/fetch_assets.py` writes it; `agtool verify-assets` checks
//! it in CI; the game reads it to decide between a real asset and its
//! procedural fallback.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;

pub const MANIFEST_VERSION: u32 = 1;

/// Licenses we accept for bundled assets. Anything else fails verification.
pub const ALLOWED_LICENSES: &[&str] = &["CC0-1.0", "CC-BY-4.0", "MIT", "OFL-1.1"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetKind {
    Hdri,
    Texture,
    Model,
    Sfx,
    Music,
    Font,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetEntry {
    /// Stable id the game uses to look the asset up, e.g. `hdri.underwater_sky`.
    pub id: String,
    pub kind: AssetKind,
    /// Path of the imported (converted) file, relative to `assets/`.
    pub path: String,
    /// Direct download URL of the original file.
    pub source_url: String,
    /// Human-facing page for attribution.
    pub page_url: String,
    pub author: String,
    /// SPDX identifier.
    pub license: String,
    /// SHA-256 of the downloaded original (hex). `None` until first fetch.
    pub source_sha256: Option<String>,
    /// SHA-256 of the imported file at `path` (hex). `None` until first fetch.
    pub sha256: Option<String>,
    /// Name of the procedural fallback used when the file is missing.
    pub fallback: String,
    /// Part of the optional Ultra texture pack rather than the base set.
    #[serde(default)]
    pub ultra_pack: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetManifest {
    pub version: u32,
    pub assets: Vec<AssetEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetStatus {
    /// File present and hash matches.
    Ok,
    /// Not downloaded yet; the procedural fallback will be used.
    Missing,
    /// Present but the hash differs from the manifest.
    HashMismatch { expected: String, actual: String },
    /// Present but the manifest has no recorded hash yet.
    Unverified,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestProblem {
    pub id: String,
    pub problem: String,
}

impl AssetManifest {
    pub fn from_json(src: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(src)
    }

    pub fn load(path: &Path) -> std::io::Result<Self> {
        let src = std::fs::read_to_string(path)?;
        Self::from_json(&src).map_err(std::io::Error::other)
    }

    pub fn get(&self, id: &str) -> Option<&AssetEntry> {
        self.assets.iter().find(|a| a.id == id)
    }

    /// Static checks that do not need the files: licenses, unique ids,
    /// required attribution fields.
    pub fn lint(&self) -> Vec<ManifestProblem> {
        let mut problems = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for a in &self.assets {
            let mut push = |p: String| {
                problems.push(ManifestProblem {
                    id: a.id.clone(),
                    problem: p,
                })
            };
            if !seen.insert(a.id.as_str()) {
                push("duplicate id".into());
            }
            if !ALLOWED_LICENSES.contains(&a.license.as_str()) {
                push(format!("license '{}' is not in the allowlist", a.license));
            }
            if a.author.trim().is_empty() {
                push("missing author".into());
            }
            if !a.source_url.starts_with("https://") {
                push("source_url must be https".into());
            }
            if a.fallback.trim().is_empty() {
                push("missing procedural fallback".into());
            }
            if a.path.contains("..") || Path::new(&a.path).is_absolute() {
                push("path must be relative to assets/ without '..'".into());
            }
        }
        problems
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    Ok(sha256_hex(&std::fs::read(path)?))
}

impl AssetEntry {
    pub fn status(&self, assets_dir: &Path) -> AssetStatus {
        let file = assets_dir.join(&self.path);
        if !file.is_file() {
            return AssetStatus::Missing;
        }
        let Some(expected) = &self.sha256 else {
            return AssetStatus::Unverified;
        };
        match sha256_file(&file) {
            Ok(actual) if &actual == expected => AssetStatus::Ok,
            Ok(actual) => AssetStatus::HashMismatch {
                expected: expected.clone(),
                actual,
            },
            Err(_) => AssetStatus::Missing,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_known_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn shipped_manifest_is_valid() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/manifest.json");
        let m = AssetManifest::load(&path).expect("manifest parses");
        assert_eq!(m.version, MANIFEST_VERSION);
        assert!(m.lint().is_empty(), "{:?}", m.lint());
    }

    #[test]
    fn lint_flags_bad_entries() {
        let entry = AssetEntry {
            id: "x".into(),
            kind: AssetKind::Texture,
            path: "../escape.png".into(),
            source_url: "http://insecure".into(),
            page_url: String::new(),
            author: String::new(),
            license: "CC-BY-NC-4.0".into(),
            source_sha256: None,
            sha256: None,
            fallback: String::new(),
            ultra_pack: false,
        };
        let m = AssetManifest {
            version: 1,
            assets: vec![entry.clone(), entry],
        };
        let problems: Vec<_> = m.lint().into_iter().map(|p| p.problem).collect();
        assert!(problems.iter().any(|p| p.contains("license")));
        assert!(problems.iter().any(|p| p.contains("duplicate")));
        assert!(problems.iter().any(|p| p.contains("https")));
        assert!(problems.iter().any(|p| p.contains("author")));
        assert!(problems.iter().any(|p| p.contains("fallback")));
        assert!(problems.iter().any(|p| p.contains("relative")));
    }

    #[test]
    fn status_detects_missing_and_mismatch() {
        let dir = std::env::temp_dir().join(format!("agg-manifest-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.bin"), b"abc").unwrap();
        let mut e = AssetEntry {
            id: "a".into(),
            kind: AssetKind::Texture,
            path: "a.bin".into(),
            source_url: "https://example.org/a".into(),
            page_url: "https://example.org".into(),
            author: "me".into(),
            license: "CC0-1.0".into(),
            source_sha256: None,
            sha256: None,
            fallback: "flat".into(),
            ultra_pack: false,
        };
        assert_eq!(e.status(&dir), AssetStatus::Unverified);
        e.sha256 = Some(sha256_hex(b"abc"));
        assert_eq!(e.status(&dir), AssetStatus::Ok);
        e.sha256 = Some("00".into());
        assert!(matches!(e.status(&dir), AssetStatus::HashMismatch { .. }));
        e.path = "nope.bin".into();
        assert_eq!(e.status(&dir), AssetStatus::Missing);
        std::fs::remove_dir_all(&dir).ok();
    }
}
