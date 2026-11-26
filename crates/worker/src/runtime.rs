//! Worker runtime - main event loop and coordination.

use crate::config::{ConfigManager, RuntimeConfig, WorkerConfig};
use crate::enrollment::EnrollmentManager;
use crate::executor::{ExecutionError, JobExecutor};
use crate::monitor::{run_monitor, ResourceMonitor};
use crate::ui::{CurrentJobInfo, OwnerStatus, TextUI};
use bg_helper_protocol::{
    enrollment::{EnrollmentRequest, EnrollmentResponse},
    job::{JobId, JobResult, JobSpec, WorkerId},
    resource::{ResourceLimits, ResourceUsage, WorkerState},
};
use chrono::Utc;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Worker runtime state.
pub struct WorkerRuntime {
    config_manager: ConfigManager,
    enrollment_manager: EnrollmentManager,
    monitor: ResourceMonitor,
    executor: JobExecutor,
    runtime_config: Arc<RwLock<Option<RuntimeConfig>>>,
    current_job: Arc<RwLock<Option<JobId>>>,
    current_job_info: Arc<RwLock<Option<CurrentJobInfo>>>,
    state: Arc<RwLock<WorkerState>>,
    sharing_enabled: Arc<RwLock<bool>>,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl WorkerRuntime {
    /// Create a new worker runtime.
    pub async fn new() -> Result<Self, String> {
        let config_manager = ConfigManager::new().await?;
        let config = config_manager.get().await;

        let worker_id = config.worker_id.unwrap_or_else(Uuid::new_v4);
        let monitor = ResourceMonitor::new(worker_id);
        let enrollment_manager = EnrollmentManager::new(config.clone());
        let executor = JobExecutor::new(worker_id, config.limits.clone());

        Ok(Self {
            config_manager,
            enrollment_manager,
            monitor,
            executor,
            runtime_config: Arc::new(RwLock::new(None)),
            current_job: Arc::new(RwLock::new(None)),
            current_job_info: Arc::new(RwLock::new(None)),
            state: Arc::new(RwLock::new(WorkerState::Offline)),
            sharing_enabled: Arc::new(RwLock::new(false)),
            shutdown_tx: None,
        })
    }

    /// Start the worker runtime.
    pub async fn start(&mut self) -> Result<(), String> {
        info!("Starting BackgroundHelper worker");

        let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel();
        self.shutdown_tx = Some(shutdown_tx);

        // Load runtime config if enrolled
        let config = self.config_manager.get().await;
        if let Some(rt_config) = RuntimeConfig::from_config(&config) {
            let worker_id = rt_config.worker_id;
            *self.runtime_config.write().await = Some(rt_config);
            *self.state.write().await = WorkerState::Online;
            *self.sharing_enabled.write().await = true;
            info!("Worker enrolled as {}", worker_id);
        } else {
            info!("Worker not enrolled yet");
        }

        // Start resource monitor
        let (usage_tx, mut usage_rx) = mpsc::unbounded_channel();
        let worker_id_for_monitor = self
            .runtime_config
            .read()
            .await
            .as_ref()
            .map(|c| c.worker_id)
            .unwrap_or_else(Uuid::new_v4);
        let monitor = ResourceMonitor::new(worker_id_for_monitor);
        tokio::spawn(run_monitor(monitor, Duration::from_secs(5), usage_tx));

        // Main event loop
        let _runtime_config = self.runtime_config.clone();
        let _current_job = self.current_job.clone();
        let _current_job_info = self.current_job_info.clone();
        let _state = self.state.clone();
        let _sharing_enabled = self.sharing_enabled.clone();

        // For now, just run the monitor loop
        tokio::select! {
            _ = async {
                while let Some(usage) = usage_rx.recv().await {
                    // Update state based on usage
                    debug!("Resource usage: CPU {:.1}%, RAM {} bytes", usage.cpu_utilization_pct, usage.ram_used_bytes);
                }
            } => {},
            _ = &mut shutdown_rx => {
                info!("Shutdown signal received");
            }
        }

        Ok(())
    }

    /// Stop the worker runtime.
    pub async fn stop(&mut self) {
        info!("Stopping worker");
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        *self.state.write().await = WorkerState::Offline;
        *self.sharing_enabled.write().await = false;
    }

    /// Enroll with coordinator using invitation code.
    pub async fn enroll(&mut self, invitation_code: &str) -> Result<EnrollmentResponse, String> {
        let response = self.enrollment_manager.enroll(invitation_code).await?;

        if response.accepted {
            if let (Some(worker_id), Some(limits)) =
                (response.worker_id, response.assigned_limits.clone())
            {
                // Update config
                self.config_manager
                    .update(|c| {
                        c.worker_id = Some(worker_id);
                        c.limits = limits.clone();
                        c.coordinator_url = Some("http://localhost:8080".into()); // Would come from invitation
                        c.enrollment_token = Some("token".into()); // Would come from response
                    })
                    .await?;

                // Update runtime config
                let rt_config = RuntimeConfig {
                    worker_id,
                    worker_name: self.config_manager.get().await.worker_name,
                    limits: limits.clone(),
                    coordinator_url: "http://localhost:8080".into(),
                    enrollment_token: "token".into(),
                };
                *self.runtime_config.write().await = Some(rt_config);
                *self.state.write().await = WorkerState::Online;
                *self.sharing_enabled.write().await = true;

                // Recreate executor with new limits
                self.executor = JobExecutor::new(worker_id, self.config_manager.get().await.limits);

                info!("Enrollment successful: {}", worker_id);
            }
        }

        Ok(response)
    }

    /// Revoke enrollment.
    pub async fn revoke(&mut self) -> Result<(), String> {
        self.config_manager.clear_enrollment().await?;
        *self.runtime_config.write().await = None;
        *self.state.write().await = WorkerState::Revoked;
        *self.sharing_enabled.write().await = false;
        self.current_job.write().await.take();
        self.current_job_info.write().await.take();
        info!("Enrollment revoked");
        Ok(())
    }

    /// Pause sharing (owner control).
    pub async fn pause_sharing(&self) {
        *self.sharing_enabled.write().await = false;
        *self.state.write().await = WorkerState::Paused;
        info!("Sharing paused by owner");
    }

    /// Resume sharing (owner control).
    pub async fn resume_sharing(&self) {
        if self.runtime_config.read().await.is_some() {
            *self.sharing_enabled.write().await = true;
            *self.state.write().await = WorkerState::Idle;
            info!("Sharing resumed by owner");
        }
    }

    /// Execute a job (called by coordinator via message passing).
    pub async fn execute_job(&self, job_spec: JobSpec) -> Result<JobResult, ExecutionError> {
        let job_id = job_spec.id;

        // Update state
        *self.current_job.write().await = Some(job_id);
        *self.state.write().await = WorkerState::Busy;

        // Create job info for UI
        let workload_type = match &job_spec.workload {
            bg_helper_protocol::job::WorkloadType::RangeSum { .. } => "range_sum",
            bg_helper_protocol::job::WorkloadType::MatrixTile { .. } => "matrix_tile",
            bg_helper_protocol::job::WorkloadType::PrimeCount { .. } => "prime_count",
        };

        *self.current_job_info.write().await = Some(CurrentJobInfo {
            job_id: job_id.to_string(),
            workload_type: workload_type.into(),
            progress_pct: 0.0,
            elapsed: Duration::from_secs(0),
            estimated_remaining: Some(Duration::from_secs(job_spec.max_duration_secs as u64)),
        });

        // Execute
        let result = self.executor.execute(job_spec).await;

        // Clear current job
        *self.current_job.write().await = None;
        *self.current_job_info.write().await = None;
        *self.state.write().await = WorkerState::Idle;

        result
    }

    /// Get current owner status for UI.
    pub async fn get_owner_status(&mut self) -> OwnerStatus {
        let config = self.config_manager.get().await;
        let usage = self.monitor.collect_usage();
        let state = *self.state.read().await;
        let sharing = *self.sharing_enabled.read().await;
        let current_job = self.current_job.read().await.clone();
        let current_job_info = self.current_job_info.read().await.clone();

        OwnerStatus {
            worker_name: config.worker_name,
            current_job: current_job_info,
            resource_usage: usage,
            limits: config.limits,
            state,
            sharing_active: sharing,
            last_update: std::time::SystemTime::now(),
        }
    }

    /// Render text UI.
    pub async fn render_text_ui(&mut self) -> String {
        let status = self.get_owner_status().await;
        let ui = TextUI::new(status);
        ui.render()
    }

    /// Get compact status line.
    pub async fn get_compact_status(&mut self) -> String {
        let status = self.get_owner_status().await;
        let ui = TextUI::new(status);
        ui.render_compact()
    }
}

impl Clone for JobExecutor {
    fn clone(&self) -> Self {
        Self::new(self.worker_id().0, self.limits().clone())
    }
}

/// Run worker as a background service (placeholder for systemd/launchd integration).
pub async fn run_service() -> Result<(), String> {
    let mut runtime = WorkerRuntime::new().await?;
    runtime.start().await?;

    // In a real service, this would run until shutdown signal
    // For now, just print status and exit
    let status = runtime.get_compact_status().await;
    println!("{}", status);

    runtime.stop().await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bg_helper_protocol::job::{JobPriority, JobSpec, WorkloadType};
    use bg_helper_protocol::resource::ResourceLimits;

    #[tokio::test]
    async fn test_runtime_creation() {
        let mut runtime = WorkerRuntime::new().await.unwrap();
        let status = runtime.get_owner_status().await;
        assert_eq!(status.state, WorkerState::Offline);
    }

    #[tokio::test]
    async fn test_pause_resume() {
        let mut runtime = WorkerRuntime::new().await.unwrap();

        // Can't resume if not enrolled
        runtime.resume_sharing().await;
        let status = runtime.get_owner_status().await;
        assert!(!status.sharing_active);

        // Pause should work regardless
        runtime.pause_sharing().await;
        let status = runtime.get_owner_status().await;
        assert_eq!(status.state, WorkerState::Paused);
    }
}
