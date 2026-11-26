//! Worker UI components (TUI and mockups for owner-facing interface).

use bg_helper_protocol::job::{JobResult, JobSpec};
use bg_helper_protocol::resource::{
    PowerState, ResourceLimits, ResourceUsage, ThermalState, WorkerState,
};
use std::time::Duration;

/// Owner-facing status information.
#[derive(Debug, Clone)]
pub struct OwnerStatus {
    pub worker_name: String,
    pub current_job: Option<CurrentJobInfo>,
    pub resource_usage: ResourceUsage,
    pub limits: ResourceLimits,
    pub state: WorkerState,
    pub sharing_active: bool,
    pub last_update: std::time::SystemTime,
}

#[derive(Debug, Clone)]
pub struct CurrentJobInfo {
    pub job_id: String,
    pub workload_type: String,
    pub progress_pct: f32,
    pub elapsed: Duration,
    pub estimated_remaining: Option<Duration>,
}

/// Text-based UI for terminal (mockup).
pub struct TextUI {
    status: OwnerStatus,
}

impl TextUI {
    pub fn new(status: OwnerStatus) -> Self {
        Self { status }
    }

    /// Render the status as text.
    pub fn render(&self) -> String {
        let mut out = String::new();

        out.push_str(&format!("╔══════════════════════════════════════════╗\n"));
        out.push_str(&format!("║  BackgroundHelper Worker Status          ║\n"));
        out.push_str(&format!("╠══════════════════════════════════════════╣\n"));
        out.push_str(&format!("║  Worker: {:<30} ║\n", self.status.worker_name));
        out.push_str(&format!(
            "║  State:  {:<30} ║\n",
            format!("{:?}", self.status.state)
        ));
        out.push_str(&format!(
            "║  Sharing: {:<29} ║\n",
            if self.status.sharing_active {
                "ACTIVE"
            } else {
                "PAUSED"
            }
        ));
        out.push_str(&format!("╠══════════════════════════════════════════╣\n"));

        // Current job
        if let Some(job) = &self.status.current_job {
            out.push_str(&format!("║  Current Job:                            ║\n"));
            out.push_str(&format!(
                "║    ID: {:<34} ║\n",
                &job.job_id[..job.job_id.len().min(34)]
            ));
            out.push_str(&format!("║    Type: {:<30} ║\n", job.workload_type));
            out.push_str(&format!(
                "║    Progress: {:>3.0}%                      ║\n",
                job.progress_pct
            ));
            out.push_str(&format!(
                "║    Elapsed: {:>5}s  Remaining: {:>5}s   ║\n",
                job.elapsed.as_secs(),
                job.estimated_remaining.map(|d| d.as_secs()).unwrap_or(0)
            ));
        } else {
            out.push_str(&format!("║  Current Job: None                       ║\n"));
        }
        out.push_str(&format!("╠══════════════════════════════════════════╣\n"));

        // Resource usage
        let usage = &self.status.resource_usage;
        out.push_str(&format!("║  Resource Usage:                         ║\n"));
        out.push_str(&format!(
            "║    CPU: {:>5.1}% ({:.1} cores)            ║\n",
            usage.cpu_utilization_pct, usage.cpu_cores_used
        ));
        out.push_str(&format!(
            "║    RAM: {:>10} / {:>10} bytes          ║\n",
            usage.ram_used_bytes,
            usage.ram_used_bytes + usage.ram_available_bytes
        ));
        out.push_str(&format!(
            "║    Storage: {:>8} / {:>8} bytes         ║\n",
            usage.storage_used_bytes,
            usage.storage_used_bytes + usage.storage_available_bytes
        ));
        out.push_str(&format!(
            "║    Power: {:<30} ║\n",
            format!("{:?}", usage.power_state)
        ));
        out.push_str(&format!(
            "║    Thermal: {:<28} ║\n",
            format!("{:?}", usage.thermal_state)
        ));
        out.push_str(&format!("╠══════════════════════════════════════════╣\n"));

        // Limits
        out.push_str(&format!("║  Configured Limits:                      ║\n"));
        if let Some(cores) = self.status.limits.cpu_max_cores {
            out.push_str(&format!("║    Max CPU Cores: {:<21} ║\n", cores));
        }
        if let Some(pct) = self.status.limits.cpu_max_utilization_pct {
            out.push_str(&format!("║    Max CPU Utilization: {:>13}%    ║\n", pct));
        }
        if let Some(ram) = self.status.limits.ram_max_workload_bytes {
            out.push_str(&format!("║    Max RAM: {:<21} ║\n", format_bytes(ram)));
        }
        if let Some(quota) = self.status.limits.storage_quota_bytes {
            out.push_str(&format!(
                "║    Storage Quota: {:<19} ║\n",
                format_bytes(quota)
            ));
        }
        if let Some(schedule) = &self.status.limits.schedule {
            out.push_str(&format!(
                "║    Schedule: {} windows               ║\n",
                schedule.allowed_windows.len()
            ));
        }
        out.push_str(&format!(
            "║    Battery: {}                      ║\n",
            if self.status.limits.power.allow_on_battery {
                "Allowed"
            } else {
                "Not allowed"
            }
        ));
        out.push_str(&format!("╚══════════════════════════════════════════╝\n"));

        out
    }

