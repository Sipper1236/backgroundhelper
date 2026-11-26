//! Default resource limits and limit construction helpers.

use bg_helper_protocol::resource::{PowerPolicy, ResourceLimits, SharingSchedule, TimeWindow};
use bg_helper_protocol::validation::Validatable;
use std::path::PathBuf;

/// Default resource limits for v0.1 prototype (conservative).
pub fn default_limits() -> ResourceLimits {
    ResourceLimits {
        cpu_max_cores: Some(1),
        cpu_max_utilization_pct: Some(50),
        gpu_enabled: false,
        gpu_allowed_devices: vec![],
        gpu_max_memory_pct: None,
        ram_max_workload_bytes: Some(1_000_000_000), // 1GB
        storage_workspace_path: default_workspace_path(),
        storage_quota_bytes: Some(5_000_000_000), // 5GB
        schedule: None,
        power: PowerPolicy::default(),
    }
}

/// Default workspace path for storage.
fn default_workspace_path() -> Option<String> {
    dirs::data_dir()
        .map(|p| p.join("BackgroundHelper").join("workspace"))
        .map(|p| p.to_string_lossy().to_string())
}

/// Conservative limits for first real-device testing.
pub fn conservative_limits() -> ResourceLimits {
    ResourceLimits {
        cpu_max_cores: Some(1),
        cpu_max_utilization_pct: Some(25),
        gpu_enabled: false,
        gpu_allowed_devices: vec![],
        gpu_max_memory_pct: None,
        ram_max_workload_bytes: Some(500_000_000), // 500MB
        storage_workspace_path: default_workspace_path(),
        storage_quota_bytes: Some(1_000_000_000), // 1GB
        schedule: Some(office_hours_schedule("UTC".into())),
        power: PowerPolicy {
            allow_on_battery: false,
            min_battery_pct: Some(80),
            require_ac_for_gpu: true,
        },
    }
}

/// Creates a typical "office hours" schedule.
pub fn office_hours_schedule(timezone: String) -> SharingSchedule {
    SharingSchedule {
        timezone,
        allowed_windows: vec![
            TimeWindow {
                days: vec![1, 2, 3, 4, 5],
                start_hour: 9,
                start_minute: 0,
                end_hour: 17,
                end_minute: 0,
            },
            TimeWindow {
                days: vec![1, 2, 3, 4, 5],
                start_hour: 18,
                start_minute: 0,
                end_hour: 22,
                end_minute: 0,
            },
        ],
    }
}

/// Creates a "night only" schedule.
pub fn night_only_schedule(timezone: String) -> SharingSchedule {
    SharingSchedule {
        timezone,
        allowed_windows: vec![TimeWindow {
            days: vec![0, 1, 2, 3, 4, 5, 6],
            start_hour: 22,
            start_minute: 0,
            end_hour: 6,
            end_minute: 0,
        }],
    }
}

/// Creates a "weekends only" schedule.
pub fn weekends_only_schedule(timezone: String) -> SharingSchedule {
    SharingSchedule {
        timezone,
        allowed_windows: vec![TimeWindow {
            days: vec![0, 6],
            start_hour: 8,
            start_minute: 0,
            end_hour: 22,
            end_minute: 0,
        }],
    }
}

/// Build limits interactively (for setup UI).
pub struct LimitsBuilder {
    limits: ResourceLimits,
}

impl LimitsBuilder {
    pub fn new() -> Self {
        Self {
            limits: default_limits(),
        }
    }

    pub fn cpu_cores(mut self, cores: u32) -> Self {
        self.limits.cpu_max_cores = Some(cores);
        self
    }

    pub fn cpu_utilization(mut self, pct: u8) -> Self {
        self.limits.cpu_max_utilization_pct = Some(pct.clamp(1, 100));
        self
    }

    pub fn enable_gpu(mut self, enabled: bool) -> Self {
        self.limits.gpu_enabled = enabled;
        self
    }

    pub fn gpu_devices(mut self, devices: Vec<String>) -> Self {
        self.limits.gpu_allowed_devices = devices;
        self
    }

    pub fn ram_limit(mut self, bytes: u64) -> Self {
        self.limits.ram_max_workload_bytes = Some(bytes);
        self
    }

    pub fn storage_path(mut self, path: PathBuf) -> Self {
        self.limits.storage_workspace_path = Some(path.to_string_lossy().to_string());
        self
    }

    pub fn storage_quota(mut self, bytes: u64) -> Self {
        self.limits.storage_quota_bytes = Some(bytes);
        self
    }

    pub fn schedule(mut self, schedule: SharingSchedule) -> Self {
        self.limits.schedule = Some(schedule);
        self
    }

    pub fn power_policy(mut self, policy: PowerPolicy) -> Self {
        self.limits.power = policy;
        self
    }

    pub fn build(self) -> Result<ResourceLimits, String> {
        self.limits.validate().map_err(|e| e.to_string())?;
        Ok(self.limits)
    }
}

impl Default for LimitsBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bg_helper_protocol::validation::Validatable;

    #[test]
    fn test_default_limits_valid() {
        let limits = default_limits();
        assert!(limits.validate().is_ok());
    }

    #[test]
    fn test_conservative_limits_valid() {
        let limits = conservative_limits();
        assert!(limits.validate().is_ok());
    }

    #[test]
    fn test_limits_builder() {
        let limits = LimitsBuilder::new()
            .cpu_cores(2)
            .cpu_utilization(75)
            .ram_limit(2_000_000_000)
            .build()
            .unwrap();

        assert_eq!(limits.cpu_max_cores, Some(2));
        assert_eq!(limits.cpu_max_utilization_pct, Some(75));
        assert_eq!(limits.ram_max_workload_bytes, Some(2_000_000_000));
    }

    #[test]
    fn test_limits_builder_clamp() {
        let limits = LimitsBuilder::new()
            .cpu_utilization(150) // Should clamp to 100
            .build()
            .unwrap();

        assert_eq!(limits.cpu_max_utilization_pct, Some(100));
    }

    #[test]
    fn test_schedule_helpers() {
        let schedule = office_hours_schedule("UTC".into());
        assert_eq!(schedule.allowed_windows.len(), 2);

        let schedule = night_only_schedule("UTC".into());
        assert_eq!(schedule.allowed_windows.len(), 1);
        assert_eq!(schedule.allowed_windows[0].start_hour, 22);
    }
}
