//! File config at `~/.config/prboard/config.toml` (or `$XDG_CONFIG_HOME`).
//!
//! This is what makes Spotlight/Finder launches work: those carry no shell
//! env and start in `/`, so env vars and cwd-based repo detection both fail
//! there. Precedence everywhere: CLI arg > env var > config file > detection.

use std::path::PathBuf;

use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
pub struct FileConfig {
    /// Default repo (`owner/name`) when neither `--repo` nor `PRBOARD_REPO` is set.
    pub repo: Option<String>,
    /// Entries for the repo picker; the active repo is always included.
    #[serde(default)]
    pub repos: Vec<String>,
    pub refresh_secs: Option<u64>,
    /// `system` | `light` | `dark`.
    pub theme: Option<String>,
    #[serde(default)]
    pub default_reviewers: Vec<String>,
    pub issue_link: Option<IssueLinkSection>,
}

#[derive(Debug, Deserialize)]
pub struct IssueLinkSection {
    pub pattern: String,
    pub url_template: String,
}

pub fn config_path() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| dirs::home_dir().map(|h| h.join(".config")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("prboard")
        .join("config.toml")
}

/// Missing file → defaults; unparseable file → defaults with a warning
/// (a typo in the config must never make the app unlaunchable).
pub fn load() -> FileConfig {
    let path = config_path();
    let Ok(text) = std::fs::read_to_string(&path) else {
        return FileConfig::default();
    };
    match toml::from_str(&text) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("prboard: ignoring invalid {}: {e}", path.display());
            FileConfig::default()
        }
    }
}
