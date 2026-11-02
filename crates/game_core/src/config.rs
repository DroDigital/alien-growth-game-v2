//! Player settings and graphics quality presets.
//!
//! `Settings` is the RON file the launcher writes and the game reads.
//! Quality presets live in `data/presets.ron`; per-setting overrides in
//! `Settings::quality.overrides` are applied on top of the chosen preset.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// Current on-disk settings schema version. Bump and add a migration in
/// [`Settings::migrate`] whenever the format changes incompatibly.
pub const SETTINGS_VERSION: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("I/O error on {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("RON parse error in {path}: {source}")]
    Ron {
        path: String,
        source: Box<ron::error::SpannedError>,
    },
    #[error("RON serialize error: {0}")]
    RonSer(Box<ron::Error>),
    #[error("preset table is missing preset {0:?}")]
    MissingPreset(QualityPreset),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum QualityPreset {
    Low,
    Medium,
    High,
    Ultra,
}

impl QualityPreset {
    pub const ALL: [QualityPreset; 4] = [Self::Low, Self::Medium, Self::High, Self::Ultra];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Ultra => "ultra",
        }
    }
}

impl std::str::FromStr for QualityPreset {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "low" => Ok(Self::Low),
            "medium" | "med" => Ok(Self::Medium),
            "high" => Ok(Self::High),
            "ultra" => Ok(Self::Ultra),
            other => Err(format!("unknown preset '{other}' (low|medium|high|ultra)")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AntiAliasing {
    Off,
    Fxaa,
    Smaa,
    Taa,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SsaoQuality {
    Off,
    Low,
    Medium,
    High,
    Ultra,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Upscaler {
    /// Bilinear upscale of the render-scaled image (with TAA when enabled).
    RenderScale,
    /// Reserved: only enabled once a maintained integration exists for the pinned Bevy.
    Dlss,
    /// Reserved: no maintained FSR integration for Bevy 0.20 yet.
    Fsr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextureQuality {
    /// 1K/2K base asset set.
    Base,
    /// 4K "Ultra texture pack", if installed.
    UltraPack,
}

/// Fully resolved render settings: a preset with overrides applied.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderSettings {
    /// Reference output resolution the preset is tuned for (width, height).
    pub reference_resolution: (u32, u32),
    /// Fraction of the output resolution the 3D scene is rendered at.
    pub render_scale: f32,
    pub upscaler: Upscaler,
    pub anti_aliasing: AntiAliasing,
    pub bloom: bool,
    pub ssao: SsaoQuality,
    pub volumetric_fog: bool,
    pub volumetric_fog_steps: u32,
    pub ssr: bool,
    pub depth_of_field: bool,
    pub motion_blur: bool,
    pub irradiance_volumes: bool,
    pub shadow_cascades: u32,
    pub shadow_map_size: u32,
    pub shadow_max_distance: f32,
    /// Multiplier on particle spawn rates and capacities.
    pub particle_density: f32,
    /// Multiplier on visibility-range / LOD distances.
    pub lod_distance_scale: f32,
    pub texture_quality: TextureQuality,
    /// Experimental ray-traced lighting (Bevy Solari). Only honored on GPUs
    /// with hardware ray tracing.
    pub ray_traced_lighting: bool,
}

/// Optional per-setting overrides layered on top of a preset.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct QualityOverrides {
    pub render_scale: Option<f32>,
    pub upscaler: Option<Upscaler>,
    pub anti_aliasing: Option<AntiAliasing>,
    pub bloom: Option<bool>,
    pub ssao: Option<SsaoQuality>,
    pub volumetric_fog: Option<bool>,
    pub volumetric_fog_steps: Option<u32>,
    pub ssr: Option<bool>,
    pub depth_of_field: Option<bool>,
    pub motion_blur: Option<bool>,
    pub irradiance_volumes: Option<bool>,
    pub shadow_cascades: Option<u32>,
    pub shadow_map_size: Option<u32>,
    pub shadow_max_distance: Option<f32>,
    pub particle_density: Option<f32>,
    pub lod_distance_scale: Option<f32>,
    pub texture_quality: Option<TextureQuality>,
    pub ray_traced_lighting: Option<bool>,
}

impl RenderSettings {
    /// Resolution the 3D scene renders at for a given output (window)
    /// resolution: the output is first capped to the preset's reference height
    /// (keeping aspect ratio), then multiplied by `render_scale`. This keeps
    /// GPU cost tied to the preset rather than the display (a 2560×1600 Retina
    /// panel on Low renders like 1080p Low), and the result is upscaled to the
    /// output.
    pub fn render_resolution(&self, output: (u32, u32)) -> (u32, u32) {
        let (ow, oh) = (output.0.max(1) as f32, output.1.max(1) as f32);
        let cap = (self.reference_resolution.1 as f32 / oh).min(1.0);
        let s = cap * self.render_scale;
        (
            ((ow * s).round() as u32).max(1),
            ((oh * s).round() as u32).max(1),
        )
    }

    /// Returns a copy with every `Some` override applied and values clamped to
    /// safe ranges.
    pub fn with_overrides(&self, o: &QualityOverrides) -> RenderSettings {
        macro_rules! pick {
            ($field:ident) => {
                o.$field.unwrap_or(self.$field)
            };
        }
        RenderSettings {
            reference_resolution: self.reference_resolution,
            render_scale: pick!(render_scale).clamp(0.25, 2.0),
            upscaler: pick!(upscaler),
            anti_aliasing: pick!(anti_aliasing),
            bloom: pick!(bloom),
            ssao: pick!(ssao),
            volumetric_fog: pick!(volumetric_fog),
            volumetric_fog_steps: pick!(volumetric_fog_steps).clamp(8, 256),
            ssr: pick!(ssr),
            depth_of_field: pick!(depth_of_field),
            motion_blur: pick!(motion_blur),
            irradiance_volumes: pick!(irradiance_volumes),
            shadow_cascades: pick!(shadow_cascades).clamp(1, 4),
            shadow_map_size: pick!(shadow_map_size).clamp(256, 8192).next_power_of_two(),
            shadow_max_distance: pick!(shadow_max_distance).max(1.0),
            particle_density: pick!(particle_density).clamp(0.0, 4.0),
            lod_distance_scale: pick!(lod_distance_scale).clamp(0.1, 4.0),
            texture_quality: pick!(texture_quality),
            ray_traced_lighting: pick!(ray_traced_lighting),
        }
    }
}

/// The table in `data/presets.ron`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetTable {
    pub presets: BTreeMap<QualityPreset, RenderSettings>,
}

impl PresetTable {
    pub fn from_ron_str(src: &str, path_for_errors: &str) -> Result<Self, ConfigError> {
        let table: PresetTable = ron::from_str(src).map_err(|source| ConfigError::Ron {
            path: path_for_errors.to_string(),
            source: Box::new(source),
        })?;
        for p in QualityPreset::ALL {
            if !table.presets.contains_key(&p) {
                return Err(ConfigError::MissingPreset(p));
            }
        }
        Ok(table)
    }

    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let src = std::fs::read_to_string(path).map_err(|source| ConfigError::Io {
            path: path.display().to_string(),
            source,
        })?;
        Self::from_ron_str(&src, &path.display().to_string())
    }

    pub fn get(&self, preset: QualityPreset) -> &RenderSettings {
        // Validated in `from_ron_str`: every preset is present.
        &self.presets[&preset]
    }

    /// Resolve the effective render settings for the given player settings.
    pub fn resolve(&self, quality: &QualitySettings) -> RenderSettings {
        let mut s = self.get(quality.preset).with_overrides(&quality.overrides);
        if !quality.motion_blur_allowed {
            s.motion_blur = false;
        }
        s
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WindowMode {
    Windowed,
    BorderlessFullscreen,
    Fullscreen,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DisplaySettings {
    pub mode: WindowMode,
    /// `None` uses the monitor's native resolution.
    pub resolution: Option<(u32, u32)>,
    pub vsync: bool,
    /// 0 = uncapped.
    pub frame_cap: u32,
    /// Monitor index; `None` = primary.
    pub monitor: Option<usize>,
}

impl Default for DisplaySettings {
    fn default() -> Self {
        Self {
            mode: WindowMode::Windowed,
            // None: Bevy's default window size (logical, so it is sensible on
            // HiDPI displays). Some((w, h)) is in physical pixels.
            resolution: None,
            vsync: true,
            frame_cap: 0,
            monitor: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct QualitySettings {
    pub preset: QualityPreset,
    pub overrides: QualityOverrides,
    /// Accessibility: when false, motion blur is forced off regardless of preset.
    pub motion_blur_allowed: bool,
}

impl Default for QualitySettings {
    fn default() -> Self {
        Self {
            preset: QualityPreset::Low,
            overrides: QualityOverrides::default(),
            motion_blur_allowed: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioSettings {
    /// `None` = system default output device.
    pub device: Option<String>,
    pub master: f32,
    pub music: f32,
    pub sfx: f32,
    pub ambience: f32,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            device: None,
            master: 0.8,
            music: 0.7,
            sfx: 0.8,
            ambience: 0.8,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColorblindMode {
    Off,
    Protanopia,
    Deuteranopia,
    Tritanopia,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AccessibilitySettings {
    pub camera_shake: bool,
    pub colorblind_mode: ColorblindMode,
    pub subtitles: bool,
    pub sfx_captions: bool,
    pub ui_scale: f32,
    /// Assist mode: game speed multiplier. Any value below 1.0 disables leaderboards.
    pub game_speed: f32,
    pub controller_rumble: bool,
}

impl Default for AccessibilitySettings {
    fn default() -> Self {
        Self {
            camera_shake: true,
            colorblind_mode: ColorblindMode::Off,
            subtitles: true,
            sfx_captions: false,
            ui_scale: 1.0,
            game_speed: 1.0,
            controller_rumble: true,
        }
    }
}

impl AccessibilitySettings {
    pub fn assist_mode_active(&self) -> bool {
        (self.game_speed - 1.0).abs() > f32::EPSILON
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GraphicsBackend {
    Auto,
    Dx12,
    Vulkan,
    Metal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AfterLaunch {
    StayOpen,
    Minimize,
    Close,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LauncherSettings {
    pub launch_directly: bool,
    pub backend: GraphicsBackend,
    pub custom_args: String,
    pub after_launch: AfterLaunch,
    /// Driver/GPU fingerprint of the last shader warm-up; re-run on change.
    pub warmup_fingerprint: Option<String>,
}

impl Default for LauncherSettings {
    fn default() -> Self {
        Self {
            launch_directly: false,
            backend: GraphicsBackend::Auto,
            custom_args: String::new(),
            after_launch: AfterLaunch::Minimize,
            warmup_fingerprint: None,
        }
    }
}

/// The whole settings file (`settings.ron` in the user config directory).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub version: u32,
    pub language: String,
    pub display: DisplaySettings,
    pub quality: QualitySettings,
    pub audio: AudioSettings,
    pub accessibility: AccessibilitySettings,
    pub launcher: LauncherSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            language: crate::locale::DEFAULT_LANGUAGE.to_string(),
            display: DisplaySettings::default(),
            quality: QualitySettings::default(),
            audio: AudioSettings::default(),
            accessibility: AccessibilitySettings::default(),
            launcher: LauncherSettings::default(),
        }
    }
}

impl Settings {
    pub fn from_ron_str(src: &str) -> Result<Self, ConfigError> {
        let s: Settings = ron::from_str(src).map_err(|source| ConfigError::Ron {
            path: "settings.ron".into(),
            source: Box::new(source),
        })?;
        Ok(s.migrate())
    }

    pub fn to_ron_string(&self) -> Result<String, ConfigError> {
        let pretty = ron::ser::PrettyConfig::new().depth_limit(4);
        ron::ser::to_string_pretty(self, pretty).map_err(|e| ConfigError::RonSer(Box::new(e)))
    }

    /// Upgrade older settings files in place. Unknown/missing fields already
    /// fall back to defaults through `#[serde(default)]`.
    fn migrate(mut self) -> Self {
        // Version 0 (pre-release) -> 1: no structural changes.
        if self.version < SETTINGS_VERSION {
            self.version = SETTINGS_VERSION;
        }
        self
    }

    /// Loads settings, falling back to defaults when the file is missing.
    /// A corrupt file is reported as an error so the caller can back it up.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        match std::fs::read_to_string(path) {
            Ok(src) => Self::from_ron_str(&src).map_err(|e| match e {
                ConfigError::Ron { source, .. } => ConfigError::Ron {
                    path: path.display().to_string(),
                    source,
                },
                other => other,
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(source) => Err(ConfigError::Io {
                path: path.display().to_string(),
                source,
            }),
        }
    }

    /// Writes atomically (temp file + rename) so a crash never leaves a
    /// half-written settings file.
    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        let io = |source| ConfigError::Io {
            path: path.display().to_string(),
            source,
        };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(io)?;
        }
        let tmp = path.with_extension("ron.tmp");
        std::fs::write(&tmp, self.to_ron_string()?).map_err(io)?;
        std::fs::rename(&tmp, path).map_err(io)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> PresetTable {
        let src = include_str!("../../../data/presets.ron");
        PresetTable::from_ron_str(src, "data/presets.ron").expect("presets.ron parses")
    }

    #[test]
    fn shipped_presets_parse_and_are_ordered_by_cost() {
        let t = table();
        let low = t.get(QualityPreset::Low);
        let ultra = t.get(QualityPreset::Ultra);
        assert!(low.render_scale < 1.0, "Low renders below native");
        assert!(!low.volumetric_fog && !low.ssr, "Low skips Medium+ effects");
        assert!(t.get(QualityPreset::Medium).volumetric_fog);
        assert!(ultra.shadow_cascades >= low.shadow_cascades);
        assert!(ultra.particle_density >= low.particle_density);
    }

    #[test]
    fn render_resolution_caps_to_reference_then_scales() {
        let t = table();
        let low = t.get(QualityPreset::Low); // 1080p reference, 0.75 scale
        assert_eq!(low.render_resolution((1920, 1080)), (1440, 810));
        // Retina 16:10 panel: capped to 1080 lines first.
        assert_eq!(low.render_resolution((2560, 1600)), (1296, 810));
        // Smaller window than the reference: no upscaling of the cap.
        assert_eq!(low.render_resolution((1280, 720)), (960, 540));
        let ultra = t.get(QualityPreset::Ultra); // 4K reference, 0.67 scale
        assert_eq!(ultra.render_resolution((3840, 2160)), (2573, 1447));
        assert_eq!(low.render_resolution((0, 0)), (1, 1));
    }

    #[test]
    fn overrides_apply_and_clamp() {
        let t = table();
        let q = QualitySettings {
            preset: QualityPreset::Low,
            overrides: QualityOverrides {
                render_scale: Some(9.0),
                bloom: Some(false),
                shadow_map_size: Some(1000),
                ..Default::default()
            },
            motion_blur_allowed: true,
        };
        let s = t.resolve(&q);
        assert_eq!(s.render_scale, 2.0);
        assert!(!s.bloom);
        assert_eq!(s.shadow_map_size, 1024);
    }

    #[test]
    fn accessibility_forces_motion_blur_off() {
        let t = table();
        let q = QualitySettings {
            preset: QualityPreset::Ultra,
            overrides: QualityOverrides::default(),
            motion_blur_allowed: false,
        };
        assert!(!t.resolve(&q).motion_blur);
    }

    #[test]
    fn settings_round_trip_and_tolerate_missing_fields() {
        let s = Settings::default();
        let text = s.to_ron_string().unwrap();
        assert_eq!(Settings::from_ron_str(&text).unwrap(), s);

        let partial = "(version: 0, language: \"de-DE\")";
        let p = Settings::from_ron_str(partial).unwrap();
        assert_eq!(p.version, SETTINGS_VERSION);
        assert_eq!(p.language, "de-DE");
        assert_eq!(p.display, DisplaySettings::default());
    }

    #[test]
    fn settings_save_load() {
        let dir = std::env::temp_dir().join(format!("agg-settings-{}", std::process::id()));
        let path = dir.join("settings.ron");
        let mut s = Settings::default();
        s.quality.preset = QualityPreset::High;
        s.save(&path).unwrap();
        assert_eq!(Settings::load(&path).unwrap(), s);
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(Settings::load(&path).unwrap(), Settings::default());
    }

    #[test]
    fn preset_from_str() {
        assert_eq!("LOW".parse::<QualityPreset>(), Ok(QualityPreset::Low));
        assert!("potato".parse::<QualityPreset>().is_err());
    }
}
