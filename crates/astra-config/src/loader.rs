use crate::{
    error::ConfigError,
    model::Config,
    paths::{config_dir, config_path},
};
use std::fs;

pub fn load() -> Result<Config, ConfigError> {
    let path = config_path();

    if !path.exists() {
        let config = Config::default();
        save(&config)?;
        return Ok(config);
    }

    let contents = fs::read_to_string(&path).map_err(|source| ConfigError::Read {
        path: path.clone(),
        source,
    })?;

    toml::from_str(&contents).map_err(|source| ConfigError::Parse { path, source })
}

pub fn save(config: &Config) -> Result<(), ConfigError> {
    let directory = config_dir();
    fs::create_dir_all(&directory).map_err(|source| ConfigError::CreateDirectory {
        path: directory,
        source,
    })?;

    let path = config_path();
    let contents = toml::to_string_pretty(config)?;

    fs::write(&path, contents).map_err(|source| ConfigError::Write { path, source })
}
