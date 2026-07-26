use std::{env, path::PathBuf};

pub fn config_dir() -> PathBuf {
    if let Ok(path) = env::var("ASTRA_CONFIG_DIR") {
        return PathBuf::from(path);
    }

    if let Ok(path) = env::var("XDG_CONFIG_HOME") {
        return PathBuf::from(path).join("astra");
    }

    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("astra")
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.toml")
}
