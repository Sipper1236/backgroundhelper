//! Worker enrollment handling.

use bg_helper_protocol::{
    enrollment::{EnrollmentRequest, EnrollmentResponse, InvitationCode, CoordinatorInfo, RevocationNotice},
    resource::{ResourceLimits, HardwareCapability},
    messages::PROTOCOL_VERSION,
};
use crate::config::WorkerConfig;
use crate::monitor::ResourceMonitor;
use uuid::Uuid;
use tracing::{info, warn, error};

/// Enrollment manager.
pub struct EnrollmentManager {
    config: WorkerConfig,
    monitor: ResourceMonitor,
}

impl EnrollmentManager {
    pub fn new(config: WorkerConfig) -> Self {
        let worker_id = config.worker_id.unwrap_or_else(Uuid::new_v4);
        Self {
            config,
            monitor: ResourceMonitor::new(worker_id),
        }
    }

    /// Enroll with coordinator using invitation code.
    pub async fn enroll(&mut self, invitation_code: &str) -> Result<EnrollmentResponse, String> {
        // Collect current capabilities
        let capabilities = self.monitor.collect_capabilities();
        
        let request = EnrollmentRequest {
            invitation_code: invitation_code.to_string(),
            worker_name: self.config.worker_name.clone(),
            capabilities,
            proposed_limits: self.config.limits.clone(),
            public_key: None, // TODO: Implement key generation
        };
        
        // In a real implementation, this would send HTTP request to coordinator
        // For now, return a simulated response
        Ok(self.simulate_enrollment(request).await)
    }

    /// Simulate enrollment response (for testing).
    async fn simulate_enrollment(&self, request: EnrollmentRequest) -> EnrollmentResponse {
        // Validate invitation code format
        if request.invitation_code.len() < 10 {
            return EnrollmentResponse {
                accepted: false,
                worker_id: None,
                assigned_limits: None,
                coordinator_info: CoordinatorInfo {
                    id: Uuid::new_v4(),
                    name: "BackgroundHelper Coordinator".into(),
                    contact: None,
                    public_key: None,
                    supported_workloads: vec!["range_sum".into(), "matrix_tile".into(), "prime_count".into()],
                    protocol_version: PROTOCOL_VERSION.into(),
                },
                error: Some("Invalid invitation code".into()),
            };
        }
        
        // Simulate acceptance
        let worker_id = Uuid::new_v4();
        info!("Simulated enrollment accepted for worker {}", worker_id);
        
        EnrollmentResponse {
            accepted: true,
            worker_id: Some(worker_id),
            assigned_limits: Some(request.proposed_limits),
            coordinator_info: CoordinatorInfo {
                id: Uuid::new_v4(),
                name: "BackgroundHelper Coordinator".into(),
                contact: None,
                public_key: None,
                supported_workloads: vec!["range_sum".into(), "matrix_tile".into(), "prime_count".into()],
                protocol_version: PROTOCOL_VERSION.into(),
            },
            error: None,
        }
    }

    /// Handle revocation notice.
    pub fn handle_revocation(&self, notice: RevocationNotice) {
        warn!("Worker revoked: {:?} - {}", notice.reason, notice.revoked_at);
        // In real implementation, this would stop all work and clear enrollment
    }

    /// Generate QR code data for enrollment.
    pub fn generate_qr_data(&self, invitation_code: &str) -> String {
        // Format: bh://enroll?code=XXX&name=YYY
        format!("bh://enroll?code={}&name={}", invitation_code, self.config.worker_name)
    }

    /// Parse QR code data.
    pub fn parse_qr_data(data: &str) -> Option<InvitationCode> {
        if let Some(query) = data.strip_prefix("bh://enroll?") {
            let params: std::collections::HashMap<_, _> = query.split('&')
                .filter_map(|p| p.split_once('='))
                .collect();
            
            if let Some(code) = params.get("code") {
                // Would need to fetch full invitation from coordinator
                // This is a placeholder
                return None;
            }
        }
        None
    }
}

/// Offline enrollment support (for air-gapped scenarios).
pub struct OfflineEnrollment;

impl OfflineEnrollment {
    /// Create an enrollment request file for manual transfer.
    pub fn create_request_file(config: &WorkerConfig, path: &std::path::Path) -> Result<(), String> {
        let capabilities = vec![
            HardwareCapability::Cpu {
                cores: 4,
                threads: 8,
                frequency_mhz: 3000,
                architecture: std::env::consts::ARCH.into(),
                features: vec!["avx2".into()],
            }
        ];
        
        let request = EnrollmentRequest {
            invitation_code: "MANUAL".into(), // Placeholder
            worker_name: config.worker_name.clone(),
            capabilities,
            proposed_limits: config.limits.clone(),
            public_key: None,
        };
        
        let content = serde_json::to_string_pretty(&request)
            .map_err(|e| e.to_string())?;
        
        std::fs::write(path, content)
            .map_err(|e| e.to_string())?;
        
        Ok(())
    }

    /// Parse enrollment response file.
    pub fn parse_response_file(path: &std::path::Path) -> Result<EnrollmentResponse, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| e.to_string())?;
        serde_json::from_str(&content)
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bg_helper_protocol::resource::ResourceLimits;
    use crate::config::WorkerConfig;

    #[tokio::test]
    async fn test_enrollment_manager() {
        let config = WorkerConfig::default();
        let mut manager = EnrollmentManager::new(config);
        
        let response = manager.enroll("VALID-INVITATION-CODE-123").await.unwrap();
        assert!(response.accepted);
        assert!(response.worker_id.is_some());
    }

    #[tokio::test]
    async fn test_enrollment_reject_invalid() {
        let config = WorkerConfig::default();
        let mut manager = EnrollmentManager::new(config);
        
        let response = manager.enroll("SHORT").await.unwrap();
        assert!(!response.accepted);
        assert!(response.error.is_some());
    }
}