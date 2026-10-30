use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;

/// Relative to a substrate's folder.
pub const CONFIG: &str = ".substrate/config.toml";

/// `.substrate/config.toml`. Its existence is what makes a folder a substrate.
/// A feature is on when its section is present.
#[derive(Deserialize)]
pub struct Config {
    /// IANA name, e.g. "Europe/Stockholm". Missing: the machine's local time.
    pub timezone: Option<String>,
    pub scratch: Option<Scratch>,
    pub inbox: Option<Inbox>,
}

#[derive(Deserialize)]
#[serde(default)]
pub struct Scratch {
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

impl Default for Scratch {
    fn default() -> Self {
        Self {
            dir: "scratch".into(),
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

/// Everything on, like a fresh `init`.
impl Default for Config {
    fn default() -> Self {
        Self {
            timezone: None,
            scratch: Some(Scratch::default()),
            inbox: Some(Inbox::default()),
        }
    }
}

impl Config {
    pub fn load(root: &Path) -> Result<Self> {
        let path = root.join(CONFIG);
        let text = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))
    }
}
