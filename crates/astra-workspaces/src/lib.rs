use std::{env, path::PathBuf};

pub fn astra_root() -> PathBuf {
    env::var_os("ASTRA_ROOT")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join("Developer")))
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn workspace_path(name: &str) -> Option<PathBuf> {
    let root = astra_root();

    match name {
        "omnia" => Some(root.join("astraeus-omnia")),
        "api" => Some(root.join("omnia-api-foundry")),
        "games" => Some(root.join("games")),
        "cyber" => Some(root.join("cybersecurity")),
        "ai" => Some(root.join("ai")),
        "learning" => Some(root.join("learning")),
        "projects" => Some(root.join("projects")),
        _ => None,
    }
}
