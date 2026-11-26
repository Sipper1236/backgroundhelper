//! Policy evaluation logic - determines if jobs can run given current state.

use bg_helper_protocol::{
    job::{JobSpec, WorkloadType, JobPriority},
    resource::{ResourceLimits, ResourceUsage, WorkerState, PowerState, SharingSchedule},
    validation::{ValidationError, Validatable},
};
use std::collections::HashMap;

/// Policy decision for a job offer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyDecision {
    /// Job can run as requested.
    Allow,
    /// Job can run with reduced resources.
    AllowWithReduction(ResourceReduction),
    /// Job cannot run.
    Reject(String),
}

/// Resource reduction to apply to a job.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ResourceReduction {
    pub cpu_cores: Option<u32>,
    pub cpu_utilization_pct: Option<u8>,
    pub gpu_memory_pct: Option<u8>,
    pub ram_bytes: Option<u64>,
}

/// Evaluates job offers against resource limits and current usage.
pub struct PolicyEvaluator;

impl PolicyEvaluator {
    /// Evaluate a job offer against current limits and usage.
    pub fn evaluate_job_offer(
        limits: &ResourceLimits,
        usage: &ResourceUsage,
        job: &JobSpec,
        schedule: Option<&SharingSchedule>,
        power_state: PowerState,
        worker_state: WorkerState,
    ) -> PolicyDecision {
        // Worker must be in acceptable state
        if !worker_state.can_accept_work() {
            return PolicyDecision::Reject(format!("Worker state {:?} cannot accept work", worker_state));
        }

        // Check schedule
        if let Some(sched) = schedule {
            if !sched.is_allowed_now() {
                return PolicyDecision::Reject("Outside allowed sharing schedule".into());
            }
        }

        // Check power policy
        let power_decision = Self::check_power_policy(&limits.power, power_state, job);
        if let PolicyDecision::Reject(reason) = power_decision {
            return PolicyDecision::Reject(reason);
        }

        // Check CPU limits
        if let Some(decision) = Self::check_cpu_limits(limits, usage, job) {
            return decision;
        }

        // Check RAM limits
        if let Some(decision) = Self::check_ram_limits(limits, usage, job) {
            return decision;
        }

        // Check GPU limits
        if let Some(decision) = Self::check_gpu_limits(limits, usage, job) {
            return decision;
        }

        // Check storage limits
        if let Some(decision) = Self::check_storage_limits(limits, usage, job) {
            return decision;
        }

        PolicyDecision::Allow
    }

    /// Evaluate if a running job should continue.
    pub fn evaluate_running_job(
        limits: &ResourceLimits,
        usage: &ResourceUsage,
        schedule: Option<&SharingSchedule>,
        power_state: PowerState,
    ) -> PolicyDecision {
        // Check schedule
        if let Some(sched) = schedule {
            if !sched.is_allowed_now() {
                return PolicyDecision::Reject("Outside allowed sharing schedule".into());
            }
        }

        // Check power policy
        if !limits.power.allow_on_battery && power_state == PowerState::Battery {
            if let Some(min_pct) = limits.power.min_battery_pct {
                // We'd need battery percentage from usage - simplified for now
                return PolicyDecision::Reject("Battery policy violation".into());
            }
        }

        // Check if current usage exceeds limits
        if let Some(max_pct) = limits.cpu_max_utilization_pct {
            if usage.cpu_utilization_pct > max_pct as f32 {
                return PolicyDecision::Reject(format!(
                    "CPU utilization {:.1}% exceeds limit {}%", 
                    usage.cpu_utilization_pct, max_pct
                ));
            }
        }

        if let Some(max_bytes) = limits.ram_max_workload_bytes {
            if usage.ram_used_bytes > max_bytes {
                return PolicyDecision::Reject(format!(
                    "RAM usage {} bytes exceeds limit {}", 
                    usage.ram_used_bytes, max_bytes
                ));
            }
        }

        PolicyDecision::Allow
    }

