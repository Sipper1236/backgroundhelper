//! Job placement strategies.

use bg_helper_protocol::job::JobSpec;
use bg_helper_protocol::resource::{ResourceLimits, ResourceUsage};
use crate::{WorkerSnapshot, WorkerScore, ScheduleDecision};
use uuid::Uuid;

/// Placement strategy trait.
pub trait PlacementStrategy: Send + Sync {
    fn place(&self, job: &JobSpec, workers: &[WorkerSnapshot], scores: &[WorkerScore]) -> Option<ScheduleDecision>;
}

/// Best-fit placement: choose worker with most available resources.
pub struct BestFitPlacement;

impl PlacementStrategy for BestFitPlacement {
    fn place(&self, job: &JobSpec, workers: &[WorkerSnapshot], scores: &[WorkerScore]) -> Option<ScheduleDecision> {
        let suitable: Vec<_> = scores.iter().filter(|s| s.is_suitable()).collect();
        if suitable.is_empty() {
            return None;
        }

        // Find worker with most available CPU headroom
        let best = suitable.into_iter().max_by(|a, b| {
            let wa = workers.iter().find(|w| w.worker_id == a.worker_id).unwrap();
            let wb = workers.iter().find(|w| w.worker_id == b.worker_id).unwrap();
            
            let headroom_a = cpu_headroom(&wa.current_usage, &wa.limits, job);
            let headroom_b = cpu_headroom(&wb.current_usage, &wb.limits, job);
            
            headroom_a.partial_cmp(&headroom_b).unwrap()
        })?;

        Some(ScheduleDecision::Assign { worker_id: best.worker_id })
    }
}

/// Least-loaded placement: choose worker with lowest current utilization.
pub struct LeastLoadedPlacement;

impl PlacementStrategy for LeastLoadedPlacement {
    fn place(&self, job: &JobSpec, workers: &[WorkerSnapshot], scores: &[WorkerScore]) -> Option<ScheduleDecision> {
        let suitable: Vec<_> = scores.iter().filter(|s| s.is_suitable()).collect();
        if suitable.is_empty() {
            return None;
        }

        let best = suitable.into_iter().min_by(|a, b| {
            let wa = workers.iter().find(|w| w.worker_id == a.worker_id).unwrap();
            let wb = workers.iter().find(|w| w.worker_id == b.worker_id).unwrap();
            
            wa.current_usage.cpu_utilization_pct
                .partial_cmp(&wb.current_usage.cpu_utilization_pct)
                .unwrap()
        })?;

        Some(ScheduleDecision::Assign { worker_id: best.worker_id })
    }
}

/// Affinity-aware placement: prefer workers that have run similar jobs before.
pub struct AffinityPlacement {
    /// Job type -> worker_id affinity scores
    affinity: std::collections::HashMap<String, std::collections::HashMap<Uuid, f64>>,
}

impl Default for AffinityPlacement {
    fn default() -> Self {
        Self { affinity: std::collections::HashMap::new() }
    }
}

impl PlacementStrategy for AffinityPlacement {
    fn place(&self, job: &JobSpec, workers: &[WorkerSnapshot], scores: &[WorkerScore]) -> Option<ScheduleDecision> {
        let suitable: Vec<_> = scores.iter().filter(|s| s.is_suitable()).collect();
        if suitable.is_empty() {
            return None;
        }

        let job_type = job.workload.name();
        let affinities = self.affinity.get(job_type);

        let best = suitable.into_iter().max_by(|a, b| {
            let affinity_a = affinities.and_then(|m| m.get(&a.worker_id)).copied().unwrap_or(0.0);
            let affinity_b = affinities.and_then(|m| m.get(&b.worker_id)).copied().unwrap_or(0.0);
            
            // Combine affinity with score
            let combined_a = a.score + affinity_a * 100.0;
            let combined_b = b.score + affinity_b * 100.0;
            
            combined_a.partial_cmp(&combined_b).unwrap()
        })?;

        Some(ScheduleDecision::Assign { worker_id: best.worker_id })
    }
}

impl AffinityPlacement {
    pub fn record_completion(&mut self, job_type: &str, worker_id: Uuid) {
        let entry = self.affinity.entry(job_type.to_string()).or_default();
        *entry.entry(worker_id).or_insert(0.0) += 1.0;
    }
}

/// Composite placement using multiple strategies with fallback.
pub struct CompositePlacement {
    strategies: Vec<Box<dyn PlacementStrategy>>,
}

impl CompositePlacement {
    pub fn new() -> Self {
        Self { strategies: Vec::new() }
    }

    pub fn add_strategy<S: PlacementStrategy + 'static>(mut self, strategy: S) -> Self {
        self.strategies.push(Box::new(strategy));
        self
    }
}

impl Default for CompositePlacement {
    fn default() -> Self {
        Self::new()
            .add_strategy(AffinityPlacement::default())
            .add_strategy(BestFitPlacement)
            .add_strategy(LeastLoadedPlacement)
    }
}

impl PlacementStrategy for CompositePlacement {
    fn place(&self, job: &JobSpec, workers: &[WorkerSnapshot], scores: &[WorkerScore]) -> Option<ScheduleDecision> {
        for strategy in &self.strategies {
            if let Some(decision) = strategy.place(job, workers, scores) {
                return Some(decision);
            }
        }
        None
    }
}

