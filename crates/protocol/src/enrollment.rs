//! Enrollment and invitation types.

use crate::resource::{HardwareCapability, ResourceLimits};
use crate::validation::{Validatable, ValidationError};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Invitation code for worker enrollment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvitationCode {
    pub code: String,
    pub cluster_name: String,
    pub coordinator_id: Uuid,
    pub expires_at: DateTime<Utc>,
    pub max_workers: Option<u32>,
    pub default_limits: ResourceLimits,
}

impl InvitationCode {
    pub fn is_valid(&self) -> bool {
        Utc::now() < self.expires_at
    }
}

/// Enrollment request from worker to coordinator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrollmentRequest {
    pub invitation_code: String,
    pub worker_name: String,
    pub capabilities: Vec<HardwareCapability>,
    pub proposed_limits: ResourceLimits,
    pub public_key: Option<String>, // For future authenticated channels
}

/// Enrollment response from coordinator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrollmentResponse {
    pub accepted: bool,
    pub worker_id: Option<Uuid>,
    pub assigned_limits: Option<ResourceLimits>,
    pub coordinator_info: CoordinatorInfo,
    pub error: Option<String>,
}

/// Coordinator identity information shared with workers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoordinatorInfo {
    pub id: Uuid,
    pub name: String,
    pub contact: Option<String>,
    pub public_key: Option<String>,
    pub supported_workloads: Vec<String>,
    pub protocol_version: String,
}

/// Revocation notice from coordinator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevocationNotice {
    pub worker_id: Uuid,
    pub reason: RevocationReason,
    pub revoked_at: DateTime<Utc>,
    pub grace_period_secs: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RevocationReason {
    AdminRevoked,
    PolicyViolation,
    SecurityConcern,
    CoordinatorMigration,
    OwnerRequested,
}

/// Configuration update from coordinator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigUpdate {
    pub worker_id: Uuid,
    pub limits: Option<ResourceLimits>,
    pub schedule: Option<crate::resource::SharingSchedule>,
    pub message: Option<String>,
    pub requires_ack: bool,
}

impl Validatable for EnrollmentRequest {
    fn validate(&self) -> Result<(), ValidationError> {
        if self.worker_name.trim().is_empty() {
            return Err(ValidationError::InvalidField(
                "worker_name cannot be empty".into(),
            ));
        }
        if self.worker_name.len() > 100 {
            return Err(ValidationError::InvalidField(
                "worker_name too long (max 100)".into(),
            ));
        }
        if self.capabilities.is_empty() {
            return Err(ValidationError::InvalidField(
                "at least one capability required".into(),
            ));
        }
        self.proposed_limits.validate()?;
        Ok(())
    }
}

impl Validatable for EnrollmentResponse {
    fn validate(&self) -> Result<(), ValidationError> {
        if self.accepted {
            if self.worker_id.is_none() {
                return Err(ValidationError::InvalidField(
                    "accepted enrollment must have worker_id".into(),
                ));
            }
            if self.assigned_limits.is_none() {
                return Err(ValidationError::InvalidField(
                    "accepted enrollment must have assigned_limits".into(),
                ));
            }
        } else {
            if self.error.is_none() {
                return Err(ValidationError::InvalidField(
                    "rejected enrollment must have error".into(),
                ));
            }
        }
        Ok(())
    }
}