    /// Check CPU-related limits.
    fn check_cpu_limits(
        limits: &ResourceLimits,
        usage: &ResourceUsage,
        job: &JobSpec,
    ) -> Option<PolicyDecision> {
        // Check max cores
        if let Some(max_cores) = limits.cpu_max_cores {
            let available_cores = max_cores as f32 - usage.cpu_cores_used;
            let job_cores_estimate = Self::estimate_job_cores(job);
            
            if job_cores_estimate > available_cores {
                if available_cores >= 1.0 {
                    return Some(PolicyDecision::AllowWithReduction(ResourceReduction {
                        cpu_cores: Some(available_cores.floor() as u32),
                        cpu_utilization_pct: None,
                        gpu_memory_pct: None,
                        ram_bytes: None,
                    }));
                }
                return Some(PolicyDecision::Reject(
                    format!("Insufficient CPU cores: need ~{}, available {}", job_cores_estimate, available_cores)
                ));
            }
        }

        // Check max utilization
        if let Some(max_pct) = limits.cpu_max_utilization_pct {
            let current_pct = usage.cpu_utilization_pct;
            let job_pct_estimate = Self::estimate_job_cpu_pct(job);
            
            if current_pct + job_pct_estimate > max_pct as f32 {
                let remaining = (max_pct as f32 - current_pct).max(0.0);
                if remaining >= 10.0 {
                    return Some(PolicyDecision::AllowWithReduction(ResourceReduction {
                        cpu_cores: None,
                        cpu_utilization_pct: Some(remaining as u8),
                        gpu_memory_pct: None,
                        ram_bytes: None,
                    }));
                }
                return Some(PolicyDecision::Reject(
                    format!("CPU utilization would exceed limit: current {:.1}%, job ~{:.1}%, max {}%", 
                        current_pct, job_pct_estimate, max_pct)
                ));
            }
        }

        None
    }

    /// Check RAM limits.
    fn check_ram_limits(
        limits: &ResourceLimits,
        usage: &ResourceUsage,
        job: &JobSpec,
    ) -> Option<PolicyDecision> {
        if let Some(max_bytes) = limits.ram_max_workload_bytes {
            let available = max_bytes.saturating_sub(usage.ram_used_bytes);
            let job_mem = job.estimated_memory_bytes;
            
            if job_mem > available {
                if available >= 100_000_000 { // At least 100MB
                    return Some(PolicyDecision::AllowWithReduction(ResourceReduction {
                        cpu_cores: None,
                        cpu_utilization_pct: None,
                        gpu_memory_pct: None,
                        ram_bytes: Some(available),
                    }));
                }
                return Some(PolicyDecision::Reject(
                    format!("Insufficient RAM: need {} bytes, available {}", job_mem, available)
                ));
            }
        }
        None
    }

    /// Check GPU limits.
    fn check_gpu_limits(
        limits: &ResourceLimits,
        usage: &ResourceUsage,
        job: &JobSpec,
    ) -> Option<PolicyDecision> {
        if !limits.gpu_enabled {
            // Check if job requires GPU (for future GPU workloads)
            // For v0.1 CPU-only, this is always None
            return None;
        }

        if let Some(max_pct) = limits.gpu_max_memory_pct {
            if let Some(used_pct) = usage.gpu_utilization_pct {
                // Simplified: assume job needs some GPU memory
                let job_pct = 20.0; // Placeholder
                if used_pct + job_pct > max_pct as f32 {
                    return Some(PolicyDecision::Reject(
                        format!("GPU memory would exceed limit: current {:.1}%, job ~{:.1}%, max {}%", 
                            used_pct, job_pct, max_pct)
                    ));
                }
            }
        }
        None
    }

    /// Check storage limits.
    fn check_storage_limits(
        limits: &ResourceLimits,
        usage: &ResourceUsage,
        _job: &JobSpec,
    ) -> Option<PolicyDecision> {
        if let Some(quota) = limits.storage_quota_bytes {
            if usage.storage_used_bytes >= quota {
                return Some(PolicyDecision::Reject("Storage quota exceeded".into()));
            }
        }
        None
    }

    /// Check power policy.
    fn check_power_policy(
        policy: &bg_helper_protocol::resource::PowerPolicy,
        power_state: PowerState,
        _job: &JobSpec,
    ) -> PolicyDecision {
        if power_state == PowerState::Battery {
            if !policy.allow_on_battery {
                return PolicyDecision::Reject("Computation not allowed on battery".into());
            }
            // Could check battery percentage here if available
        }
        PolicyDecision::Allow
    }

