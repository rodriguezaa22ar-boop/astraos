use std::{path::PathBuf, time::Duration};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SystemSnapshot {
    pub operating_system: Option<String>,
    pub hostname: Option<String>,
    pub cpu: Option<CpuSnapshot>,
    pub memory: Option<MemorySnapshot>,
    pub disk: Option<DiskSnapshot>,
    pub battery: Option<BatterySnapshot>,
    pub uptime: Option<Duration>,
    pub services: DeveloperServicesSnapshot,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CpuSnapshot {
    pub usage_percent: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemorySnapshot {
    pub used_bytes: u64,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskSnapshot {
    pub mount_point: PathBuf,
    pub used_bytes: u64,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BatterySnapshot {
    pub charge_percent: f32,
    pub state: BatteryState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatteryState {
    Charging,
    Discharging,
    Empty,
    Full,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceStatus {
    Running,
    Stopped,
    Unavailable,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeveloperServicesSnapshot {
    pub docker: ServiceStatus,
    pub ollama: ServiceStatus,
}

impl Default for DeveloperServicesSnapshot {
    fn default() -> Self {
        Self {
            docker: ServiceStatus::Unknown,
            ollama: ServiceStatus::Unknown,
        }
    }
}
