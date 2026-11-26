//! Worker registry and enrollment management.

use bg_helper_protocol::{
    enrollment::{CoordinatorInfo, EnrollmentRequest, EnrollmentResponse, InvitationCode},
    resource::{ResourceLimits, WorkerSnapshot},
    validation::Validatable,
};
use chrono::{DateTime, Duration, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};
use uuid::Uuid;

/// Worker registry for managing enrollments and invitations.
pub struct WorkerRegistry {
    /// Pending invitation codes
    invitations: Arc<RwLock<HashMap<String, InvitationCode>>>,
    /// Enrolled workers
    workers: Arc<RwLock<HashMap<Uuid, WorkerSnapshot>>>,
    /// Coordinator info
    coordinator_info: CoordinatorInfo,
    /// Default limits for new workers
    default_limits: ResourceLimits,
}

impl WorkerRegistry {
    pub fn new(coordinator_info: CoordinatorInfo, default_limits: ResourceLimits) -> Self {
        Self {
            invitations: Arc::new(RwLock::new(HashMap::new())),
            workers: Arc::new(RwLock::new(HashMap::new())),
            coordinator_info,
            default_limits,
        }
    }

    /// Create a new invitation code.
    pub async fn create_invitation(
        &self,
        cluster_name: String,
        expires_in_hours: u32,
        max_workers: Option<u32>,
    ) -> InvitationCode {
        let code = generate_invitation_code();
        let invitation = InvitationCode {
            code: code.clone(),
            cluster_name,
            coordinator_id: self.coordinator_info.id,
            expires_at: Utc::now() + Duration::hours(expires_in_hours as i64),
            max_workers,
            default_limits: self.default_limits.clone(),
        };

        self.invitations
            .write()
            .await
            .insert(code, invitation.clone());
        invitation
    }

    /// Validate and consume an invitation code.
    pub async fn validate_invitation(&self, code: &str) -> Option<InvitationCode> {
        let mut invitations = self.invitations.write().await;
        if let Some(invitation) = invitations.get(code) {
            if invitation.is_valid() {
                // Check max workers if specified
                if let Some(max) = invitation.max_workers {
                    let current = self.workers.read().await.len() as u32;
                    if current >= max {
                        return None;
                    }
                }
                Some(invitation.clone())
            } else {
                invitations.remove(code);
                None
            }
        } else {
            None
        }
    }

    /// Process enrollment request.
    pub async fn enroll_worker(&self, request: EnrollmentRequest) -> EnrollmentResponse {
        if let Err(e) = request.validate() {
            return EnrollmentResponse {
                accepted: false,
                worker_id: None,
                assigned_limits: None,
                coordinator_info: self.coordinator_info.clone(),
                error: Some(e.to_string()),
            };
        }

        // Validate invitation
        let invitation = match self.validate_invitation(&request.invitation_code).await {
            Some(inv) => inv,
            None => {
                return EnrollmentResponse {
                    accepted: false,
                    worker_id: None,
                    assigned_limits: None,
                    coordinator_info: self.coordinator_info.clone(),
                    error: Some("Invalid or expired invitation code".into()),
                };
            }
        };

        let worker_id = Uuid::new_v4();
        info!("Enrolling worker {} ({})", request.worker_name, worker_id);

        // Merge limits (local takes precedence for restrictions)
        let assigned_limits =
            bg_helper_policy::merge_limits(&request.proposed_limits, &invitation.default_limits);

        if let Err(e) = assigned_limits.validate() {
            return EnrollmentResponse {
                accepted: false,
                worker_id: None,
                assigned_limits: None,
                coordinator_info: self.coordinator_info.clone(),
                error: Some(format!("Invalid limits: {}", e)),
            };
        }

        // Create worker snapshot
        let snapshot = WorkerSnapshot {
            worker_id,
            capabilities: request.capabilities,
            limits: assigned_limits.clone(),
            usage: bg_helper_protocol::resource::ResourceUsage {
                worker_id,
                timestamp: Utc::now(),
                cpu_utilization_pct: 0.0,
                cpu_cores_used: 0.0,
                cpu_frequency_mhz: None,
                gpu_utilization_pct: None,
                gpu_memory_used_bytes: None,
                gpu_temperature_c: None,
                ram_used_bytes: 0,
                ram_available_bytes: 0,
                storage_used_bytes: 0,
                storage_available_bytes: 0,
                power_state: bg_helper_protocol::resource::PowerState::Unknown,
                thermal_state: bg_helper_protocol::resource::ThermalState::Normal,
            },
            state: bg_helper_protocol::resource::WorkerState::Online,
            enrolled_at: Utc::now(),
            last_heartbeat: Utc::now(),
            version: bg_helper_protocol::messages::PROTOCOL_VERSION.into(),
        };

        self.workers.write().await.insert(worker_id, snapshot);

        EnrollmentResponse {
            accepted: true,
            worker_id: Some(worker_id),
            assigned_limits: Some(assigned_limits),
            coordinator_info: self.coordinator_info.clone(),
            error: None,
        }
    }

