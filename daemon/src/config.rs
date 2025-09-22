use common::Profile;
use serde::Deserialize;
use std::{collections::HashMap, fs, path::PathBuf};
use tvix_serde::from_str;

#[derive(Deserialize, Debug, Default)]
pub struct Config {
    #[serde(default)]
    pub profiles: HashMap<String, Profile>,
}

impl Config {
    pub fn load(path: Option<PathBuf>) -> anyhow::Result<Config> {
        let config_path = if let Some(path) = path {
            path
        } else {
            Self::path()?
        };

        let nix_code = fs::read_to_string(&config_path)?;
        let config: Config =
            from_str(&nix_code).map_err(|e| anyhow::anyhow!("tvix_serde failed: {e:?}"))?;

        Ok(config)
    }

    pub fn path() -> anyhow::Result<PathBuf> {
        let config_dir = PathBuf::from("/etc");

        let config_dir_first = config_dir.join("wl-clicker-rs.nix");
        if config_dir_first.exists() {
            log::info!("Configuration found at {}", config_dir_first.display());
            return Ok(config_dir_first);
        } else {
            log::warn!("Configuration not found at {}", config_dir_first.display());
        }

        let config_dir_second = config_dir.join("wl-clicker-rs").join("default.nix");
        if config_dir_second.exists() {
            log::info!("Configuration found at {}", config_dir_second.display());
            Ok(config_dir_second)
        } else {
            log::error!(
                "Configuration not found at {} or {}",
                config_dir_first.display(),
                config_dir_second.display()
            );
            Err(anyhow::anyhow!(
                "Configuration not found at {} or {}",
                config_dir_first.display(),
                config_dir_second.display()
            ))
        }
    }
}