    /// Render a compact single-line status.
    pub fn render_compact(&self) -> String {
        let job_str = if let Some(job) = &self.status.current_job {
            format!("Job: {} ({:.0}%)", job.workload_type, job.progress_pct)
        } else {
            "Idle".to_string()
        };

        format!(
            "BackgroundHelper | {} | {} | CPU: {:.0}% | RAM: {} | {}",
            self.status.worker_name,
            job_str,
            self.status.resource_usage.cpu_utilization_pct,
            format_bytes(self.status.resource_usage.ram_used_bytes),
            if self.status.sharing_active {
                "SHARING"
            } else {
                "PAUSED"
            }
        )
    }
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{:.1} {}", value, UNITS[unit])
}

/// Web-based UI data structures (for React/TypeScript frontend).
pub mod web {
    use super::*;
    use serde::Serialize;

    #[derive(Serialize)]
    pub struct DashboardData {
        pub worker: WorkerInfo,
        pub current_job: Option<JobInfo>,
        pub resources: ResourceInfo,
        pub limits: LimitsInfo,
        pub activity: Vec<ActivityEntry>,
    }

    #[derive(Serialize)]
    pub struct WorkerInfo {
        pub name: String,
        pub state: String,
        pub enrolled_at: String,
        pub version: String,
    }

    #[derive(Serialize)]
    pub struct JobInfo {
        pub id: String,
        pub workload_type: String,
        pub progress_pct: f32,
        pub elapsed_secs: u64,
        pub estimated_remaining_secs: Option<u64>,
    }

    #[derive(Serialize)]
    pub struct ResourceInfo {
        pub cpu_pct: f32,
        pub cpu_cores: f32,
        pub ram_used: u64,
        pub ram_total: u64,
        pub storage_used: u64,
        pub storage_total: u64,
        pub power_state: String,
        pub thermal_state: String,
    }

    #[derive(Serialize)]
    pub struct LimitsInfo {
        pub max_cpu_cores: Option<u32>,
        pub max_cpu_pct: Option<u8>,
        pub max_ram: Option<u64>,
        pub storage_quota: Option<u64>,
        pub schedule_windows: usize,
        pub battery_allowed: bool,
    }

    #[derive(Serialize)]
    pub struct ActivityEntry {
        pub timestamp: String,
        pub event: String,
        pub details: String,
    }