fn cpu_headroom(usage: &ResourceUsage, limits: &ResourceLimits, _job: &JobSpec) -> f32 {
    let mut headroom = f32::INFINITY;
    let max_cores = limits.cpu_max_cores.unwrap_or(64) as f32;
    
    if let Some(max_c) = limits.cpu_max_cores {
        let available = max_c as f32 - usage.cpu_cores_used;
        headroom = headroom.min(available);
    }
    
    if let Some(max_pct) = limits.cpu_max_utilization_pct {
        let available_pct = (max_pct as f32 - usage.cpu_utilization_pct).max(0.0);
        let cores_from_pct = available_pct / 100.0 * max_cores;
        headroom = headroom.min(cores_from_pct);
    }
    
    headroom
}

#[cfg(test)]
mod tests {
    use super::*;
    use bg_helper_protocol::job::{JobSpec, WorkloadType};
    use bg_helper_protocol::resource::{ResourceLimits, ResourceUsage, WorkerState, PowerState};
    use crate::{WorkerSnapshot, WorkerScore};
    use chrono::Utc;
    use uuid::Uuid;

    fn test_worker(id: Uuid, util: f32, cores_used: f32) -> WorkerSnapshot {
        WorkerSnapshot {
            worker_id: id,
            status: WorkerState::Idle,
            capabilities: vec![],
            limits: ResourceLimits {
                cpu_max_cores: Some(4),
                cpu_max_utilization_pct: Some(50),
                gpu_enabled: false,
                gpu_allowed_devices: vec![],
                gpu_max_memory_pct: None,
                ram_max_workload_bytes: Some(2_000_000_000),
                storage_workspace_path: None,
                storage_quota_bytes: None,
                schedule: None,
                power: Default::default(),
            },
            current_offer: None,
            current_usage: ResourceUsage {
                worker_id: id,
                timestamp: Utc::now(),
                cpu_utilization_pct: util,
                cpu_cores_used: cores_used,
                gpu_utilization_pct: None,
                gpu_memory_used_bytes: None,
                ram_used_bytes: 1_000_000_000,
                ram_available_bytes: 15_000_000_000,
                storage_used_bytes: 0,
                storage_available_bytes: 0,
                power_state: PowerState::Ac,
            },
            current_job: None,
            last_seen: Utc::now(),
        }
    }

    fn test_scores(workers: &[WorkerSnapshot]) -> Vec<WorkerScore> {
        workers.iter().map(|w| WorkerScore {
            worker_id: w.worker_id,
            score: 100.0,
            cpu_available: true,
            ram_available: true,
            gpu_compatible: true,
            schedule_ok: true,
            power_ok: true,
        }).collect()
    }

    #[test]
    fn test_best_fit_prefers_more_headroom() {
        let placement = BestFitPlacement;
        let worker1 = Uuid::new_v4();
        let worker2 = Uuid::new_v4();
        
        // Worker1 has more headroom (lower util)
        let workers = vec![
            test_worker(worker1, 10.0, 0.5), // 10% util
            test_worker(worker2, 40.0, 2.0), // 40% util
        ];
        
        let job = JobSpec::new(WorkloadType::RangeSum { start: 0, end: 1000 }, 100, 300);
        let scores = test_scores(&workers);
        
        let decision = placement.place(&job, &workers, &scores);
        assert!(matches!(decision, Some(ScheduleDecision::Assign { worker_id }) if worker_id == worker1));
    }

    #[test]
    fn test_least_loaded_prefers_lower_util() {
        let placement = LeastLoadedPlacement;
        let worker1 = Uuid::new_v4();
        let worker2 = Uuid::new_v4();
        
        let workers = vec![
            test_worker(worker1, 10.0, 0.5),
            test_worker(worker2, 40.0, 2.0),
        ];
        
        let job = JobSpec::new(WorkloadType::RangeSum { start: 0, end: 1000 }, 100, 300);
        let scores = test_scores(&workers);
        
        let decision = placement.place(&job, &workers, &scores);
        assert!(matches!(decision, Some(ScheduleDecision::Assign { worker_id }) if worker_id == worker1));
    }

    #[test]
    fn test_affinity_prefers_known_worker() {
        let mut placement = AffinityPlacement::default();
        let worker1 = Uuid::new_v4();
        let worker2 = Uuid::new_v4();
        
        placement.record_completion("range_sum", worker1);
        placement.record_completion("range_sum", worker1);
        // worker2 has no affinity
        
        let workers = vec![
            test_worker(worker1, 30.0, 1.5),
            test_worker(worker2, 10.0, 0.5), // Less loaded but no affinity
        ];
        
        let job = JobSpec::new(WorkloadType::RangeSum { start: 0, end: 1000 }, 100, 300);
        let scores = test_scores(&workers);
        
        let decision = placement.place(&job, &workers, &scores);
        // Should prefer worker1 due to affinity despite higher load
        assert!(matches!(decision, Some(ScheduleDecision::Assign { worker_id }) if worker_id == worker1));
    }

    #[test]
    fn test_composite_fallback() {
        let placement = CompositePlacement::default();
        let worker = Uuid::new_v4();
        
        let workers = vec![test_worker(worker, 10.0, 0.5)];
        let job = JobSpec::new(WorkloadType::RangeSum { start: 0, end: 1000 }, 100, 300);
        let scores = test_scores(&workers);
        
        let decision = placement.place(&job, &workers, &scores);
        assert!(matches!(decision, Some(ScheduleDecision::Assign { worker_id }) if worker_id == worker));
    }
}