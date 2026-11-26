//! Placement strategies for job scheduling.

use bg_helper_protocol::job::JobSpec;
use crate::WorkerSnapshot;

/// Placement strategy for scheduling decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlacementStrategy {
    /// Prefer workers with most available resources (best-fit).
    #[default]
    BestFit,
    /// Prefer workers with least current load (least-loaded).
    LeastLoaded,
    /// Prefer workers with specific hardware match.
    HardwareMatch,
    /// Spread jobs across workers (round-robin like).
    Spread,
}

impl PlacementStrategy {
    /// Score a worker for a job (higher = better).
    pub fn score_worker(&self, job: &JobSpec, worker: &WorkerSnapshot) -> f64 {
        match self {
            PlacementStrategy::BestFit => self.score_best_fit(job, worker),
            PlacementStrategy::LeastLoaded => self.score_least_loaded(job, worker),
            PlacementStrategy::HardwareMatch => self.score_hardware_match(job, worker),
            PlacementStrategy::Spread => self.score_spread(job, worker),
        }
    }

    fn score_best_fit(&self, job: &JobSpec, worker: &WorkerSnapshot) -> f64 {
        // Score based on how well the job fits the available resources
        let mut score: f64 = 0.0;
        
        // CPU fit
        if let Some(max_cores) = worker.limits.cpu_max_cores {
            let available_cores = max_cores as f64 - worker.usage.cpu_cores_used as f64;
            let job_cores = estimate_job_cores(job) as f64;
            if available_cores >= job_cores {
                // Prefer tighter fit (but not too tight)
                let utilization = job_cores / available_cores;
                if utilization > 0.3 && utilization < 0.9 {
                    score += 100.0 * (1.0 - (utilization - 0.5).abs());
                } else {
                    score += 50.0;
                }
            }
        }
        
        // RAM fit
        if let Some(max_ram) = worker.limits.ram_max_workload_bytes {
            let available_ram = max_ram.saturating_sub(worker.usage.ram_used_bytes) as f64;
            let job_ram = job.estimated_memory_bytes as f64;
            if available_ram >= job_ram {
                let utilization = job_ram / available_ram;
                if utilization > 0.3 && utilization < 0.9 {
                    score += 50.0 * (1.0 - (utilization - 0.5).abs());
                } else {
                    score += 25.0;
                }
            }
        }
        
        score
    }

    fn score_least_loaded(&self, job: &JobSpec, worker: &WorkerSnapshot) -> f64 {
        // Prefer workers with lowest current utilization
        let cpu_load = worker.usage.cpu_utilization_pct as f64 / 100.0;
        let ram_load = worker.usage.ram_used_bytes as f64 / 
            (worker.usage.ram_used_bytes + worker.usage.ram_available_bytes).max(1) as f64;
        
        let avg_load = (cpu_load + ram_load) / 2.0;
        100.0 * (1.0 - avg_load)
    }

    fn score_hardware_match(&self, job: &JobSpec, worker: &WorkerSnapshot) -> f64 {
        // Check if worker has specific hardware needed for job
        let mut score = 0.0;
        
        match &job.workload {
            bg_helper_protocol::job::WorkloadType::MatrixTile { .. } => {
                // Matrix work benefits from AVX2
                if worker.capabilities.iter().any(|c| {
                    matches!(c, bg_helper_protocol::resource::HardwareCapability::Cpu { features, .. } 
                        if features.iter().any(|f| f == "avx2"))
                }) {
                    score += 200.0;
                }
            }
            bg_helper_protocol::job::WorkloadType::PrimeCount { .. } => {
                // Prime counting benefits from many cores
                if let Some(bg_helper_protocol::resource::HardwareCapability::Cpu { cores, .. }) = 
                    worker.capabilities.iter().find(|c| matches!(c, bg_helper_protocol::resource::HardwareCapability::Cpu { .. })) {
                    score += (*cores as f64).min(64.0) * 5.0;
                }
            }
            _ => {}
        }
        
        score
    }