    impl DashboardData {
        pub fn from_status(status: &OwnerStatus) -> Self {
            Self {
                worker: WorkerInfo {
                    name: status.worker_name.clone(),
                    state: format!("{:?}", status.state),
                    enrolled_at: "2024-01-01T00:00:00Z".into(), // Would come from config
                    version: "0.1.0".into(),
                },
                current_job: status.current_job.as_ref().map(|j| JobInfo {
                    id: j.job_id.clone(),
                    workload_type: j.workload_type.clone(),
                    progress_pct: j.progress_pct,
                    elapsed_secs: j.elapsed.as_secs(),
                    estimated_remaining_secs: j.estimated_remaining.map(|d| d.as_secs()),
                }),
                resources: ResourceInfo {
                    cpu_pct: status.resource_usage.cpu_utilization_pct,
                    cpu_cores: status.resource_usage.cpu_cores_used,
                    ram_used: status.resource_usage.ram_used_bytes,
                    ram_total: status.resource_usage.ram_used_bytes
                        + status.resource_usage.ram_available_bytes,
                    storage_used: status.resource_usage.storage_used_bytes,
                    storage_total: status.resource_usage.storage_used_bytes
                        + status.resource_usage.storage_available_bytes,
                    power_state: format!("{:?}", status.resource_usage.power_state),
                    thermal_state: format!("{:?}", status.resource_usage.thermal_state),
                },
                limits: LimitsInfo {
                    max_cpu_cores: status.limits.cpu_max_cores,
                    max_cpu_pct: status.limits.cpu_max_utilization_pct,
                    max_ram: status.limits.ram_max_workload_bytes,
                    storage_quota: status.limits.storage_quota_bytes,
                    schedule_windows: status
                        .limits
                        .schedule
                        .as_ref()
                        .map(|s| s.allowed_windows.len())
                        .unwrap_or(0),
                    battery_allowed: status.limits.power.allow_on_battery,
                },
                activity: vec![], // Would be populated from history
            }
        }
    }
}

