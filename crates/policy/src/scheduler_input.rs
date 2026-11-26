//! Input types for the scheduler from the policy layer.

use bg_helper_protocol::{
    job::{JobSpec, JobId},
    resource::{ResourceLimits, ResourceUsage, ResourceOffer, WorkerState, WorkerStatus, HardwareCapability},
};
use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Snapshot of worker state for scheduler consumption.
#[derive(Debug, Clone)]
pub struct WorkerSnapshot {
    pub worker_id: Uuid,
    pub status: WorkerState,
    pub capabilities: Vec<HardwareCapability>,
    pub limits: ResourceLimits,
    pub current_offer: Option<ResourceOffer>,
    pub current_usage: ResourceUsage,
    pub current_job: Option<JobId>,
    pub last_seen: DateTime<Utc>,
}

impl From<WorkerStatus> for WorkerSnapshot {
    fn from(status: WorkerStatus) -> Self {
        Self {
            worker_id: status.worker_id,
            status: status.state,
            capabilities: status.capabilities,
            limits: status.limits,
            current_offer: status.current_offer,
            current_usage: status.current_usage.unwrap_or_else(|| {
                // Fallback if no usage reported
                ResourceUsage {
                    worker_id: status.worker_id,
                    timestamp: Utc::now(),
                    cpu_utilization_pct: 0.0,
                    cpu_cores_used: 0.0,
                    gpu_utilization_pct: None,
                    gpu_memory_used_bytes: None,
                    ram_used_bytes: 0,
                    ram_available_bytes: 0,
                    storage_used_bytes: 0,
                    storage_available_bytes: 0,
                    power_state: bg_helper_protocol::resource::PowerState::Unknown,
                }
            }),
            current_job: status.current_job.map(JobId),
            last_seen: status.last_seen,
        }
    }
}

/// Job queue item for scheduler.
#[derive(Debug, Clone)]
pub struct QueuedJob {
    pub spec: JobSpec,
    pub queued_at: DateTime<Utc>,
    pub priority: u8,
    pub attempts: u32,
}

impl From<JobSpec> for QueuedJob {
    fn from(spec: JobSpec) -> Self {
        let priority = spec.priority;
        Self {
            spec,
            queued_at: Utc::now(),
            priority,
            attempts: 0,
        }
    }
}

/// Scheduler decision for a job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScheduleDecision {
    /// Assign job to worker
    Assign { worker_id: Uuid },
    /// Reject job (not suitable for any worker)
    Reject { reason: String },
    /// Defer job (no suitable worker now, try later)
    Defer { reason: String },
}

/// Worker suitability score for a job.
#[derive(Debug, Clone, PartialEq)]
pub struct WorkerScore {
    pub worker_id: Uuid,
    pub score: f64, // Higher = better fit
    pub cpu_available: bool,
    pub ram_available: bool,
    pub gpu_compatible: bool,
    pub schedule_ok: bool,
    pub power_ok: bool,
}

impl WorkerScore {
    pub fn is_suitable(&self) -> bool {
        self.cpu_available && self.ram_available && self.gpu_compatible && self.schedule_ok && self.power_ok
    }
}