    fn score_spread(&self, _job: &JobSpec, _worker: &WorkerSnapshot) -> f64 {
        // Spread strategy would need cluster-wide state
        // For now, return neutral score
        0.0
    }
}

fn estimate_job_cores(job: &JobSpec) -> f32 {
    match &job.workload {
        bg_helper_protocol::job::WorkloadType::RangeSum { .. } => 1.0,
        bg_helper_protocol::job::WorkloadType::MatrixTile { size, tile } => (*size / tile.tile_size) as f32,
        bg_helper_protocol::job::WorkloadType::PrimeCount { .. } => 1.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bg_helper_protocol::job::{JobSpec, WorkloadType, JobPriority};
    use bg_helper_protocol::resource::{ResourceLimits, ResourceUsage, WorkerState, HardwareCapability, PowerState};
    use crate::WorkerSnapshot;
    use chrono::Utc;
    use uuid::Uuid;

    fn test_worker(id: Uuid, features: Vec<String>) -> WorkerSnapshot {
        WorkerSnapshot {
            worker_id: id,
            status: WorkerState::Idle,
            capabilities: vec![HardwareCapability::Cpu {
                cores: 4,
                threads: 8,
                frequency_mhz: 3000,
                architecture: "x86_64".into(),
                features,
            }],
            limits: ResourceLimits {
                cpu_max_cores: Some(4),
                cpu_max_utilization_pct: Some(50),
                ram_max_workload_bytes: Some(8_000_000_000),
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
                ram_used_bytes: 2_000_000_000,
                ram_available_bytes: 14_000_000_000,
                storage_used_bytes: 10_000_000_000,
                storage_available_bytes: 100_000_000_000,
                power_state: PowerState::Ac,
                thermal_state: bg_helper_protocol::resource::ThermalState::Normal,
            },
        }
    }

    #[test]
    fn test_best_fit_strategy() {
        let strategy = PlacementStrategy::BestFit;
        let worker_id = Uuid::new_v4();
        let worker = test_worker(worker_id, vec!["avx2".into()]);
        
        let job = JobSpec::new(WorkloadType::RangeSum { start: 0, end: 1_000_000 }, JobPriority::Normal, 300);
        let score = strategy.score_worker(&job, &worker);
        
        assert!(score > 0.0);
    }

    #[test]
    fn test_hardware_match_avx2() {
        let strategy = PlacementStrategy::HardwareMatch;
        let worker_id = Uuid::new_v4();
        
        let worker_with_avx2 = test_worker(worker_id, vec!["avx2".into()]);
        let worker_without_avx2 = test_worker(Uuid::new_v4(), vec![]);
        
        let job = JobSpec::new(WorkloadType::MatrixTile { 
            size: 1000, 
            tile: bg_helper_protocol::job::TileSpec { row: 0, col: 0, tile_size: 100 } 
        }, JobPriority::Normal, 300);
        
        let score_with = strategy.score_worker(&job, &worker_with_avx2);
        let score_without = strategy.score_worker(&job, &worker_without_avx2);
        
        assert!(score_with > score_without);
    }

    #[test]
    fn test_least_loaded() {
        let strategy = PlacementStrategy::LeastLoaded;
        let worker_id = Uuid::new_v4();
        
        let mut worker_low = test_worker(worker_id, vec![]);
        worker_low.usage.cpu_utilization_pct = 10.0;
        
        let mut worker_high = test_worker(Uuid::new_v4(), vec![]);
        worker_high.usage.cpu_utilization_pct = 80.0;
        
        let job = JobSpec::new(WorkloadType::RangeSum { start: 0, end: 1_000_000 }, JobPriority::Normal, 300);
        
        let score_low = strategy.score_worker(&job, &worker_low);
        let score_high = strategy.score_worker(&job, &worker_high);
        
        assert!(score_low > score_high);
    }
}