/// Settings UI data structures.
pub mod settings {
    use bg_helper_protocol::resource::{PowerPolicy, ResourceLimits, SharingSchedule, TimeWindow};
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize)]
    pub struct SettingsForm {
        pub cpu_cores: Option<u32>,
        pub cpu_utilization_pct: Option<u8>,
        pub ram_limit_gb: Option<f32>,
        pub storage_quota_gb: Option<f32>,
        pub battery_allowed: bool,
        pub min_battery_pct: Option<u8>,
        pub schedule: ScheduleForm,
    }

    #[derive(Serialize, Deserialize, Default)]
    pub struct ScheduleForm {
        pub enabled: bool,
        pub timezone: String,
        pub windows: Vec<WindowForm>,
    }

    #[derive(Serialize, Deserialize)]
    pub struct WindowForm {
        pub days: Vec<u8>, // 0=Sunday
        pub start_hour: u8,
        pub start_minute: u8,
        pub end_hour: u8,
        pub end_minute: u8,
    }

    impl SettingsForm {
        pub fn from_limits(limits: &ResourceLimits) -> Self {
            Self {
                cpu_cores: limits.cpu_max_cores,
                cpu_utilization_pct: limits.cpu_max_utilization_pct,
                ram_limit_gb: limits
                    .ram_max_workload_bytes
                    .map(|b| b as f32 / 1_073_741_824.0),
                storage_quota_gb: limits
                    .storage_quota_bytes
                    .map(|b| b as f32 / 1_073_741_824.0),
                battery_allowed: limits.power.allow_on_battery,
                min_battery_pct: limits.power.min_battery_pct,
                schedule: ScheduleForm {
                    enabled: limits.schedule.is_some(),
                    timezone: limits
                        .schedule
                        .as_ref()
                        .map(|s| s.timezone.clone())
                        .unwrap_or_else(|| "UTC".into()),
                    windows: limits
                        .schedule
                        .as_ref()
                        .map(|s| {
                            s.allowed_windows
                                .iter()
                                .map(|w| WindowForm {
                                    days: w.days.clone(),
                                    start_hour: w.start_hour,
                                    start_minute: w.start_minute,
                                    end_hour: w.end_hour,
                                    end_minute: w.end_minute,
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                },
            }
        }

        pub fn to_limits(&self) -> ResourceLimits {
            ResourceLimits {
                cpu_max_cores: self.cpu_cores,
                cpu_max_utilization_pct: self.cpu_utilization_pct,
                ram_max_workload_bytes: self.ram_limit_gb.map(|g| (g * 1_073_741_824.0) as u64),
                storage_quota_bytes: self.storage_quota_gb.map(|g| (g * 1_073_741_824.0) as u64),
                schedule: if self.schedule.enabled {
                    Some(SharingSchedule {
                        timezone: self.schedule.timezone.clone(),
                        allowed_windows: self
                            .schedule
                            .windows
                            .iter()
                            .map(|w| TimeWindow {
                                days: w.days.clone(),
                                start_hour: w.start_hour,
                                start_minute: w.start_minute,
                                end_hour: w.end_hour,
                                end_minute: w.end_minute,
                            })
                            .collect(),
                    })
                } else {
                    None
                },
                power: PowerPolicy {
                    allow_on_battery: self.battery_allowed,
                    min_battery_pct: self.min_battery_pct,
                    require_ac_for_gpu: true,
                },
                ..Default::default()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bg_helper_protocol::resource::{
        PowerState, ResourceLimits, ResourceUsage, ThermalState, WorkerState,
    };
    use chrono::Utc;
    use uuid::Uuid;

    fn test_status() -> OwnerStatus {
        OwnerStatus {
            worker_name: "test-worker".into(),
            current_job: Some(CurrentJobInfo {
                job_id: "job-123".into(),
                workload_type: "range_sum".into(),
                progress_pct: 50.0,
                elapsed: Duration::from_secs(10),
                estimated_remaining: Some(Duration::from_secs(10)),
            }),
            resource_usage: ResourceUsage {
                worker_id: Uuid::new_v4(),
                timestamp: Utc::now(),
                cpu_utilization_pct: 45.0,
                cpu_cores_used: 2.0,
                cpu_frequency_mhz: Some(3000),
                gpu_utilization_pct: None,
                gpu_memory_used_bytes: None,
                gpu_temperature_c: None,
                ram_used_bytes: 4_000_000_000,
                ram_available_bytes: 12_000_000_000,
                storage_used_bytes: 50_000_000_000,
                storage_available_bytes: 50_000_000_000,
                power_state: PowerState::Ac,
                thermal_state: ThermalState::Normal,
            },
            limits: ResourceLimits {
                cpu_max_cores: Some(4),
                cpu_max_utilization_pct: Some(50),
                ram_max_workload_bytes: Some(8_000_000_000),
                storage_quota_bytes: Some(100_000_000_000),
                power: Default::default(),
                ..Default::default()
            },
            state: WorkerState::Busy,
            sharing_active: true,
            last_update: std::time::SystemTime::now(),
        }
    }

    #[test]
    fn test_text_ui_render() {
        let ui = TextUI::new(test_status());
        let output = ui.render();

        assert!(output.contains("BackgroundHelper Worker Status"));
        assert!(output.contains("test-worker"));
        assert!(output.contains("range_sum"));
        assert!(output.contains("50%"));
    }

    #[test]
    fn test_compact_render() {
        let ui = TextUI::new(test_status());
        let output = ui.render_compact();

        assert!(output.contains("BackgroundHelper"));
        assert!(output.contains("test-worker"));
        assert!(output.contains("SHARING"));
    }

    #[test]
    fn test_settings_form() {
        let limits = ResourceLimits {
            cpu_max_cores: Some(2),
            cpu_max_utilization_pct: Some(50),
            ram_max_workload_bytes: Some(4_000_000_000),
            storage_quota_bytes: Some(10_000_000_000),
            power: Default::default(),
            ..Default::default()
        };

        let form = settings::SettingsForm::from_limits(&limits);
        assert_eq!(form.cpu_cores, Some(2));
        assert_eq!(form.cpu_utilization_pct, Some(50));
        assert!((form.ram_limit_gb.unwrap() - 3.72).abs() < 0.1);
    }
}