    /// Get worker snapshot.
    pub async fn get_worker(&self, worker_id: Uuid) -> Option<WorkerSnapshot> {
        self.workers.read().await.get(&worker_id).cloned()
    }

    /// Update worker status and usage.
    pub async fn update_worker(
        &self,
        worker_id: Uuid,
        usage: bg_helper_protocol::resource::ResourceUsage,
        state: bg_helper_protocol::resource::WorkerState,
    ) -> bool {
        let mut workers = self.workers.write().await;
        if let Some(worker) = workers.get_mut(&worker_id) {
            worker.usage = usage;
            worker.state = state;
            worker.last_heartbeat = Utc::now();
            true
        } else {
            false
        }
    }

    /// Revoke a worker.
    pub async fn revoke_worker(&self, worker_id: Uuid) -> bool {
        let mut workers = self.workers.write().await;
        if let Some(worker) = workers.get_mut(&worker_id) {
            worker.state = bg_helper_protocol::resource::WorkerState::Revoked;
            true
        } else {
            false
        }
    }

    /// List all workers.
    pub async fn list_workers(&self) -> Vec<WorkerSnapshot> {
        self.workers.read().await.values().cloned().collect()
    }

    /// Get coordinator info.
    pub fn coordinator_info(&self) -> &CoordinatorInfo {
        &self.coordinator_info
    }
}

fn generate_invitation_code() -> String {
    use rand::Rng;
    const CHARSET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789"; // No confusing chars
    let mut rng = rand::thread_rng();
    (0..16)
        .map(|_| CHARSET[rng.gen_range(0..CHARSET.len())] as char)
        .collect::<String>()
        .as_bytes()
        .chunks(4)
        .map(|chunk| std::str::from_utf8(chunk).unwrap())
        .collect::<Vec<_>>()
        .join("-")
}

#[cfg(test)]
mod tests {
    use super::*;
    use bg_helper_protocol::enrollment::EnrollmentRequest;
    use bg_helper_protocol::resource::{HardwareCapability, ResourceLimits};
    use uuid::Uuid;

    #[tokio::test]
    async fn test_registry_create_invitation() {
        let coordinator_info = bg_helper_protocol::enrollment::CoordinatorInfo {
            id: Uuid::new_v4(),
            name: "Test Coordinator".into(),
            contact: None,
            public_key: None,
            supported_workloads: vec![],
            protocol_version: "0.1.0".into(),
        };

        let registry = WorkerRegistry::new(coordinator_info, ResourceLimits::default());
        let invitation = registry
            .create_invitation("Test Cluster".into(), 24, Some(10))
            .await;

        assert_eq!(invitation.cluster_name, "Test Cluster");
        assert!(invitation.is_valid());
        assert_eq!(invitation.max_workers, Some(10));
    }

    #[tokio::test]
    async fn test_registry_enroll_worker() {
        let coordinator_info = bg_helper_protocol::enrollment::CoordinatorInfo {
            id: Uuid::new_v4(),
            name: "Test Coordinator".into(),
            contact: None,
            public_key: None,
            supported_workloads: vec![],
            protocol_version: "0.1.0".into(),
        };

        let registry = WorkerRegistry::new(coordinator_info, ResourceLimits::default());
        let invitation = registry
            .create_invitation("Test Cluster".into(), 24, Some(10))
            .await;

        let request = EnrollmentRequest {
            invitation_code: invitation.code.clone(),
            worker_name: "Test Worker".into(),
            capabilities: vec![HardwareCapability::Cpu {
                cores: 4,
                threads: 8,
                frequency_mhz: 3000,
                architecture: "x86_64".into(),
                features: vec![],
            }],
            proposed_limits: ResourceLimits::default(),
            public_key: None,
        };

        let response = registry.enroll_worker(request).await;

        assert!(response.accepted);
        assert!(response.worker_id.is_some());
        assert!(response.assigned_limits.is_some());
    }

    #[tokio::test]
    async fn test_registry_reject_invalid_invitation() {
        let coordinator_info = bg_helper_protocol::enrollment::CoordinatorInfo {
            id: Uuid::new_v4(),
            name: "Test Coordinator".into(),
            contact: None,
            public_key: None,
            supported_workloads: vec![],
            protocol_version: "0.1.0".into(),
        };

        let registry = WorkerRegistry::new(coordinator_info, ResourceLimits::default());

        let request = EnrollmentRequest {
            invitation_code: "INVALID-CODE".into(),
            worker_name: "Test Worker".into(),
            capabilities: vec![HardwareCapability::Cpu {
                cores: 4,
                threads: 8,
                frequency_mhz: 3000,
                architecture: "x86_64".into(),
                features: vec![],
            }],
            proposed_limits: ResourceLimits::default(),
            public_key: None,
        };

        let response = registry.enroll_worker(request).await;

        assert!(!response.accepted);
        assert!(response.error.is_some());
    }
}
