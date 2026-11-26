//! Resource descriptions, limits, and hardware capabilities.

use crate::validation::{Validatable, ValidationError};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Hardware capability advertised by a worker.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HardwareCapability {
    Cpu {
        cores: u32,
        threads: u32,
        frequency_mhz: u32,
        architecture: String,
        features: Vec<String>,
    },
    Gpu {
        device_id: String,
        name: String,
        vendor: GpuVendor,
        memory_bytes: u64,
        compute_units: u32,
        api_support: Vec<GpuApi>,
    },
    Memory {
        total_bytes: u64,
        available_bytes: u64,
    },
    Storage {
        path: String,
        total_bytes: u64,
        available_bytes: u64,
        kind: StorageKind,
    },
    Network {
        interface: String,
        bandwidth_mbps: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GpuVendor {
    Amd,
    Nvidia,
    Intel,
    Apple,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GpuApi {
    Cuda,
    Rocm,
    Vulkan,
    Metal,
    OpenCl,
    LevelZero,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StorageKind {
    Ssd,
    Hdd,
    Nvme,
    Network,
    Other(String),
}

/// Current resource usage snapshot from a worker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUsage {
    pub worker_id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub cpu_utilization_pct: f32,
    pub cpu_cores_used: f32,
    pub cpu_frequency_mhz: Option<u32>,
    pub gpu_utilization_pct: Option<f32>,
    pub gpu_memory_used_bytes: Option<u64>,
    pub gpu_temperature_c: Option<f32>,
    pub ram_used_bytes: u64,
    pub ram_available_bytes: u64,
    pub storage_used_bytes: u64,
    pub storage_available_bytes: u64,
    pub power_state: PowerState,
    pub thermal_state: ThermalState,
}

/// Power state of the device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PowerState {
    Ac,
    Battery,
    Unknown,
}

/// Thermal state of the device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThermalState {
    Normal,
    Warm,
    Hot,
    Critical,
}

/// Worker state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorkerState {
    Online,
    Idle,
    Busy,
    Paused,
    Offline,
    Revoked,
}

impl WorkerState {
    pub fn can_accept_work(&self) -> bool {
        matches!(self, WorkerState::Online | WorkerState::Idle)
    }
}

/// Owner-defined resource limits (authoritative).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// Maximum CPU cores the worker may use for cluster work.
    pub cpu_max_cores: Option<u32>,
    /// Maximum CPU utilization percentage (0-100).
    pub cpu_max_utilization_pct: Option<u8>,
    /// Whether GPU workloads are allowed.
    pub gpu_enabled: bool,
    /// Specific GPU devices allowed (empty = any).
    pub gpu_allowed_devices: Vec<String>,
    /// Maximum GPU memory percentage (0-100).
    pub gpu_max_memory_pct: Option<u8>,
    /// Maximum RAM for workloads in bytes.
    pub ram_max_workload_bytes: Option<u64>,
    /// Dedicated workspace path for cluster storage.
    pub storage_workspace_path: Option<String>,
    /// Storage quota in bytes.
    pub storage_quota_bytes: Option<u64>,
    /// Sharing schedule (time windows when work is allowed).
    pub schedule: Option<SharingSchedule>,
    /// Power-related policies.
    pub power: PowerPolicy,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            cpu_max_cores: Some(4),
            cpu_max_utilization_pct: Some(100), // Allow full CPU utilization for simulation
            gpu_enabled: false,
            gpu_allowed_devices: vec![],
            gpu_max_memory_pct: None,
            ram_max_workload_bytes: Some(1_000_000_000), // 1GB
            storage_workspace_path: None,
            storage_quota_bytes: None,
            schedule: None,
            power: PowerPolicy::default(),
        }
    }
}

/// Power-related policy settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerPolicy {
    /// Allow computation on battery power.
    pub allow_on_battery: bool,
    /// Minimum battery percentage to allow work (if on battery).
    pub min_battery_pct: Option<u8>,
    /// Require AC power for GPU workloads.
    pub require_ac_for_gpu: bool,
}

impl Default for PowerPolicy {
    fn default() -> Self {
        Self {
            allow_on_battery: false,
            min_battery_pct: Some(50),
            require_ac_for_gpu: true,
        }
    }
}

/// Time window for sharing schedule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeWindow {
    /// Days of week (0=Sunday, 6=Saturday).
    pub days: Vec<u8>,
    /// Start hour (0-23).
    pub start_hour: u8,
    /// Start minute (0-59).
    pub start_minute: u8,
    /// End hour (0-23).
    pub end_hour: u8,
    /// End minute (0-59).
    pub end_minute: u8,
}

