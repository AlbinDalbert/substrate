use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;

/// `.substrate/config.toml`. Every field is optional; a folder without one works on defaults.
#[derive(Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub daily: Daily,
    pub inbox: Inbox,
}

#[derive(Deserialize)]
#[serde(default)]
pub struct Daily {
    pub dir: String,
    /// chrono strftime pattern
    pub filename: String,
    /// path relative to `.substrate/`; `{date}` in it gets replaced
    pub template: Option<String>,
}

#[derive(Deserialize)]
#[serde(default)]
pub struct Inbox {
    pub dir: String,
    /// chrono strftime pattern
    pub filename: String,
}

impl Default for Daily {
    fn default() -> Self {
        Self {
            dir: "daily".into(),
            filename: "%Y-%m-%d.md".into(),
            template: None,
        }
    }
}

impl Default for Inbox {
    fn default() -> Self {
        Self {
            dir: "inbox".into(),
            filename: "%Y-%m-%dT%H%M%S.md".into(),
        }
    }
}

impl Config {
    pub fn load(root: &Path) -> Result<Self> {
        let path = root.join(".substrate/config.toml");
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(&path)?;
        toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))
    }
}
