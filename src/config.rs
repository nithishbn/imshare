use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    pub public_domain: String,
    pub default_ttl: String,
    pub db_path: String,
    pub upstream: String,
    pub verify_port: u16,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            public_domain: "pub.nith.sh".to_string(),
            default_ttl: "30d".to_string(),
            db_path: "~/.local/share/imshare/links.db".to_string(),
            upstream: "http://localhost:3000".to_string(),
            verify_port: 3001,
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path();

        if !config_path.exists() {
            // Create default config
            let config = Config::default();
            config.save(&config_path)?;
            return Ok(config);
        }

        let contents =
            std::fs::read_to_string(&config_path).context("Failed to read config file")?;
        let config: Config = toml::from_str(&contents).context("Failed to parse config file")?;

        Ok(config)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let contents = toml::to_string_pretty(self)?;
        std::fs::write(path, contents)?;

        Ok(())
    }

    pub fn config_path() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home).join(".config/imshare/config.toml")
    }

    pub fn db_path(&self) -> PathBuf {
        expand_tilde(&self.db_path)
    }
}

fn expand_tilde(path: &str) -> PathBuf {
    if path.starts_with("~/") {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home).join(&path[2..])
    } else {
        PathBuf::from(path)
    }
}
