//! Local simulation for testing with simulated workers.

use crate::state::CoordinatorState;
use crate::CoordinatorConfig;
use bg_helper_protocol::{
    enrollment::EnrollmentRequest,
    job::{JobId, JobOutput, JobPriority, JobResult, JobSpec, JobState, WorkerId, WorkloadType},
    resource::{
        HardwareCapability, PowerPolicy, PowerState, ResourceLimits, ResourceUsage, WorkerState,
    },
};
use chrono::Utc;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info, warn};
use uuid::Uuid;

/// Simulate job execution and return result.
async fn simulate_job_execution(spec: &JobSpec) -> JobResult {
    let start_time = Utc::now();

    // Simulate computation time based on workload
    let output = match &spec.workload {
        WorkloadType::RangeSum { start, end } => {
            // Very fast simulation
            let sum: u128 = (*start..=*end).map(|x| x as u128).sum();
            tokio::time::sleep(Duration::from_millis(10)).await;
            JobOutput::RangeSum { sum }
        }
        WorkloadType::MatrixTile { size, tile } => {
            // Simulate matrix tile computation
            let rows = size / tile.tile_size;
            let cols = size / tile.tile_size;
            let tile_data = vec![1.0; rows * cols]; // Identity-like result
            tokio::time::sleep(Duration::from_millis(50)).await;
            JobOutput::MatrixTile {
                tile_data,
                rows,
                cols,
            }
        }
        WorkloadType::PrimeCount { start, end } => {
            let count = count_primes(*start, *end);
            tokio::time::sleep(Duration::from_millis(100)).await;
            JobOutput::PrimeCount { count }
        }
    };

    let end_time = Utc::now();

    JobResult {
        job_id: spec.id,
        worker_id: WorkerId(Uuid::new_v4()), // Will be overwritten by coordinator
        output,
        started_at: start_time,
        completed_at: end_time,
        cpu_cycles_used: spec.estimated_cycles,
        peak_memory_bytes: spec.estimated_memory_bytes,
    }
}

/// Count primes in range (simple implementation).
fn count_primes(start: u64, end: u64) -> u64 {
    let mut count = 0;
    for n in start..=end {
        if is_prime(n) {
            count += 1;
        }
    }
    count
}

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

