//! Coordinator-Worker message types.

use crate::enrollment::{
    ConfigUpdate, CoordinatorInfo, EnrollmentRequest, EnrollmentResponse, RevocationNotice,
};
use crate::job::{Job, JobId, JobOutput, JobResult, JobSpec, JobState, WorkerId};
use crate::resource::{
    HardwareCapability, ResourceLimits, ResourceUsage, WorkerSnapshot, WorkerState,
};
use crate::validation::{Validatable, ValidationError};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Protocol version for compatibility checking.
pub const PROTOCOL_VERSION: &str = "0.1.0";

/// Messages from coordinator to worker.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CoordinatorMessage {
    /// Job offer to a worker.
    JobOffer {
        job: Job,
        offer_expires_at: DateTime<Utc>,
    },
    /// Cancel a previously offered/assigned job.
    JobCancel { job_id: JobId, reason: String },
    /// Update worker configuration.
    ConfigUpdate(ConfigUpdate),
    /// Revoke worker enrollment.
    Revoke(RevocationNotice),
    /// Heartbeat/ping from coordinator.
    Heartbeat {
        timestamp: DateTime<Utc>,
        coordinator_info: CoordinatorInfo,
    },
    /// Acknowledgment of worker message.
    Ack {
        correlation_id: Uuid,
        success: bool,
        message: Option<String>,
    },
    /// Request worker snapshot.
    RequestSnapshot,
    /// Protocol version negotiation.
    VersionCheck {
        min_version: String,
        max_version: String,
    },
}

/// Messages from worker to coordinator.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkerMessage {
    /// Enrollment request.
    Enroll(EnrollmentRequest),
    /// Worker heartbeat with current status.
    Heartbeat {
        worker_id: WorkerId,
        timestamp: DateTime<Utc>,
        state: WorkerState,
        usage: ResourceUsage,
        current_job: Option<JobId>,
    },
    /// Job acceptance (worker approves the offer).
    JobAccept { job_id: JobId, worker_id: WorkerId },
    /// Job rejection (worker declines the offer).
    JobReject {
        job_id: JobId,
        worker_id: WorkerId,
        reason: String,
    },
    /// Job progress update.
    JobProgress {
        job_id: JobId,
        worker_id: WorkerId,
        progress_pct: f32,
        estimated_remaining_secs: Option<u32>,
    },
    /// Job completion with result.
    JobComplete {
        job_id: JobId,
        worker_id: WorkerId,
        result: JobResult,
    },
    /// Job failure.
    JobFail {
        job_id: JobId,
        worker_id: WorkerId,
        error: String,
    },
    /// Worker snapshot (full state).
    Snapshot(WorkerSnapshot),
    /// Acknowledgment of coordinator message.
    Ack {
        correlation_id: Uuid,
        success: bool,
        message: Option<String>,
    },
    /// Request configuration update.
    RequestConfig,
    /// Protocol version negotiation.
    VersionCheck {
        min_version: String,
        max_version: String,
    },
}

/// Generic envelope for protocol messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageEnvelope<T> {
    pub correlation_id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub protocol_version: String,
    pub payload: T,
}

impl<T> MessageEnvelope<T> {
    pub fn new(payload: T) -> Self {
        Self {
            correlation_id: Uuid::new_v4(),
            timestamp: Utc::now(),
            protocol_version: PROTOCOL_VERSION.to_string(),
            payload,
        }
    }
}

/// Type aliases for convenience.
pub type CoordinatorEnvelope = MessageEnvelope<CoordinatorMessage>;
pub type WorkerEnvelope = MessageEnvelope<WorkerMessage>;

/// Result of message processing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageResult {
    pub correlation_id: Uuid,
    pub success: bool,
    pub message: Option<String>,
    pub response: Option<serde_json::Value>,
}

impl Validatable for CoordinatorMessage {
    fn validate(&self) -> Result<(), ValidationError> {
        match self {
            CoordinatorMessage::JobOffer {
                job,
                offer_expires_at,
            } => {
                job.spec.validate()?;
                if *offer_expires_at <= Utc::now() {
                    return Err(ValidationError::InvalidField(
                        "offer_expires_at must be in future".into(),
                    ));
                }
            }
            CoordinatorMessage::JobCancel { job_id: _, reason } => {
                if reason.trim().is_empty() {
                    return Err(ValidationError::InvalidField(
                        "cancel reason cannot be empty".into(),
                    ));
                }
            }
            CoordinatorMessage::ConfigUpdate(update) => {
                if let Some(limits) = &update.limits {
                    limits.validate()?;
                }
            }
            CoordinatorMessage::Revoke(notice) => {
                if notice.grace_period_secs > 3600 {
                    return Err(ValidationError::InvalidField(
                        "grace_period_secs too long (max 3600)".into(),
                    ));
                }
            }
            CoordinatorMessage::VersionCheck {
                min_version,
                max_version,
            } => {
                if min_version > max_version {
                    return Err(ValidationError::InvalidField(
                        "min_version > max_version".into(),
                    ));
                }
            }
            _ => {}
        }
        Ok(())
    }
}

impl Validatable for WorkerMessage {
    fn validate(&self) -> Result<(), ValidationError> {
        match self {
            WorkerMessage::Enroll(req) => req.validate()?,
            WorkerMessage::Heartbeat {
                worker_id: _,
                timestamp,
                state: _,
                usage,
                current_job: _,
            } => {
                if *timestamp > Utc::now() + chrono::Duration::seconds(60) {
                    return Err(ValidationError::InvalidField(
                        "heartbeat timestamp too far in future".into(),
                    ));
                }
                usage.validate()?;
            }
            WorkerMessage::JobAccept {
                job_id: _,
                worker_id: _,
            } => {}
            WorkerMessage::JobReject {
                job_id: _,
                worker_id: _,
                reason,
            } => {
                if reason.trim().is_empty() {
                    return Err(ValidationError::InvalidField(
                        "reject reason cannot be empty".into(),
                    ));
                }
            }
            WorkerMessage::JobProgress {
                job_id: _,
                worker_id: _,
                progress_pct,
                estimated_remaining_secs,
            } => {
                if *progress_pct < 0.0 || *progress_pct > 100.0 {
                    return Err(ValidationError::InvalidField(
                        "progress_pct must be 0..100".into(),
                    ));
                }
                if let Some(secs) = estimated_remaining_secs {
                    if *secs > 86400 {
                        return Err(ValidationError::InvalidField(
                            "estimated_remaining_secs too large".into(),
                        ));
                    }
                }
            }
            WorkerMessage::JobComplete {
                job_id: _,
                worker_id: _,
                result,
            } => {
                // Result validation would be workload-specific
            }
            WorkerMessage::JobFail {
                job_id: _,
                worker_id: _,
                error,
            } => {
                if error.trim().is_empty() {
                    return Err(ValidationError::InvalidField(
                        "error message cannot be empty".into(),
                    ));
                }
            }
            WorkerMessage::Snapshot(snapshot) => snapshot.validate()?,
            WorkerMessage::VersionCheck {
                min_version,
                max_version,
            } => {
                if min_version > max_version {
                    return Err(ValidationError::InvalidField(
                        "min_version > max_version".into(),
                    ));
                }
            }
            _ => {}
        }
        Ok(())
    }
}
