//! Fair scheduling algorithm implementation.

use std::collections::HashMap;
use bg_helper_protocol::{
    job::{JobSpec, JobPriority, JobId},
    resource::{WorkerState, ResourceUsage, ResourceLimits, HardwareCapability},
    validation::Validatable,
};
use crate::queue::{QueuedJob, JobQueue};
use crate::strategy::PlacementStrategy;
use uuid::Uuid;
use chrono::Utc;
use tracing::{debug, trace};

/// Scheduling decision.
#[derive(Debug, Clone, PartialEq)]
pub enum ScheduleDecision {
    Assign { worker_id: Uuid },
    Defer { reason: String },
    Reject { reason: String },
}

/// Worker score for a specific job.
#[derive(Debug, Clone)]
pub struct WorkerScore {
    pub worker_id: Uuid,
    pub score: f64,
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

/// Fair scheduler using virtual time for fairness.
pub struct FairScheduler {
    /// Queued jobs
    job_queue: JobQueue,
    /// Virtual time per worker for fairness
    worker_virtual_time: HashMap<Uuid, f64>,
    /// Priority weights (higher = more important)
    priority_weights: [f64; 4],
    /// Placement strategy
    strategy: PlacementStrategy,
}

impl Default for FairScheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl FairScheduler {
    pub fn new() -> Self {
        Self {
            job_queue: JobQueue::new(),
            worker_virtual_time: HashMap::new(),
            priority_weights: [1.0, 2.0, 4.0, 8.0], // Low, Normal, High, Critical
            strategy: PlacementStrategy::default(),
        }
    }