/// Run a local simulation with multiple workers.
pub async fn run_simulation(
    num_workers: usize,
    num_jobs: usize,
) -> Result<SimulationResult, String> {
    info!(
        "Starting simulation: {} workers, {} jobs",
        num_workers, num_jobs
    );

    // Create coordinator with permissive limits for simulation
    let config = CoordinatorConfig {
        coordinator_id: Uuid::new_v4(),
        name: "Simulation Coordinator".into(),
        contact: None,
        default_limits: ResourceLimits {
            cpu_max_cores: Some(4),
            cpu_max_utilization_pct: Some(100),
            gpu_enabled: false,
            gpu_allowed_devices: vec![],
            gpu_max_memory_pct: None,
            ram_max_workload_bytes: Some(1_000_000_000),
            storage_workspace_path: None,
            storage_quota_bytes: None,
            schedule: None,
            power: bg_helper_protocol::resource::PowerPolicy {
                allow_on_battery: false,
                min_battery_pct: None,
                require_ac_for_gpu: false,
            },
        },
        supported_workloads: vec![
            "range_sum".into(),
            "matrix_tile".into(),
            "prime_count".into(),
        ],
        offer_timeout_secs: 30,
        heartbeat_interval_secs: 30,
        max_retries: 3,
    };
    let coordinator = Arc::new(CoordinatorState::new(config));

    // Register workers with coordinator (simulate enrollment)
    let mut worker_ids = Vec::new();
    for i in 0..num_workers {
        let response = coordinator
            .handle_enrollment(EnrollmentRequest {
                invitation_code: "SIMULATION".into(),
                worker_name: format!("sim-worker-{}", i),
                capabilities: vec![HardwareCapability::Cpu {
                    cores: 4,
                    threads: 8,
                    frequency_mhz: 3000,
                    architecture: "x86_64".into(),
                    features: vec!["avx2".into()],
                }],
                proposed_limits: ResourceLimits {
                    cpu_max_cores: Some(4),             // Allow more cores for simulation
                    cpu_max_utilization_pct: Some(100), // Allow full utilization for simulation
                    ram_max_workload_bytes: Some(1_000_000_000),
                    gpu_enabled: false,
                    gpu_allowed_devices: vec![],
                    gpu_max_memory_pct: None,
                    storage_workspace_path: None,
                    storage_quota_bytes: None,
                    schedule: None,
                    power: bg_helper_protocol::resource::PowerPolicy {
                        allow_on_battery: false,
                        min_battery_pct: None,
                        require_ac_for_gpu: false,
                    },
                },
                public_key: None,
            })
            .await
            .map_err(|e| e.to_string())?;

        if let Some(worker_id) = response.worker_id {
            worker_ids.push(worker_id);
        } else {
            return Err("Enrollment failed - no worker_id returned".into());
        }
    }

    // Submit jobs
    for i in 0..num_jobs {
        coordinator
            .submit_range_sum(
                (i * 10000) as u64,
                ((i + 1) * 10000) as u64,
                JobPriority::Normal,
                60,
            )
            .await
            .map_err(|e| e.to_string())?;
    }

    println!("Submitted {} jobs, checking queue", num_jobs);
    let (queued, assigned, comp) = coordinator.get_queue_status().await;
    println!(
        "Queue status: queued={}, assigned={}, completed={}",
        queued, assigned, comp
    );

    // Debug: check registered workers
    let workers = coordinator.get_workers().await;
    println!("Registered workers: {}", workers.len());
    for w in &workers {
        println!("  Worker {} - state: {:?}", w.worker_id, w.state);
    }

    // Run scheduling cycle to assign jobs
    let messages = coordinator
        .run_scheduling_cycle()
        .await
        .map_err(|e| e.to_string())?;
    println!(
        "Scheduling cycle completed with {} messages",
        messages.len()
    );

    let (queued, assigned, comp) = coordinator.get_queue_status().await;
    println!(
        "After scheduling: queued={}, assigned={}, completed={}",
        queued, assigned, comp
    );

    // Debug: check active jobs
    let active = coordinator.get_active_jobs().await;
    println!("Active jobs: {}", active.len());

    // Simulate workers accepting jobs (transition from Offered -> Approved -> Running)
    for worker_id in &worker_ids {
        coordinator
            .accept_all_offers_for_worker(*worker_id)
            .await
            .map_err(|e| e.to_string())?;
    }

    let (queued, assigned, comp) = coordinator.get_queue_status().await;
    println!(
        "After accepts: queued={}, assigned={}, completed={}",
        queued, assigned, comp
    );

    let active = coordinator.get_active_jobs().await;
    println!("Active jobs after accept: {}", active.len());

    // Simulate workers completing jobs
    let (completed, failed) = coordinator
        .complete_jobs_for_workers(&worker_ids)
        .await
        .map_err(|e| e.to_string())?;

    println!("Completed {} jobs, {} failed", completed, failed);

    Ok(SimulationResult {
        total_jobs: num_jobs,
        completed,
        failed,
        workers: num_workers,
    })
}

/// Result of a simulation run.
#[derive(Debug, Clone)]
pub struct SimulationResult {
    pub total_jobs: usize,
    pub completed: usize,
    pub failed: usize,
    pub workers: usize,
}

impl SimulationResult {
    pub fn success_rate(&self) -> f64 {
        if self.total_jobs == 0 {
            0.0
        } else {
            self.completed as f64 / self.total_jobs as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bg_helper_protocol::job::{JobPriority, JobSpec, WorkloadType};

    #[tokio::test]
    async fn test_simulate_range_sum() {
        let spec = JobSpec::new(
            WorkloadType::RangeSum { start: 0, end: 100 },
            JobPriority::Normal,
            60,
        );
        let result = simulate_job_execution(&spec).await;

        if let JobOutput::RangeSum { sum } = result.output {
            // Sum of 0..=100 = 5050
            assert_eq!(sum, 5050);
        } else {
            panic!("Wrong output type");
        }
    }

    #[tokio::test]
    async fn test_simulate_prime_count() {
        let spec = JobSpec::new(
            WorkloadType::PrimeCount { start: 0, end: 100 },
            JobPriority::Normal,
            60,
        );
        let result = simulate_job_execution(&spec).await;

        if let JobOutput::PrimeCount { count } = result.output {
            // Primes up to 100: 25 primes
            assert_eq!(count, 25);
        } else {
            panic!("Wrong output type");
        }
    }

    #[tokio::test]
    async fn test_simulation_basic() {
        let result = run_simulation(2, 5).await.unwrap();
        assert_eq!(result.total_jobs, 5);
        assert_eq!(result.workers, 2);
        // All jobs should complete
        assert_eq!(result.completed, 5);
    }
}
