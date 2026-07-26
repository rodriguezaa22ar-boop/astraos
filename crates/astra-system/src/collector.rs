use crate::{
    services::DeveloperServicesCollector, BatterySnapshot, BatteryState, CpuSnapshot, DiskSnapshot,
    MemorySnapshot, SystemSnapshot,
};
use starship_battery::{units::ratio::percent, Manager, State};
use std::{
    path::Path,
    time::{Duration, Instant},
};
use sysinfo::{Disks, System, IS_SUPPORTED_SYSTEM, MINIMUM_CPU_UPDATE_INTERVAL};

const SERVICE_REFRESH_INTERVAL: Duration = Duration::from_secs(5);

pub struct SystemCollector {
    system: System,
    disks: Disks,
    battery_manager: Option<Manager>,
    services: DeveloperServicesCollector,
    last_cpu_refresh: Instant,
    last_service_refresh: Option<Instant>,
    last_services: crate::DeveloperServicesSnapshot,
}

impl SystemCollector {
    pub fn new() -> Self {
        let mut system = System::new();
        system.refresh_memory();
        system.refresh_cpu_usage();

        Self {
            system,
            disks: Disks::new_with_refreshed_list(),
            battery_manager: Manager::new().ok(),
            services: DeveloperServicesCollector::default(),
            last_cpu_refresh: Instant::now(),
            last_service_refresh: None,
            last_services: crate::DeveloperServicesSnapshot::default(),
        }
    }

    pub fn refresh(&mut self, filesystem_path: &Path) -> SystemSnapshot {
        self.refresh_at(filesystem_path, Instant::now(), false)
    }

    pub fn refresh_now(&mut self, filesystem_path: &Path) -> SystemSnapshot {
        self.refresh_at(filesystem_path, Instant::now(), true)
    }

    fn refresh_at(
        &mut self,
        filesystem_path: &Path,
        now: Instant,
        force_services: bool,
    ) -> SystemSnapshot {
        self.system.refresh_memory();

        let cpu = if now.duration_since(self.last_cpu_refresh) >= MINIMUM_CPU_UPDATE_INTERVAL {
            self.system.refresh_cpu_usage();
            self.last_cpu_refresh = now;
            Some(CpuSnapshot {
                usage_percent: self.system.global_cpu_usage().clamp(0.0, 100.0),
            })
        } else {
            None
        };

        self.disks.refresh(true);

        if force_services
            || self
                .last_service_refresh
                .is_none_or(|last| now.duration_since(last) >= SERVICE_REFRESH_INTERVAL)
        {
            self.last_services = self.services.collect();
            self.last_service_refresh = Some(now);
        }

        SystemSnapshot {
            operating_system: nonempty(System::long_os_version().or_else(System::name)),
            hostname: nonempty(System::host_name()),
            cpu,
            memory: memory_snapshot(&self.system),
            disk: disk_snapshot(&self.disks, filesystem_path),
            battery: battery_snapshot(self.battery_manager.as_ref()),
            uptime: uptime(),
            services: self.last_services,
        }
    }
}

impl Default for SystemCollector {
    fn default() -> Self {
        Self::new()
    }
}

fn nonempty(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.trim().is_empty())
}

fn memory_snapshot(system: &System) -> Option<MemorySnapshot> {
    let total_bytes = system.total_memory();

    (total_bytes > 0).then(|| MemorySnapshot {
        used_bytes: system.used_memory().min(total_bytes),
        total_bytes,
    })
}

fn disk_snapshot(disks: &Disks, filesystem_path: &Path) -> Option<DiskSnapshot> {
    disks
        .list()
        .iter()
        .filter(|disk| filesystem_path.starts_with(disk.mount_point()))
        .max_by_key(|disk| disk.mount_point().components().count())
        .or_else(|| {
            disks
                .list()
                .iter()
                .find(|disk| disk.mount_point() == Path::new("/"))
        })
        .map(|disk| {
            let total_bytes = disk.total_space();

            DiskSnapshot {
                mount_point: disk.mount_point().to_path_buf(),
                used_bytes: total_bytes.saturating_sub(disk.available_space()),
                total_bytes,
            }
        })
        .filter(|snapshot| snapshot.total_bytes > 0)
}

fn battery_snapshot(manager: Option<&Manager>) -> Option<BatterySnapshot> {
    let batteries = manager?.batteries().ok()?;

    batteries.filter_map(Result::ok).find_map(|battery| {
        battery_snapshot_from_values(battery.state_of_charge().get::<percent>(), battery.state())
    })
}

fn battery_snapshot_from_values(charge_percent: f32, state: State) -> Option<BatterySnapshot> {
    charge_percent.is_finite().then(|| BatterySnapshot {
        charge_percent: charge_percent.clamp(0.0, 100.0),
        state: match state {
            State::Charging => BatteryState::Charging,
            State::Discharging => BatteryState::Discharging,
            State::Empty => BatteryState::Empty,
            State::Full => BatteryState::Full,
            State::Unknown => BatteryState::Unknown,
        },
    })
}

fn uptime() -> Option<Duration> {
    if !IS_SUPPORTED_SYSTEM {
        return None;
    }

    Some(Duration::from_secs(System::uptime()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_battery_manager_is_a_normal_absence() {
        assert_eq!(battery_snapshot(None), None);
    }

    #[test]
    fn invalid_battery_percentage_is_unavailable() {
        assert_eq!(
            battery_snapshot_from_values(f32::NAN, State::Charging),
            None
        );
    }

    #[test]
    fn battery_values_are_clamped_and_state_is_mapped() {
        assert_eq!(
            battery_snapshot_from_values(101.5, State::Charging),
            Some(BatterySnapshot {
                charge_percent: 100.0,
                state: BatteryState::Charging,
            })
        );
    }

    #[test]
    fn nonempty_rejects_blank_optional_values() {
        assert_eq!(nonempty(Some("  ".to_string())), None);
        assert_eq!(
            nonempty(Some("Astra".to_string())),
            Some("Astra".to_string())
        );
    }
}