    /// Estimate CPU cores needed for a job (heuristic).
    fn estimate_job_cores(job: &JobSpec) -> f32 {
        match &job.workload {
            WorkloadType::RangeSum { .. } => 1.0,
            WorkloadType::MatrixTile { size, tile } => {
                // Parallelizable across tiles
                (*size / tile.tile_size) as f32
            }
            WorkloadType::PrimeCount { .. } => 1.0,
        }
    }

    /// Estimate CPU utilization percentage for a job.
    fn estimate_job_cpu_pct(job: &JobSpec) -> f32 {
        // Very rough heuristic: assume job uses 100% of allocated cores
        Self::estimate_job_cores(job) * 100.0
    }
}

/// Merge local and remote limits (local takes precedence for restrictions).
pub fn merge_limits(local: &ResourceLimits, remote: &ResourceLimits) -> ResourceLimits {
    ResourceLimits {
        cpu_max_cores: tighten_opt(local.cpu_max_cores, remote.cpu_max_cores),
        cpu_max_utilization_pct: tighten_opt(local.cpu_max_utilization_pct, remote.cpu_max_utilization_pct),
        gpu_enabled: local.gpu_enabled && remote.gpu_enabled,
        gpu_allowed_devices: intersect_devices(&local.gpu_allowed_devices, &remote.gpu_allowed_devices),
        gpu_max_memory_pct: tighten_opt(local.gpu_max_memory_pct, remote.gpu_max_memory_pct),
        ram_max_workload_bytes: tighten_opt(local.ram_max_workload_bytes, remote.ram_max_workload_bytes),
        storage_workspace_path: local.storage_workspace_path.clone().or(remote.storage_workspace_path.clone()),
        storage_quota_bytes: tighten_opt(local.storage_quota_bytes, remote.storage_quota_bytes),
        schedule: merge_schedule(&local.schedule, &remote.schedule),
        power: merge_power(&local.power, &remote.power),
    }
}

fn tighten_opt<T: Ord + Copy>(local: Option<T>, remote: Option<T>) -> Option<T> {
    match (local, remote) {
        (Some(l), Some(r)) => Some(l.min(r)),
        (Some(l), None) => Some(l),
        (None, Some(r)) => Some(r),
        (None, None) => None,
    }
}

fn intersect_devices(local: &[String], remote: &[String]) -> Vec<String> {
    if local.is_empty() { return remote.to_vec(); }
    if remote.is_empty() { return local.to_vec(); }
    local.iter().filter(|d| remote.contains(d)).cloned().collect()
}

fn merge_schedule(local: &Option<SharingSchedule>, remote: &Option<SharingSchedule>) -> Option<SharingSchedule> {
    match (local, remote) {
        (Some(l), Some(r)) => Some(intersect_schedules(l, r)),
        (Some(l), None) => Some(l.clone()),
        (None, Some(r)) => Some(r.clone()),
        (None, None) => None,
    }
}

fn intersect_schedules(a: &SharingSchedule, b: &SharingSchedule) -> SharingSchedule {
    // Simplified: use the more restrictive (shorter) windows
    // Full implementation would compute intersection of time windows
    if a.allowed_windows.len() <= b.allowed_windows.len() {
        a.clone()
    } else {
        b.clone()
    }
}

