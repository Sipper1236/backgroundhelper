//! Coordinator state management.

use bg_helper_protocol::{
    enrollment::{
        CoordinatorInfo, EnrollmentRequest, EnrollmentResponse, RevocationNotice, RevocationReason,
    },
    job::{
        Job, JobId, JobOutput, JobPriority, JobResult, JobSpec, JobState, WorkerId, WorkloadType,
    },
    messages::{
        CoordinatorEnvelope, CoordinatorMessage, MessageEnvelope, WorkerEnvelope, WorkerMessage,
        PROTOCOL_VERSION,
    },
    resource::{HardwareCapability, ResourceLimits, ResourceUsage, WorkerSnapshot, WorkerState},
    validation::{Validatable, ValidationError},
};
use bg_helper_scheduler::{FairScheduler, JobQueue, WorkerSnapshot as SchedulerWorkerSnapshot};
use chrono::{DateTime, Duration, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Count primes in range (for simulation).
fn count_primes(start: u64, end: u64) -> u64 {
    let mut count = 0;
    for n in start..=end {
        if is_prime(n) {
            count += 1;
        }
    }
    count
}

/// Check if a number is prime.
fn is_prime(n: u64) -> bool {
    if n < 2 {
        return false;
    }
    if n == 2 {
        return true;
    }
    if n % 2 == 0 {
        return false;
    }
    let mut i = 3u64;
    while i * i <= n {
        if n % i == 0 {
            return false;
        }
        i += 2;
    }
    true
}

#[derive(Debug, Error)]
pub enum CoordinatorError {
    #[error("Worker not found: {0}")]
    WorkerNotFound(Uuid),
    #[error("Job not found: {0}")]
    JobNotFound(JobId),
    #[error("Invalid state transition: {0}")]
    InvalidStateTransition(String),
    #[error("Validation error: {0}")]
    Validation(#[from] ValidationError),
    #[error("Scheduler error: {0}")]
    Scheduler(String),
    #[error("Policy error: {0}")]
    Policy(String),
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// Coordinator configuration.
#[derive(Debug, Clone)]
pub struct CoordinatorConfig {
    pub coordinator_id: Uuid,
    pub name: String,
    pub contact: Option<String>,
    pub default_limits: ResourceLimits,
    pub supported_workloads: Vec<String>,
    pub offer_timeout_secs: u32,
    pub heartbeat_interval_secs: u32,
    pub max_retries: u32,
}

impl Default for CoordinatorConfig {
    fn default() -> Self {
        Self {
            coordinator_id: Uuid::new_v4(),
            name: "BackgroundHelper Coordinator".into(),
            contact: None,
            default_limits: ResourceLimits::default(),
            supported_workloads: vec![
                "range_sum".into(),
                "matrix_tile".into(),
                "prime_count".into(),
            ],
            offer_timeout_secs: 60,
            heartbeat_interval_secs: 30,
            max_retries: 3,
        }
    }
}

/// Coordinator state.
pub struct CoordinatorState {
    config: CoordinatorConfig,
    /// Registered workers
    workers: Arc<RwLock<HashMap<Uuid, WorkerSnapshot>>>,
    /// Job queue and scheduler
    queue: Arc<RwLock<JobQueue>>,
    scheduler: Arc<RwLock<FairScheduler>>,
    /// Active jobs (offered, approved, running)
    active_jobs: Arc<RwLock<HashMap<JobId, Job>>>,
    /// Completed jobs history
    job_history: Arc<RwLock<HashMap<JobId, Job>>>,
    /// Pending job offers awaiting worker response
    pending_offers: Arc<RwLock<HashMap<JobId, (Uuid, DateTime<Utc>)>>>,
}

impl CoordinatorState {
    pub fn new(config: CoordinatorConfig) -> Self {
        Self {
            config,
            workers: Arc::new(RwLock::new(HashMap::new())),
            queue: Arc::new(RwLock::new(JobQueue::new())),
            scheduler: Arc::new(RwLock::new(FairScheduler::new())),
            active_jobs: Arc::new(RwLock::new(HashMap::new())),
            job_history: Arc::new(RwLock::new(HashMap::new())),
            pending_offers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get coordinator info for enrollment responses.
    pub fn coordinator_info(&self) -> CoordinatorInfo {
        CoordinatorInfo {
            id: self.config.coordinator_id,
            name: self.config.name.clone(),
            contact: self.config.contact.clone(),
            public_key: None,
            supported_workloads: self.config.supported_workloads.clone(),
            protocol_version: PROTOCOL_VERSION.into(),
        }
    }

    /// Handle worker enrollment request.
    pub async fn handle_enrollment(
        &self,
        request: EnrollmentRequest,
    ) -> Result<EnrollmentResponse, CoordinatorError> {
        request.validate()?;

        // Verify invitation code (simplified - in production would check against issued codes)
        // For now, accept any non-empty code
        if request.invitation_code.trim().is_empty() {
            return Ok(EnrollmentResponse {
                accepted: false,
                worker_id: None,
                assigned_limits: None,
                coordinator_info: self.coordinator_info(),
                error: Some("Invalid invitation code".into()),
            });
        }

        let worker_id = Uuid::new_v4();
        info!("Enrolling worker {} ({})", request.worker_name, worker_id);

        // Merge proposed limits with defaults (local limits take precedence for restrictions)
        let assigned_limits =
            bg_helper_policy::merge_limits(&request.proposed_limits, &self.config.default_limits);

        // Validate merged limits
        assigned_limits.validate()?;

        // Create worker snapshot
        let snapshot = WorkerSnapshot {
            worker_id,
            capabilities: request.capabilities.clone(),
            limits: assigned_limits.clone(),
            usage: ResourceUsage {
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
            state: WorkerState::Online,
            enrolled_at: Utc::now(),
            last_heartbeat: Utc::now(),
            version: PROTOCOL_VERSION.into(),
        };

        // Register worker
        self.workers.write().await.insert(worker_id, snapshot);

        Ok(EnrollmentResponse {
            accepted: true,
            worker_id: Some(worker_id),
            assigned_limits: Some(assigned_limits),
            coordinator_info: self.coordinator_info(),
            error: None,
        })
    }

    /// Handle worker heartbeat.
    pub async fn handle_heartbeat(
        &self,
        worker_id: Uuid,
        usage: ResourceUsage,
        state: WorkerState,
        current_job: Option<JobId>,
    ) -> Result<(), CoordinatorError> {
        let mut workers = self.workers.write().await;
        if let Some(worker) = workers.get_mut(&worker_id) {
            worker.usage = usage;
            worker.state = state;
            worker.last_heartbeat = Utc::now();
            Ok(())
        } else {
            Err(CoordinatorError::WorkerNotFound(worker_id))
        }
    }

    /// Submit a new job to the coordinator.
    pub async fn submit_job(&self, spec: JobSpec) -> Result<JobId, CoordinatorError> {
        spec.validate()?;

        let job = Job::new(spec);
        let job_id = job.spec.id;

        // Add to both queues for tracking and scheduling
        let queued_job = bg_helper_scheduler::queue::QueuedJob::new(job.spec.clone(), Utc::now());
        self.queue.write().await.enqueue(queued_job.clone());
        self.scheduler.write().await.enqueue(queued_job);

        info!("Job {} queued", job_id);
        Ok(job_id)
    }

    /// Submit a simple range sum job.
    pub async fn submit_range_sum(
        &self,
        start: u64,
        end: u64,
        priority: JobPriority,
        max_duration_secs: u32,
    ) -> Result<JobId, CoordinatorError> {
        let spec = JobSpec::new(
            WorkloadType::RangeSum { start, end },
            priority,
            max_duration_secs,
        );
        self.submit_job(spec).await
    }

    /// Run the scheduling loop: assigns queued jobs to available workers.
    pub async fn run_scheduling_cycle(&self) -> Result<Vec<CoordinatorMessage>, CoordinatorError> {
        let mut messages = Vec::new();

        // Get current worker snapshots
        let workers = self.get_worker_snapshots().await;

        // Run scheduler
        let mut scheduler = self.scheduler.write().await;
        let mut queue = self.queue.write().await;

        // Process all available jobs
        for _ in 0..queue.len() {
            match scheduler.schedule_next(&workers) {
                Some(bg_helper_scheduler::ScheduleDecision::Assign { worker_id }) => {
                    // Dequeue job from scheduler's internal queue
                    let job = scheduler.dequeue().ok_or_else(|| {
                        CoordinatorError::Scheduler("Job dequeued but not found".into())
                    })?;
                    let job_id = job.spec.id;
                    let offer_expires =
                        Utc::now() + Duration::seconds(self.config.offer_timeout_secs as i64);

                    // Record assignment for fairness (already done in scheduler)
                    scheduler.record_assignment(worker_id, &job.spec);

                    // Create job object and add to active jobs
                    let mut job_obj = Job::new(job.spec);
                    job_obj.state = JobState::Offered;
                    job_obj.assigned_worker = Some(WorkerId(worker_id));

                    // Add to active jobs for tracking
                    self.active_jobs
                        .write()
                        .await
                        .insert(job_id, job_obj.clone());

                    // Mark as assigned in queue
                    queue.assign(job_id, worker_id, self.config.offer_timeout_secs);

                    messages.push(CoordinatorMessage::JobOffer {
                        job: job_obj,
                        offer_expires_at: offer_expires,
                    });

                    // Track pending offer
                    let mut pending = self.pending_offers.write().await;
                    pending.insert(job_id, (worker_id, offer_expires));
                }
                Some(bg_helper_scheduler::ScheduleDecision::Defer { reason }) => {
                    debug!("Scheduling deferred: {}", reason);
                    break; // No more suitable workers now
                }
                Some(bg_helper_scheduler::ScheduleDecision::Reject { reason }) => {
                    warn!("Job rejected: {}", reason);
                    break;
                }
                None => break, // No more jobs
            }
        }

        // Expire old offers
        let expired = queue.expire_offers();
        for job_id in expired {
            let mut pending = self.pending_offers.write().await;
            if let Some((worker_id, _)) = pending.remove(&job_id) {
                messages.push(CoordinatorMessage::JobCancel {
                    job_id,
                    reason: "Offer expired".into(),
                });
            }
        }

        debug!(
            "Scheduling cycle completed with {} messages",
            messages.len()
        );
        Ok(messages)
    }

    /// Handle job acceptance from worker.
    pub async fn handle_job_accept(
        &self,
        job_id: JobId,
        worker_id: Uuid,
    ) -> Result<(), CoordinatorError> {
        let mut pending = self.pending_offers.write().await;
        if let Some((assigned_worker, _)) = pending.remove(&job_id) {
            if assigned_worker != worker_id {
                return Err(CoordinatorError::InvalidStateTransition(format!(
                    "Job {} accepted by wrong worker",
                    job_id
                )));
            }

            // Update job state
            let mut active = self.active_jobs.write().await;
            if let Some(job) = active.get_mut(&job_id) {
                job.transition_to(JobState::Approved)?;
                job.transition_to(JobState::Running)?;
            } else {
                // Job might be in queue still
                let mut queue = self.queue.write().await;
                if let Some(pos) = queue
                    .all_assigned()
                    .iter()
                    .position(|j| j.spec.id == job_id)
                {
                    let assigned = queue.all_assigned()[pos].clone();
                    let mut job = Job::new(assigned.spec);
                    job.transition_to(JobState::Approved)?;
                    job.transition_to(JobState::Running)?;
                    active.insert(job_id, job);
                }
            }

            info!(
                "Job {} accepted by worker {}, now running",
                job_id, worker_id
            );
            Ok(())
        } else {
            Err(CoordinatorError::InvalidStateTransition(
                "Job not in pending offers".into(),
            ))
        }
    }

    /// Accept all pending offers for a worker (simulation helper).
    pub async fn accept_all_offers_for_worker(
        &self,
        worker_id: Uuid,
    ) -> Result<(), CoordinatorError> {
        let pending: Vec<JobId> = {
            let pending = self.pending_offers.read().await;
            pending
                .iter()
                .filter(|(_, (wid, _))| *wid == worker_id)
                .map(|(job_id, _)| job_id.clone())
                .collect()
        };

        debug!(
            "accept_all_offers_for_worker: worker {} has {} pending offers",
            worker_id,
            pending.len()
        );

        for job_id in pending {
            debug!("Accepting job {} for worker {}", job_id, worker_id);
            self.handle_job_accept(job_id, worker_id).await?;
        }

        Ok(())
    }

    /// Handle job rejection from worker.
    pub async fn handle_job_reject(
        &self,
        job_id: JobId,
        worker_id: Uuid,
        reason: String,
    ) -> Result<(), CoordinatorError> {
        let mut pending = self.pending_offers.write().await;
        if let Some((assigned_worker, _)) = pending.remove(&job_id) {
            if assigned_worker != worker_id {
                return Err(CoordinatorError::InvalidStateTransition(format!(
                    "Job {} rejected by wrong worker",
                    job_id
                )));
            }

            // Re-queue the job for another worker
            let mut queue = self.queue.write().await;
            queue.requeue_assigned(job_id);

            // Reset scheduler virtual time for this worker? No, keep fairness.

            info!(
                "Job {} rejected by worker {}: {}",
                job_id, worker_id, reason
            );
            Ok(())
        } else {
            Err(CoordinatorError::InvalidStateTransition(
                "Job not in pending offers".into(),
            ))
        }
    }

    /// Handle job completion from worker.
    pub async fn handle_job_complete(
        &self,
        job_id: JobId,
        worker_id: Uuid,
        result: JobResult,
    ) -> Result<(), CoordinatorError> {
        let mut active = self.active_jobs.write().await;
        if let Some(job) = active.remove(&job_id) {
            if job.assigned_worker != Some(WorkerId(worker_id)) {
                return Err(CoordinatorError::InvalidStateTransition(format!(
                    "Job {} completed by wrong worker",
                    job_id
                )));
            }

            let mut completed_job = job;
            completed_job.state = JobState::Completed;
            completed_job.result = Some(result);
            completed_job.completed_at = Some(Utc::now());

            // Move to history
            self.job_history.write().await.insert(job_id, completed_job);

            // Remove from pending offers if still there
            self.pending_offers.write().await.remove(&job_id);

            info!("Job {} completed successfully", job_id);
            Ok(())
        } else {
            Err(CoordinatorError::JobNotFound(job_id))
        }
    }

    /// Handle job failure from worker.
    pub async fn handle_job_fail(
        &self,
        job_id: JobId,
        worker_id: Uuid,
        error: String,
    ) -> Result<(), CoordinatorError> {
        let mut active = self.active_jobs.write().await;
        if let Some(job) = active.remove(&job_id) {
            if job.assigned_worker != Some(WorkerId(worker_id)) {
                return Err(CoordinatorError::InvalidStateTransition(format!(
                    "Job {} failed by wrong worker",
                    job_id
                )));
            }

            let mut failed_job = job;
            failed_job.state = JobState::Failed;
            failed_job.error_message = Some(error);
            failed_job.completed_at = Some(Utc::now());

            // Clone for potential retry before moving to history
            let retry_count = failed_job.retry_count;
            let max_retries = self.config.max_retries;
            let spec_clone = failed_job.spec.clone();

            // Move to history
            self.job_history.write().await.insert(job_id, failed_job);

            // Remove from pending offers
            self.pending_offers.write().await.remove(&job_id);

            // Retry logic
            if retry_count < max_retries {
                let new_job = Job::new(spec_clone);

                let queued =
                    bg_helper_scheduler::queue::QueuedJob::new(new_job.spec.clone(), Utc::now());
                self.queue.write().await.enqueue(queued.clone());
                self.scheduler.write().await.enqueue(queued);

                info!(
                    "Job {} failed, re-queued for retry ({}/{})",
                    job_id,
                    retry_count + 1,
                    max_retries
                );
            } else {
                warn!("Job {} failed after {} retries", job_id, max_retries);
            }

            Ok(())
        } else {
            Err(CoordinatorError::JobNotFound(job_id))
        }
    }

    /// Revoke a worker.
    pub async fn revoke_worker(
        &self,
        worker_id: Uuid,
        reason: RevocationReason,
    ) -> Result<(), CoordinatorError> {
        let mut workers = self.workers.write().await;
        if let Some(worker) = workers.get_mut(&worker_id) {
            worker.state = WorkerState::Revoked;

            // Cancel any pending offers for this worker
            let mut pending = self.pending_offers.write().await;
            let expired_jobs: Vec<_> = pending
                .iter()
                .filter(|(_, (w, _))| *w == worker_id)
                .map(|(job_id, _)| *job_id)
                .collect();

            for job_id in expired_jobs {
                pending.remove(&job_id);
                // Re-queue the job
                let mut queue = self.queue.write().await;
                queue.requeue_assigned(job_id);
            }

            info!("Worker {} revoked: {:?}", worker_id, reason);
            Ok(())
        } else {
            Err(CoordinatorError::WorkerNotFound(worker_id))
        }
    }

    /// Get worker snapshots for scheduling.
    async fn get_worker_snapshots(&self) -> Vec<SchedulerWorkerSnapshot> {
        let workers = self.workers.read().await;
        workers
            .values()
            .filter(|w| w.state.can_accept_work())
            .map(|w| SchedulerWorkerSnapshot::from_protocol(w.clone()))
            .collect()
    }

    /// Get all workers.
    pub async fn get_workers(&self) -> Vec<WorkerSnapshot> {
        self.workers.read().await.values().cloned().collect()
    }

    /// Get a specific worker.
    pub async fn get_worker(&self, worker_id: Uuid) -> Option<WorkerSnapshot> {
        self.workers.read().await.get(&worker_id).cloned()
    }

    /// Get active jobs.
    pub async fn get_active_jobs(&self) -> Vec<Job> {
        self.active_jobs.read().await.values().cloned().collect()
    }

    /// Get job history.
    pub async fn get_job_history(&self) -> Vec<Job> {
        self.job_history.read().await.values().cloned().collect()
    }

    /// Get a specific job.
    pub async fn get_job(&self, job_id: JobId) -> Option<Job> {
        let active = self.active_jobs.read().await;
        if let Some(job) = active.get(&job_id) {
            return Some(job.clone());
        }
        self.job_history.read().await.get(&job_id).cloned()
    }

    /// Get queue status.
    pub async fn get_queue_status(&self) -> (usize, usize, usize) {
        let queue = self.queue.read().await;
        (queue.len(), queue.assigned_len(), queue.completed().len())
    }

    /// Simulate completing all jobs offered to the given worker IDs.
    /// This is used for testing/simulation purposes.
    pub async fn complete_jobs_for_workers(
        &self,
        worker_ids: &[Uuid],
    ) -> Result<(usize, usize), CoordinatorError> {
        let mut completed_count = 0;
        let mut failed_count = 0;

        // First, collect all job IDs for each worker (release read lock before processing)
        let job_ids_by_worker: Vec<(Uuid, Vec<JobId>)> = {
            let active = self.active_jobs.read().await;
            worker_ids
                .iter()
                .map(|worker_id| {
                    let job_ids: Vec<JobId> = active
                        .iter()
                        .filter(|(_, job)| {
                            job.assigned_worker
                                .map(|w| w == WorkerId(*worker_id))
                                .unwrap_or(false)
                        })
                        .filter(|(job_id, job)| {
                            job.state != JobState::Completed && job.state != JobState::Failed
                        })
                        .map(|(job_id, _)| job_id.clone())
                        .collect();
                    (*worker_id, job_ids)
                })
                .collect()
        };

        // Now process each worker's jobs
        for (worker_id, job_ids) in job_ids_by_worker {
            for job_id in job_ids {
                let output = {
                    let active = self.active_jobs.read().await;
                    match &active.get(&job_id) {
                        Some(job) => {
                            // Simulate execution based on workload type
                            match &job.spec.workload {
                                bg_helper_protocol::job::WorkloadType::RangeSum { start, end } => {
                                    let sum: u128 = (*start..=*end).map(|x| x as u128).sum();
                                    JobOutput::RangeSum { sum }
                                }
                                bg_helper_protocol::job::WorkloadType::MatrixTile {
                                    size,
                                    tile,
                                } => {
                                    let tile_rows = size / tile.tile_size;
                                    let tile_cols = size / tile.tile_size;
                                    JobOutput::MatrixTile {
                                        tile_data: vec![1.0; tile_rows * tile_cols],
                                        rows: tile_rows,
                                        cols: tile_cols,
                                    }
                                }
                                bg_helper_protocol::job::WorkloadType::PrimeCount {
                                    start,
                                    end,
                                } => {
                                    let count = count_primes(*start, *end);
                                    JobOutput::PrimeCount { count }
                                }
                            }
                        }
                        None => continue,
                    }
                };

                // Complete the job
                if let Err(_) = self
                    .handle_job_complete(
                        job_id,
                        worker_id,
                        JobResult {
                            job_id,
                            worker_id: WorkerId(worker_id),
                            output,
                            started_at: Utc::now(),
                            completed_at: Utc::now(),
                            cpu_cycles_used: 0,
                            peak_memory_bytes: 0,
                        },
                    )
                    .await
                {
                    failed_count += 1;
                } else {
                    completed_count += 1;
                }
            }
        }

        Ok((completed_count, failed_count))
    }

    /// Clean up stale workers (no heartbeat for 2x interval).
    pub async fn cleanup_stale_workers(&self) -> Vec<Uuid> {
        let threshold =
            Utc::now() - Duration::seconds((self.config.heartbeat_interval_secs * 2) as i64);
        let mut workers = self.workers.write().await;
        let stale: Vec<_> = workers
            .iter()
            .filter(|(_, w)| w.last_heartbeat < threshold && w.state != WorkerState::Revoked)
            .map(|(id, _)| *id)
            .collect();

        for id in &stale {
            if let Some(w) = workers.get_mut(id) {
                w.state = WorkerState::Offline;
            }
        }

        stale
    }
}
