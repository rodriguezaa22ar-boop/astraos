use std::path::PathBuf;

pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("astra")
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.toml")
}
