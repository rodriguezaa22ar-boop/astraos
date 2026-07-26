mod collector;
mod model;
mod services;

pub use collector::SystemCollector;
pub use model::{
    BatterySnapshot, BatteryState, CpuSnapshot, DeveloperServicesSnapshot, DiskSnapshot,
    MemorySnapshot, ServiceStatus, SystemSnapshot,
};

use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
};

pub fn command_exists(command: &str) -> bool {
    let command_path = Path::new(command);

    if command_path.components().count() > 1 {
        return is_executable(command_path);
    }

    env::var_os("PATH")
        .map(|path| command_exists_in_path(command, &path))
        .unwrap_or(false)
}

fn command_exists_in_path(command: &str, path: &std::ffi::OsStr) -> bool {
    env::split_paths(path)
        .map(|directory| executable_path(&directory, command))
        .any(|candidate| is_executable(&candidate))
}

fn executable_path(directory: &Path, command: &str) -> PathBuf {
    directory.join(command)
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    path.metadata()
        .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

pub fn command_output(command: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(command).args(args).output().ok()?;

    if !output.status.success() {
        return None;
    }

    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn command_exists_uses_path_without_spawning_a_shell() {
        let directory = tempfile::tempdir().expect("temporary directory should be created");
        let executable = directory.path().join("test-command");
        fs::write(&executable, "").expect("test command should be written");
        #[cfg(unix)]
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755))
            .expect("test command should be executable");

        assert!(command_exists_in_path(
            "test-command",
            directory.path().as_os_str()
        ));
        assert!(!command_exists_in_path(
            "missing-command",
            directory.path().as_os_str()
        ));
    }
}
