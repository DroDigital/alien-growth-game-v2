//! Fluent (`.ftl`) localization shared by the game and the launcher.
//!
//! Files live in `assets/locale/<lang>/*.ftl`. Lookups fall back to
//! [`DEFAULT_LANGUAGE`] and finally to the message id itself, so a missing
//! string is visible but never crashes.

use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::{FluentArgs, FluentResource};
use std::path::Path;
use unic_langid::LanguageIdentifier;

pub use fluent_bundle::FluentValue;

pub const DEFAULT_LANGUAGE: &str = "en-US";

#[derive(Debug, thiserror::Error)]
pub enum LocaleError {
    #[error("invalid language id '{0}'")]
    BadLanguage(String),
    #[error("I/O error reading {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("Fluent syntax errors in {path}: {errors}")]
    Syntax { path: String, errors: String },
    #[error("duplicate Fluent message in {path}: {errors}")]
    Duplicate { path: String, errors: String },
}

type Bundle = FluentBundle<FluentResource>;

/// A language bundle plus the default-language fallback bundle.
pub struct Localizer {
    language: String,
    primary: Bundle,
    fallback: Option<Bundle>,
}

impl std::fmt::Debug for Localizer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Localizer")
            .field("language", &self.language)
            .finish_non_exhaustive()
    }
}

fn new_bundle(lang: &str) -> Result<Bundle, LocaleError> {
    let id: LanguageIdentifier = lang
        .parse()
        .map_err(|_| LocaleError::BadLanguage(lang.to_string()))?;
    let mut bundle = Bundle::new_concurrent(vec![id]);
    // Unicode isolation marks around placeables render as boxes in some fonts.
    bundle.set_use_isolating(false);
    Ok(bundle)
}

fn add_source(bundle: &mut Bundle, src: String, path: &str) -> Result<(), LocaleError> {
    let res = FluentResource::try_new(src).map_err(|(_, errs)| LocaleError::Syntax {
        path: path.to_string(),
        errors: format!("{errs:?}"),
    })?;
    bundle
        .add_resource(res)
        .map_err(|errs| LocaleError::Duplicate {
            path: path.to_string(),
            errors: format!("{errs:?}"),
        })
}

fn load_dir(bundle: &mut Bundle, dir: &Path) -> Result<usize, LocaleError> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(0);
    };
    let mut files: Vec<_> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "ftl"))
        .collect();
    files.sort();
    for path in &files {
        let src = std::fs::read_to_string(path).map_err(|source| LocaleError::Io {
            path: path.display().to_string(),
            source,
        })?;
        add_source(bundle, src, &path.display().to_string())?;
    }
    Ok(files.len())
}

impl Localizer {
    /// Load `<locale_root>/<language>/*.ftl`, with `en-US` as fallback.
    pub fn load(locale_root: &Path, language: &str) -> Result<Self, LocaleError> {
        let mut primary = new_bundle(language)?;
        let found = load_dir(&mut primary, &locale_root.join(language))?;
        let (language, primary) = if found == 0 && language != DEFAULT_LANGUAGE {
            // Unknown language: use the default as primary.
            let mut b = new_bundle(DEFAULT_LANGUAGE)?;
            load_dir(&mut b, &locale_root.join(DEFAULT_LANGUAGE))?;
            (DEFAULT_LANGUAGE.to_string(), b)
        } else {
            (language.to_string(), primary)
        };
        let fallback = if language != DEFAULT_LANGUAGE {
            let mut b = new_bundle(DEFAULT_LANGUAGE)?;
            load_dir(&mut b, &locale_root.join(DEFAULT_LANGUAGE))?;
            Some(b)
        } else {
            None
        };
        Ok(Self {
            language,
            primary,
            fallback,
        })
    }

    /// Build from in-memory sources (tests, embedded strings).
    pub fn from_sources(language: &str, primary: &[&str], fallback: &[&str]) -> Self {
        let mut p = new_bundle(language).expect("valid language id");
        for s in primary {
            add_source(&mut p, s.to_string(), "<memory>").expect("valid ftl");
        }
        let fallback = (!fallback.is_empty()).then(|| {
            let mut b = new_bundle(DEFAULT_LANGUAGE).expect("valid language id");
            for s in fallback {
                add_source(&mut b, s.to_string(), "<memory>").expect("valid ftl");
            }
            b
        });
        Self {
            language: language.to_string(),
            primary: p,
            fallback,
        }
    }

    pub fn language(&self) -> &str {
        &self.language
    }

    pub fn has(&self, id: &str) -> bool {
        self.primary.has_message(id) || self.fallback.as_ref().is_some_and(|b| b.has_message(id))
    }

    /// Look up a message. Returns the id itself if no bundle has it.
    pub fn get(&self, id: &str) -> String {
        self.format(id, None)
    }

    pub fn get_args(&self, id: &str, args: &[(&str, FluentValue<'_>)]) -> String {
        let mut fa = FluentArgs::new();
        for (k, v) in args {
            fa.set(*k, v.clone());
        }
        self.format(id, Some(&fa))
    }

    fn format(&self, id: &str, args: Option<&FluentArgs>) -> String {
        for bundle in std::iter::once(&self.primary).chain(self.fallback.as_ref()) {
            if let Some(pattern) = bundle.get_message(id).and_then(|m| m.value()) {
                let mut errors = Vec::new();
                return bundle
                    .format_pattern(pattern, args, &mut errors)
                    .into_owned();
            }
        }
        id.to_string()
    }
}

/// Lists language folders available under `locale_root`.
pub fn available_languages(locale_root: &Path) -> Vec<String> {
    let mut langs: Vec<String> = std::fs::read_dir(locale_root)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|name| name.parse::<LanguageIdentifier>().is_ok())
        .collect();
    langs.sort();
    langs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_with_fallback_and_args() {
        let l = Localizer::from_sources(
            "de-DE",
            &["menu-play = Spielen\n"],
            &["menu-play = Play\nmenu-quit = Quit\nhud-biomass = Biomass: { $value }\n"],
        );
        assert_eq!(l.get("menu-play"), "Spielen");
        assert_eq!(l.get("menu-quit"), "Quit");
        assert_eq!(
            l.get_args("hud-biomass", &[("value", FluentValue::from(12))]),
            "Biomass: 12"
        );
        assert_eq!(l.get("does-not-exist"), "does-not-exist");
    }

    #[test]
    fn shipped_english_strings_load() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/locale");
        let l = Localizer::load(&root, DEFAULT_LANGUAGE).unwrap();
        assert!(l.has("menu-play"));
        assert!(l.has("launcher-play"));
        assert!(available_languages(&root).contains(&DEFAULT_LANGUAGE.to_string()));
        // Unknown language falls back to English instead of failing.
        let l = Localizer::load(&root, "xx-XX").unwrap();
        assert_eq!(l.language(), DEFAULT_LANGUAGE);
    }
}
