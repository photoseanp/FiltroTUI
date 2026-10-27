//! Program configuration in TOML (`config.toml` next to `settings.conf`).
//!
//! [output]
//! dir = "/path/to/saved/measurements"

use std::fs;
use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct Output {
    /// Folder for saved measurement files (txt + png). Empty = not chosen yet.
    #[serde(default)]
    pub dir: String,
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct Config {
    #[serde(default)]
    pub output: Output,
}

impl Config {
    pub fn path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("filtrotui").join("config.toml"))
    }

    /// Missing or broken file = defaults.
    pub fn load() -> Config {
        Self::path()
            .and_then(|p| fs::read_to_string(p).ok())
            .and_then(|t| toml::from_str::<Config>(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> io::Result<()> {
        let p = Self::path()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no config directory"))?;
        if let Some(dir) = p.parent() {
            fs::create_dir_all(dir)?;
        }
        let text = toml::to_string_pretty(self)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
        fs::write(p, text)
    }
}