    pub fn with_strategy(mut self, strategy: PlacementStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    /// Add a job to the internal queue.
    pub fn enqueue(&mut self, job: QueuedJob) {
        self.job_queue.enqueue(job);
    }

    /// Remove and return the highest priority job.
    pub fn dequeue(&mut self) -> Option<QueuedJob> {
        self.job_queue.dequeue()
    }

    /// Peek at the next job without removing it.
    pub fn peek(&self) -> Option<&QueuedJob> {
        self.job_queue.peek()
    }

    /// Get number of queued jobs.
    pub fn len(&self) -> usize {
        self.job_queue.len()
    }

    pub fn is_empty(&self) -> bool {
        self.job_queue.is_empty()
    }

    /// Score workers for a specific job.
    pub fn score_workers(&self, job: &JobSpec, workers: &[WorkerSnapshot]) -> Vec<WorkerScore> {
        workers.iter().map(|w| self.score_worker(job, w)).collect()
    }

    /// Score a single worker for a job.
    fn score_worker(&self, job: &JobSpec, worker: &WorkerSnapshot) -> WorkerScore {
        let mut score = 0.0;

        // Base score from virtual time (lower virtual time = higher score for fairness)
        let vtime = self.worker_virtual_time.get(&worker.worker_id).copied().unwrap_or(0.0);
        score += 1000.0 - vtime.min(1000.0);

        // Worker must be able to accept work
        let can_accept = worker.status.can_accept_work();
        if !can_accept {
            return WorkerScore {
                worker_id: worker.worker_id,
                score: -1.0,
                cpu_available: false,
                ram_available: false,
                gpu_compatible: false,
                schedule_ok: false,
                power_ok: false,
            };
        }

        // Check resource constraints
        let cpu_ok = self.check_cpu(job, &worker.usage, &worker.limits);
        let ram_ok = self.check_ram(job, &worker.usage, &worker.limits);
        let gpu_ok = self.check_gpu(job, &worker.usage, &worker.limits);
        let schedule_ok = self.check_schedule(&worker.limits);
        let power_ok = self.check_power(&worker.usage, &worker.limits);

        // Apply placement strategy scoring
        score += self.strategy.score_worker(job, worker);

        WorkerScore {
            worker_id: worker.worker_id,
            score,
            cpu_available: cpu_ok,
            ram_available: ram_ok,
            gpu_compatible: gpu_ok,
            schedule_ok,
            power_ok,
        }
    }

    fn check_cpu(&self, job: &JobSpec, usage: &ResourceUsage, limits: &ResourceLimits) -> bool {
        // Check core limit
        if let Some(max_cores) = limits.cpu_max_cores {
            let available = max_cores as f32 - usage.cpu_cores_used;
            let needed = estimate_job_cores(job);
            if needed > available {
                return false;
            }
        }
        // Check utilization limit
        if let Some(max_pct) = limits.cpu_max_utilization_pct {
            let job_pct = estimate_job_cpu_pct(job);
            if usage.cpu_utilization_pct + job_pct > max_pct as f32 {
                return false;
            }
        }
        true
    }

    fn check_ram(&self, job: &JobSpec, usage: &ResourceUsage, limits: &ResourceLimits) -> bool {
        if let Some(max_bytes) = limits.ram_max_workload_bytes {
            let job_mem = job.estimated_memory_bytes;
            let available = max_bytes.saturating_sub(usage.ram_used_bytes);
            if job_mem > available {
                return false;
            }
        }
        true
    }

    fn check_gpu(&self, job: &JobSpec, usage: &ResourceUsage, limits: &ResourceLimits) -> bool {
        // For v0.1 CPU-only, all jobs are GPU-compatible if GPU not required
        // Future: check if job needs GPU and if GPU is available/allowed
        if !limits.gpu_enabled {
            return true; // GPU workloads not enabled, but CPU jobs are fine
        }
        // Check if job requires GPU (placeholder)
        true
    }

    fn check_schedule(&self, limits: &ResourceLimits) -> bool {
        limits.schedule.as_ref().map(|s| s.is_allowed_now()).unwrap_or(true)
    }

    fn check_power(&self, usage: &ResourceUsage, limits: &ResourceLimits) -> bool {
        if usage.power_state == bg_helper_protocol::resource::PowerState::Battery {
            if !limits.power.allow_on_battery {
                return false;
            }
        }
        true
    }

    /// Makes a scheduling decision for the next job.
    pub fn schedule_next(&mut self, workers: &[WorkerSnapshot]) -> Option<ScheduleDecision> {
        let job = self.peek()?.clone();
        let scores = self.score_workers(&job.spec, workers);
        
        // Filter suitable workers
        let suitable: Vec<_> = scores.into_iter().filter(|s| s.is_suitable()).collect();
        
        if suitable.is_empty() {
            // Check if any worker could work but is temporarily unavailable
            let any_online = workers.iter().any(|w| w.status != WorkerState::Offline && w.status != WorkerState::Revoked);
            if any_online {
                return Some(ScheduleDecision::Defer { 
                    reason: "No suitable workers currently available".into() 
                });
            }
            return Some(ScheduleDecision::Reject { 
                reason: "No eligible workers in cluster".into() 
            });
        }

        // Pick highest scoring worker
        let best = suitable.into_iter().max_by(|a, b| a.score.partial_cmp(&b.score).unwrap())?;
        
        // Update virtual time for fairness
        let job_weight = self.priority_weights[job.spec.priority as usize];
        let vtime = self.worker_virtual_time.entry(best.worker_id).or_insert(0.0);
        *vtime += job_weight * job.spec.workload.estimated_cycles() as f64 / 1_000_000.0;

        Some(ScheduleDecision::Assign { worker_id: best.worker_id })
    }

    /// Record a job assignment for fairness tracking.
    pub fn record_assignment(&mut self, worker_id: Uuid, job_spec: &JobSpec) {
        let job_weight = self.priority_weights[job_spec.priority as usize];
        let vtime = self.worker_virtual_time.entry(worker_id).or_insert(0.0);
        *vtime += job_weight * job_spec.workload.estimated_cycles() as f64 / 1_000_000.0;
    }

    /// Get current virtual time for a worker.
    pub fn get_virtual_time(&self, worker_id: Uuid) -> f64 {
        self.worker_virtual_time.get(&worker_id).copied().unwrap_or(0.0)
    }

    /// Reset virtual time (e.g., on worker reconnect).
    pub fn reset_virtual_time(&mut self, worker_id: Uuid) {
        self.worker_virtual_time.remove(&worker_id);
    }
}

/// Worker snapshot for scheduling decisions.
#[derive(Debug, Clone)]
pub struct WorkerSnapshot {
    pub worker_id: Uuid,
    pub status: WorkerState,
    pub capabilities: Vec<HardwareCapability>,
    pub limits: ResourceLimits,
    pub usage: ResourceUsage,
}

impl WorkerSnapshot {
    pub fn from_protocol(snapshot: bg_helper_protocol::resource::WorkerSnapshot) -> Self {
        Self {
            worker_id: snapshot.worker_id,
            status: snapshot.state,
            capabilities: snapshot.capabilities,
            limits: snapshot.limits,
            usage: snapshot.usage,
        }
    }
}

fn estimate_job_cores(job: &JobSpec) -> f32 {
    match &job.workload {
        bg_helper_protocol::job::WorkloadType::RangeSum { .. } => 1.0,
        bg_helper_protocol::job::WorkloadType::MatrixTile { size, tile } => (*size / tile.tile_size) as f32,
        bg_helper_protocol::job::WorkloadType::PrimeCount { .. } => 1.0,
    }
}

fn estimate_job_cpu_pct(job: &JobSpec) -> f32 {
    estimate_job_cores(job) * 100.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use bg_helper_protocol::job::{JobSpec, WorkloadType, JobPriority};
    use bg_helper_protocol::resource::{ResourceLimits, ResourceUsage, WorkerState, HardwareCapability, PowerState};
    use crate::{WorkerSnapshot, QueuedJob};
    use chrono::Utc;
    use uuid::Uuid;

    fn test_worker(id: Uuid, cpu_limit: Option<u32>, util_limit: Option<u8>) -> WorkerSnapshot {
        WorkerSnapshot {
            worker_id: id,
            status: WorkerState::Idle,
            capabilities: vec![HardwareCapability::Cpu {
                cores: 4,
                threads: 8,
                frequency_mhz: 3000,
                architecture: "x86_64".into(),
                features: vec![],
            }],
            limits: ResourceLimits {
                cpu_max_cores: cpu_limit,
                cpu_max_utilization_pct: util_limit,
                ..Default::default()
            },
            usage: ResourceUsage {
                worker_id: id,
                timestamp: Utc::now(),
                cpu_utilization_pct: 10.0,
                cpu_cores_used: 0.5,
                cpu_frequency_mhz: Some(3000),
                gpu_utilization_pct: None,
                gpu_memory_used_bytes: None,
                gpu_temperature_c: None,
                ram_used_bytes: 100_000_000, // 100MB, leaving 900MB available for 1GB limit
                ram_available_bytes: 14_000_000_000,
                storage_used_bytes: 10_000_000_000,
                storage_available_bytes: 100_000_000_000,
                power_state: PowerState::Ac,
                thermal_state: bg_helper_protocol::resource::ThermalState::Normal,
            },
        }
    }

    fn test_job(workload: WorkloadType) -> JobSpec {
        JobSpec::new(workload, JobPriority::Normal, 300)
    }

    #[test]
    fn test_fair_scheduler_basic() {
        let mut scheduler = FairScheduler::new();
        let worker_id = Uuid::new_v4();
        // 10% current + 100% job = 110%, so limit must be >= 110
        let worker = test_worker(worker_id, Some(4), Some(150));
        
        let job = test_job(WorkloadType::RangeSum { start: 0, end: 1_000_000 });
        let queued = QueuedJob::new(job.clone(), Utc::now());
        scheduler.enqueue(queued);
        
        let decision = scheduler.schedule_next(&[worker.clone()]);
        assert!(matches!(decision, Some(ScheduleDecision::Assign { worker_id: w }) if w == worker_id));
    }

    #[test]
    fn test_fair_scheduler_multiple_workers() {
        let mut scheduler = FairScheduler::new();
        let worker1 = Uuid::new_v4();
        let worker2 = Uuid::new_v4();
        
        // 10% current + 100% job = 110%, so limit must be >= 110
        let w1 = test_worker(worker1, Some(4), Some(150));
        let w2 = test_worker(worker2, Some(4), Some(150));
        
        // Add two jobs
        let job1 = test_job(WorkloadType::RangeSum { start: 0, end: 1_000_000 });
        let job2 = test_job(WorkloadType::RangeSum { start: 1_000_000, end: 2_000_000 });
        
        scheduler.enqueue(QueuedJob::new(job1, Utc::now()));
        scheduler.enqueue(QueuedJob::new(job2, Utc::now()));
        
        // First job should go to one of the workers
        let d1 = scheduler.schedule_next(&[w1.clone(), w2.clone()]);
        let worker1_id = if let Some(ScheduleDecision::Assign { worker_id }) = d1.clone() {
            worker_id
        } else {
            panic!("Expected Assign decision");
        };
        
        // First job should be assigned to one of the workers
        assert!(worker1_id == worker1 || worker1_id == worker2);
        
        // Second job should go to the other worker
        let d2 = scheduler.schedule_next(&[w1, w2]);
        if let Some(ScheduleDecision::Assign { worker_id }) = d2 {
            assert!(worker_id != worker1_id, "Second job should go to different worker");
        } else {
            panic!("Expected Assign decision");
        }
    }

    #[test]
    fn test_fair_scheduler_cpu_limit() {
        let mut scheduler = FairScheduler::new();
        let worker_id = Uuid::new_v4();
        
        // Worker with 1 core limit, already using 0.9
        // cpu_cores_used = 0.9 means only 0.1 cores available
        // Job needs 1 core, so worker cannot accept
        let worker = test_worker(worker_id, Some(1), Some(150)); // Allow for util check, RAM should work
        let mut worker = worker;
        worker.usage.cpu_cores_used = 0.9;
        
        let job = test_job(WorkloadType::RangeSum { start: 0, end: 1_000_000 });
        scheduler.enqueue(QueuedJob::new(job, Utc::now()));
        
        let decision = scheduler.schedule_next(&[worker]);
        // Worker is online but cannot accept - should defer (might become available)
        assert!(matches!(decision, Some(ScheduleDecision::Defer { .. })));
    }

    #[test]
    fn test_fair_scheduler_utilization_limit() {
        let mut scheduler = FairScheduler::new();
        let worker_id = Uuid::new_v4();
        
        // Worker at 45% utilization, limit 50%
        // 45% + 100% (job estimate) = 145% > 50% limit
        let mut worker = test_worker(worker_id, Some(4), Some(60)); // Use 60% limit to avoid rejection due to RAM
        worker.usage.cpu_utilization_pct = 45.0;
        
        let job = test_job(WorkloadType::RangeSum { start: 0, end: 1_000_000 });
        scheduler.enqueue(QueuedJob::new(job, Utc::now()));
        
        let decision = scheduler.schedule_next(&[worker]);
        // Worker is online but cannot accept - should defer (might become available)
        assert!(matches!(decision, Some(ScheduleDecision::Defer { .. })));
    }

    #[test]
    fn test_fair_scheduler_priority() {
        let mut scheduler = FairScheduler::new();
        let worker_id = Uuid::new_v4();
        // 10% current + 100% job = 110%, so limit must be >= 110
        let worker = test_worker(worker_id, Some(4), Some(150));
        
        // Add low priority job first
        let job_low = test_job(WorkloadType::RangeSum { start: 0, end: 1_000_000 });
        let mut job_low = job_low;
        job_low.priority = JobPriority::Low;
        scheduler.enqueue(QueuedJob::new(job_low.clone(), Utc::now()));
        
        // Add high priority job
        let job_high = test_job(WorkloadType::RangeSum { start: 1_000_000, end: 2_000_000 });
        let mut job_high = job_high;
        job_high.priority = JobPriority::High;
        scheduler.enqueue(QueuedJob::new(job_high.clone(), Utc::now()));
        
        // High priority should be scheduled first
        let decision = scheduler.schedule_next(&[worker.clone()]);
        eprintln!("Decision: {:?}", decision);
        assert!(matches!(decision, Some(ScheduleDecision::Assign { .. })));
        
        // The high priority job should be the one assigned
        let assigned_job = scheduler.dequeue().unwrap();
        assert_eq!(assigned_job.spec.priority, JobPriority::High);
    }

    #[test]
    fn test_worker_snapshot_conversion() {
        let proto_snapshot = bg_helper_protocol::resource::WorkerSnapshot {
            worker_id: Uuid::new_v4(),
            capabilities: vec![],
            limits: ResourceLimits::default(),
            usage: bg_helper_protocol::resource::ResourceUsage {
                worker_id: Uuid::new_v4(),
                timestamp: Utc::now(),
                cpu_utilization_pct: 10.0,
                cpu_cores_used: 0.5,
                cpu_frequency_mhz: None,
                gpu_utilization_pct: None,
                gpu_memory_used_bytes: None,
                gpu_temperature_c: None,
                ram_used_bytes: 1_000_000_000,
                ram_available_bytes: 15_000_000_000,
                storage_used_bytes: 10_000_000_000,
                storage_available_bytes: 100_000_000_000,
                power_state: PowerState::Ac,
                thermal_state: bg_helper_protocol::resource::ThermalState::Normal,
            },
            state: WorkerState::Idle,
            enrolled_at: Utc::now(),
            last_heartbeat: Utc::now(),
            version: "0.1.0".into(),
        };
        
        let snapshot = WorkerSnapshot::from_protocol(proto_snapshot);
        assert_eq!(snapshot.status, WorkerState::Idle);
    }
}