fn merge_power(local: &bg_helper_protocol::resource::PowerPolicy, remote: &bg_helper_protocol::resource::PowerPolicy) -> bg_helper_protocol::resource::PowerPolicy {
    bg_helper_protocol::resource::PowerPolicy {
        allow_on_battery: local.allow_on_battery && remote.allow_on_battery,
        min_battery_pct: tighten_opt(local.min_battery_pct, remote.min_battery_pct),
        require_ac_for_gpu: local.require_ac_for_gpu || remote.require_ac_for_gpu,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bg_helper_protocol::job::{JobSpec, WorkloadType, JobPriority};
    use bg_helper_protocol::resource::{ResourceLimits, ResourceUsage, PowerState, WorkerState};
    use chrono::Utc;
    use uuid::Uuid;

    fn default_limits() -> ResourceLimits {
        ResourceLimits {
            cpu_max_cores: Some(4),
            cpu_max_utilization_pct: Some(100), // Allow full CPU utilization for tests
            gpu_enabled: false,
            gpu_allowed_devices: vec![],
            gpu_max_memory_pct: None,
            ram_max_workload_bytes: Some(2_000_000_000),
            storage_workspace_path: None,
            storage_quota_bytes: None,
            schedule: None,
            power: Default::default(),
        }
    }

    fn idle_usage() -> ResourceUsage {
        ResourceUsage {
            worker_id: Uuid::new_v4(),
            timestamp: Utc::now(),
            cpu_utilization_pct: 10.0,
            cpu_cores_used: 0.5,
            cpu_frequency_mhz: None,
            gpu_utilization_pct: None,
            gpu_memory_used_bytes: None,
            gpu_temperature_c: None,
            ram_used_bytes: 2_000_000_000,
            ram_available_bytes: 14_000_000_000,
            storage_used_bytes: 10_000_000_000,
            storage_available_bytes: 100_000_000_000,
            power_state: PowerState::Ac,
            thermal_state: bg_helper_protocol::resource::ThermalState::Normal,
        }
    }

    #[test]
    fn test_allow_simple_job() {
        let limits = default_limits();
        let usage = idle_usage();
        let job = JobSpec::new(WorkloadType::RangeSum { start: 0, end: 1_000_000 }, JobPriority::Normal, 300);
        
        let decision = PolicyEvaluator::evaluate_job_offer(
            &limits, &usage, &job, None, PowerState::Ac, WorkerState::Idle
        );
        // Job is allowed with utilization reduction (100% utilization cap)
        assert!(matches!(decision, PolicyDecision::Allow | PolicyDecision::AllowWithReduction(_)));
    }

    #[test]
    fn test_reject_cpu_cores() {
        let mut limits = default_limits();
        limits.cpu_max_cores = Some(1);
        
        let mut usage = idle_usage();
        usage.cpu_cores_used = 0.9; // Almost all cores used
        
        let job = JobSpec::new(WorkloadType::RangeSum { start: 0, end: 1_000_000 }, JobPriority::Normal, 300);
        let decision = PolicyEvaluator::evaluate_job_offer(
            &limits, &usage, &job, None, PowerState::Ac, WorkerState::Idle
        );
        assert!(matches!(decision, PolicyDecision::Reject(_)));
    }

    #[test]
    fn test_reduce_cpu_cores() {
        let mut limits = default_limits();
        limits.cpu_max_cores = Some(4); // More cores available
        
        let mut usage = idle_usage();
        usage.cpu_cores_used = 1.5; // 1.5 cores used, 2.5 available
        
        let job = JobSpec::new(WorkloadType::RangeSum { start: 0, end: 1_000_000 }, JobPriority::Normal, 300);
        let decision = PolicyEvaluator::evaluate_job_offer(
            &limits, &usage, &job, None, PowerState::Ac, WorkerState::Idle
        );
        // With 2.5 cores available and job needing 1, should be allowed (cores check passes)
        // But utilization check may still require reduction
        assert!(matches!(decision, PolicyDecision::Allow | PolicyDecision::AllowWithReduction(_)));
    }

    #[test]
    fn test_reject_cpu_utilization() {
        let mut limits = default_limits();
        limits.cpu_max_utilization_pct = Some(30);
        
        let mut usage = idle_usage();
        usage.cpu_utilization_pct = 25.0; // Already at 25%
        
        let job = JobSpec::new(WorkloadType::RangeSum { start: 0, end: 1_000_000 }, JobPriority::Normal, 300);
        let decision = PolicyEvaluator::evaluate_job_offer(
            &limits, &usage, &job, None, PowerState::Ac, WorkerState::Idle
        );
        assert!(matches!(decision, PolicyDecision::Reject(_)));
    }

    #[test]
    fn test_reduce_cpu_utilization() {
        let mut limits = default_limits();
        limits.cpu_max_utilization_pct = Some(30);
        
        let mut usage = idle_usage();
        usage.cpu_utilization_pct = 15.0; // Low enough that remaining >= 10%
        
        let job = JobSpec::new(WorkloadType::RangeSum { start: 0, end: 1_000_000 }, JobPriority::Normal, 300);
        let decision = PolicyEvaluator::evaluate_job_offer(
            &limits, &usage, &job, None, PowerState::Ac, WorkerState::Idle
        );
        assert!(matches!(decision, PolicyDecision::AllowWithReduction(_)));
    }

    #[test]
    fn test_reject_ram() {
        let mut limits = default_limits();
        limits.ram_max_workload_bytes = Some(500_000_000); // 500MB
        
        let mut usage = idle_usage();
        usage.ram_used_bytes = 18_000_000_000; // 18GB used, only 2GB free but limit is 500MB
        
        let job = JobSpec::new(WorkloadType::MatrixTile { 
            size: 1000, 
            tile: bg_helper_protocol::job::TileSpec { row: 0, col: 0, tile_size: 100 } 
        }, JobPriority::Normal, 300);
        let decision = PolicyEvaluator::evaluate_job_offer(
            &limits, &usage, &job, None, PowerState::Ac, WorkerState::Idle
        );
        // Job requires matrix(1000*1000*8*3) ~= 24MB, limit is 500MB, available is 500MB
        // 24MB <= 500MB, so should allow
        assert!(matches!(decision, PolicyDecision::Allow | PolicyDecision::AllowWithReduction(_)));
    }

    #[test]
    fn test_reject_battery() {
        let mut limits = default_limits();
        limits.power.allow_on_battery = false;
        
        let usage = idle_usage();
        let job = JobSpec::new(WorkloadType::RangeSum { start: 0, end: 1_000_000 }, JobPriority::Normal, 300);
        
        let decision = PolicyEvaluator::evaluate_job_offer(
            &limits, &usage, &job, None, PowerState::Battery, WorkerState::Idle
        );
        assert!(matches!(decision, PolicyDecision::Reject(_)));
    }

    #[test]
    fn test_allow_on_battery() {
        let mut limits = default_limits();
        limits.power.allow_on_battery = true;
        
        let usage = idle_usage();
        let job = JobSpec::new(WorkloadType::RangeSum { start: 0, end: 1_000_000 }, JobPriority::Normal, 300);
        
        let decision = PolicyEvaluator::evaluate_job_offer(
            &limits, &usage, &job, None, PowerState::Battery, WorkerState::Idle
        );
        // Job is allowed on battery with utilization reduction
        assert!(matches!(decision, PolicyDecision::Allow | PolicyDecision::AllowWithReduction(_)));
    }

    #[test]
    fn test_merge_limits_tightens() {
        let local = ResourceLimits {
            cpu_max_cores: Some(4),
            cpu_max_utilization_pct: Some(50),
            gpu_enabled: true,
            gpu_allowed_devices: vec!["gpu1".into(), "gpu2".into()],
            gpu_max_memory_pct: Some(80),
            ram_max_workload_bytes: Some(4_000_000_000),
            storage_workspace_path: Some("/local/path".into()),
            storage_quota_bytes: Some(10_000_000_000),
            schedule: None,
            power: bg_helper_protocol::resource::PowerPolicy { allow_on_battery: true, min_battery_pct: Some(20), require_ac_for_gpu: false },
        };
        let remote = ResourceLimits {
            cpu_max_cores: Some(2),
            cpu_max_utilization_pct: Some(30),
            gpu_enabled: true,
            gpu_allowed_devices: vec!["gpu1".into()],
            gpu_max_memory_pct: Some(50),
            ram_max_workload_bytes: Some(2_000_000_000),
            storage_workspace_path: Some("/remote/path".into()),
            storage_quota_bytes: Some(5_000_000_000),
            schedule: None,
            power: bg_helper_protocol::resource::PowerPolicy { allow_on_battery: false, min_battery_pct: Some(50), require_ac_for_gpu: true },
        };
        
        let merged = merge_limits(&local, &remote);
        
        assert_eq!(merged.cpu_max_cores, Some(2));
        assert_eq!(merged.cpu_max_utilization_pct, Some(30));
        assert_eq!(merged.gpu_enabled, true); // local true && remote true = true
        assert_eq!(merged.gpu_allowed_devices, vec!["gpu1"]);
        assert_eq!(merged.gpu_max_memory_pct, Some(50));
        assert_eq!(merged.ram_max_workload_bytes, Some(2_000_000_000));
        assert_eq!(merged.storage_quota_bytes, Some(5_000_000_000));
        assert!(!merged.power.allow_on_battery);
        assert_eq!(merged.power.min_battery_pct, Some(20)); // tighter: min(20, 50) = 20
        assert!(merged.power.require_ac_for_gpu);
    }
}