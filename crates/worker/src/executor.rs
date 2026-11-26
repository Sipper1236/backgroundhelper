//! Job executor for running workloads.

use bg_helper_protocol::job::{JobSpec, JobResult, JobOutput, WorkloadType, JobId, WorkerId};
use bg_helper_protocol::resource::{ResourceLimits, ResourceUsage};
use uuid::Uuid;
use chrono::Utc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{info, debug, error};

/// Job executor that runs workloads locally.
pub struct JobExecutor {
    worker_id: WorkerId,
    limits: ResourceLimits,
}

impl JobExecutor {
    pub fn new(worker_id: Uuid, limits: ResourceLimits) -> Self {
        Self { worker_id: WorkerId(worker_id), limits }
    }

    /// Get the worker ID.
    pub fn worker_id(&self) -> WorkerId {
        self.worker_id
    }

    /// Get the resource limits.
    pub fn limits(&self) -> &ResourceLimits {
        &self.limits
    }

    /// Execute a job and return the result.
    pub async fn execute(&self, job_spec: JobSpec) -> Result<JobResult, ExecutionError> {
        info!("Executing job {}", job_spec.id);
        let start_time = Utc::now();
        
        // Check if job is allowed by policy
        self.check_policy(&job_spec)?;
        
        // Execute based on workload type
        let output = match &job_spec.workload {
            WorkloadType::RangeSum { start, end } => {
                self.execute_range_sum(*start, *end).await?
            }
            WorkloadType::MatrixTile { size, tile } => {
                self.execute_matrix_tile(*size, tile).await?
            }
            WorkloadType::PrimeCount { start, end } => {
                self.execute_prime_count(*start, *end).await?
            }
        };
        
        let end_time = Utc::now();
        
        Ok(JobResult {
            job_id: job_spec.id,
            worker_id: self.worker_id,
            output,
            started_at: start_time,
            completed_at: end_time,
            cpu_cycles_used: job_spec.estimated_cycles,
            peak_memory_bytes: job_spec.estimated_memory_bytes,
        })
    }

    fn check_policy(&self, job: &JobSpec) -> Result<(), ExecutionError> {
        // Check CPU cores limit
        if let Some(max_cores) = self.limits.cpu_max_cores {
            let needed = estimate_job_cores(job);
            if needed > max_cores as f32 {
                return Err(ExecutionError::PolicyViolation(
                    format!("Job requires {:.1} cores, limit is {}", needed, max_cores)
                ));
            }
        }
        
        // Check RAM limit
        if let Some(max_ram) = self.limits.ram_max_workload_bytes {
            if job.estimated_memory_bytes > max_ram {
                return Err(ExecutionError::PolicyViolation(
                    format!("Job requires {} bytes RAM, limit is {}", job.estimated_memory_bytes, max_ram)
                ));
            }
        }
        
        Ok(())
    }

    async fn execute_range_sum(&self, start: u64, end: u64) -> Result<JobOutput, ExecutionError> {
        debug!("Computing range sum {}..={}", start, end);
        
        // Simulate work with small delay
        sleep(Duration::from_millis(10)).await;
        
        // Compute sum (use u128 to avoid overflow)
        let sum: u128 = (start..=end).map(|x| x as u128).sum();
        
        Ok(JobOutput::RangeSum { sum })
    }

    async fn execute_matrix_tile(&self, size: usize, tile: &bg_helper_protocol::job::TileSpec) -> Result<JobOutput, ExecutionError> {
        debug!("Computing matrix tile {}x{} at ({},{})", size, size, tile.row, tile.col);
        
        // Simulate work
        sleep(Duration::from_millis(50)).await;
        
        // Generate tile data (simplified - identity matrix tile)
        let tile_rows = size / tile.tile_size;
        let tile_cols = size / tile.tile_size;
        let tile_data = vec![1.0; tile_rows * tile_cols];
        
        Ok(JobOutput::MatrixTile {
            tile_data,
            rows: tile_rows,
            cols: tile_cols,
        })
    }

    async fn execute_prime_count(&self, start: u64, end: u64) -> Result<JobOutput, ExecutionError> {
        debug!("Counting primes in range {}..={}", start, end);
        
        // Simulate work
        sleep(Duration::from_millis(100)).await;
        
        let count = count_primes_in_range(start, end);
        
        Ok(JobOutput::PrimeCount { count })
    }
}

fn estimate_job_cores(job: &JobSpec) -> f32 {
    match &job.workload {
        WorkloadType::RangeSum { .. } => 1.0,
        WorkloadType::MatrixTile { size, tile } => (size / tile.tile_size) as f32,
        WorkloadType::PrimeCount { .. } => 1.0,
    }
}

fn count_primes_in_range(start: u64, end: u64) -> u64 {
    let mut count = 0;
    for n in start..=end {
        if is_prime(n) {
            count += 1;
        }
    }
    count
}

fn is_prime(n: u64) -> bool {
    if n < 2 { return false; }
    if n == 2 { return true; }
    if n % 2 == 0 { return false; }
    let mut i = 3;
    while i * i <= n {
        if n % i == 0 { return false; }
        i += 2;
    }
    true
}

/// Execution errors.
#[derive(Debug, thiserror::Error)]
pub enum ExecutionError {
    #[error("Policy violation: {0}")]
    PolicyViolation(String),
    #[error("Computation error: {0}")]
    ComputationError(String),
    #[error("Resource limit exceeded: {0}")]
    ResourceLimit(String),
    #[error("Job cancelled")]
    Cancelled,
}

#[cfg(test)]
mod tests {
    use super::*;
    use bg_helper_protocol::job::{JobSpec, WorkloadType, JobPriority};
    use bg_helper_protocol::resource::ResourceLimits;
    use uuid::Uuid;

    #[tokio::test]
    async fn test_execute_range_sum() {
        let executor = JobExecutor::new(Uuid::new_v4(), ResourceLimits::default());
        let spec = JobSpec::new(WorkloadType::RangeSum { start: 0, end: 100 }, JobPriority::Normal, 60);
        
        let result = executor.execute(spec).await.unwrap();
        
        if let JobOutput::RangeSum { sum } = result.output {
            assert_eq!(sum, 5050); // Sum of 0..=100
        } else {
            panic!("Wrong output type");
        }
    }

    #[tokio::test]
    async fn test_execute_prime_count() {
        let executor = JobExecutor::new(Uuid::new_v4(), ResourceLimits::default());
        let spec = JobSpec::new(WorkloadType::PrimeCount { start: 0, end: 100 }, JobPriority::Normal, 60);
        
        let result = executor.execute(spec).await.unwrap();
        
        if let JobOutput::PrimeCount { count } = result.output {
            assert_eq!(count, 25); // Primes up to 100
        } else {
            panic!("Wrong output type");
        }
    }

    #[tokio::test]
    async fn test_policy_violation_cores() {
        let mut limits = ResourceLimits::default();
        limits.cpu_max_cores = Some(1);
        
        let executor = JobExecutor::new(Uuid::new_v4(), limits);
        let spec = JobSpec::new(WorkloadType::MatrixTile { 
            size: 1000, 
            tile: bg_helper_protocol::job::TileSpec { row: 0, col: 0, tile_size: 100 } 
        }, JobPriority::Normal, 60);
        
        let result = executor.execute(spec).await;
        assert!(matches!(result, Err(ExecutionError::PolicyViolation(_))));
    }
}