impl TimeWindow {
    pub fn contains(&self, dt: &chrono::DateTime<chrono::FixedOffset>) -> bool {
        use chrono::{Datelike, Timelike};
        let weekday = dt.weekday().number_from_sunday() as u8 - 1; // 0=Sunday
        let hour = dt.hour() as u8;
        let minute = dt.minute() as u8;

        if !self.days.contains(&weekday) {
            return false;
        }

        let current_minutes = hour as u16 * 60 + minute as u16;
        let start_minutes = self.start_hour as u16 * 60 + self.start_minute as u16;
        let end_minutes = self.end_hour as u16 * 60 + self.end_minute as u16;

        if start_minutes <= end_minutes {
            current_minutes >= start_minutes && current_minutes < end_minutes
        } else {
            // Window crosses midnight
            current_minutes >= start_minutes || current_minutes < end_minutes
        }
    }
}

/// Sharing schedule defining when cluster work is allowed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharingSchedule {
    pub timezone: String,
    pub allowed_windows: Vec<TimeWindow>,
}

impl SharingSchedule {
    pub fn is_allowed_now(&self) -> bool {
        use chrono::TimeZone;
        // Parse timezone as offset in hours (e.g., "UTC", "-5", "+8")
        let offset_hours = self.timezone.parse::<i32>().unwrap_or(0);
        let tz = chrono::FixedOffset::east_opt(offset_hours * 3600)
            .unwrap_or_else(|| chrono::FixedOffset::east_opt(0).unwrap());
        let now = Utc::now().with_timezone(&tz);
        self.allowed_windows.iter().any(|w| w.contains(&now))
    }
}

/// Worker snapshot combining capabilities, limits, and current usage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerSnapshot {
    pub worker_id: Uuid,
    pub capabilities: Vec<HardwareCapability>,
    pub limits: ResourceLimits,
    pub usage: ResourceUsage,
    pub state: WorkerState,
    pub enrolled_at: DateTime<Utc>,
    pub last_heartbeat: DateTime<Utc>,
    pub version: String,
}

impl Validatable for ResourceLimits {
    fn validate(&self) -> Result<(), ValidationError> {
        if let Some(pct) = self.cpu_max_utilization_pct {
            if pct == 0 || pct > 100 {
                return Err(ValidationError::InvalidField(
                    "cpu_max_utilization_pct must be 1..100".into(),
                ));
            }
        }
        if let Some(pct) = self.gpu_max_memory_pct {
            if pct == 0 || pct > 100 {
                return Err(ValidationError::InvalidField(
                    "gpu_max_memory_pct must be 1..100".into(),
                ));
            }
        }
        if let Some(pct) = self.power.min_battery_pct {
            if pct == 0 || pct > 100 {
                return Err(ValidationError::InvalidField(
                    "min_battery_pct must be 1..100".into(),
                ));
            }
        }
        if let Some(schedule) = &self.schedule {
            for window in &schedule.allowed_windows {
                for day in &window.days {
                    if *day > 6 {
                        return Err(ValidationError::InvalidField(
                            "schedule days must be 0..6".into(),
                        ));
                    }
                }
                if window.start_hour > 23 || window.end_hour > 23 {
                    return Err(ValidationError::InvalidField(
                        "schedule hours must be 0..23".into(),
                    ));
                }
                if window.start_minute > 59 || window.end_minute > 59 {
                    return Err(ValidationError::InvalidField(
                        "schedule minutes must be 0..59".into(),
                    ));
                }
            }
        }
        Ok(())
    }
}

impl Validatable for WorkerSnapshot {
    fn validate(&self) -> Result<(), ValidationError> {
        self.limits.validate()?;
        self.usage.validate()?;
        Ok(())
    }
}

impl Validatable for ResourceUsage {
    fn validate(&self) -> Result<(), ValidationError> {
        if self.cpu_utilization_pct < 0.0 || self.cpu_utilization_pct > 100.0 {
            return Err(ValidationError::InvalidField(
                "cpu_utilization_pct must be 0..100".into(),
            ));
        }
        if let Some(pct) = self.gpu_utilization_pct {
            if pct < 0.0 || pct > 100.0 {
                return Err(ValidationError::InvalidField(
                    "gpu_utilization_pct must be 0..100".into(),
                ));
            }
        }
        Ok(())
    }
}
