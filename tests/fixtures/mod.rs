//! Test fixtures for synthetic hardware and job data.

use bg_helper_protocol::{
    job::{JobSpec, WorkloadType},
    resource::{HardwareCapability, ResourceKind, ResourceLimits, ResourceUsage, WorkerState, PowerState},
};
use chrono::Utc;
use uuid::Uuid;

/// Creates a synthetic worker with specified capabilities.
pub fn synthetic_worker(
    id: Uuid,
    cpu_cores: u32,
    ram_gb: u64,
    state: WorkerState,
) -> bg_helper_protocol::resource::WorkerStatus {
    let caps = vec![
        HardwareCapability {
            kind: ResourceKind::Cpu,
            name: "Test CPU".into(),
            total_capacity: cpu_cores as u64,
            attributes: [
                ("cores".to_string(), cpu_cores.to_string()),
                ("threads".to_string(), (cpu_cores * 2).to_string()),
                ("frequency_mhz".to_string(), "3000".to_string()),
            ].into(),
        },
        HardwareCapability {
            kind: ResourceKind::Ram,
            name: "Test RAM".into(),
            total_capacity: ram_gb * 1_000_000_000,
            attributes: [
                ("total_bytes".to_string(), (ram_gb * 1_000_000_000).to_string()),
            ].into(),
        },
    ];
    
    bg_helper_protocol::resource::WorkerStatus {
        worker_id: id,
        hostname: format!("worker-{}", id.simple()),
        os: "linux".into(),
        arch: "x86_64".into(),
        capabilities: caps,
        limits: ResourceLimits {
            cpu_max_cores: Some(cpu_cores),
            cpu_max_utilization_pct: Some(50),
            gpu_enabled: false,
            gpu_allowed_devices: vec![],
            gpu_max_memory_pct: None,
            ram_max_workload_bytes: Some((ram_gb / 2) * 1_000_000_000),
            storage_workspace_path: Some("/tmp/bg-helper".into()),
            storage_quota_bytes: Some(5_000_000_000),
            schedule: None,
            power: Default::default(),
        },
        current_offer: None,
        current_usage: Some(ResourceUsage {
            worker_id: id,
            timestamp: Utc::now(),
            cpu_utilization_pct: 10.0,
            cpu_cores_used: 0.5,
            gpu_utilization_pct: None,
            gpu_memory_used_bytes: None,
            ram_used_bytes: 1_000_000_000,
            ram_available_bytes: (ram_gb - 1) * 1_000_000_000,
            storage_used_bytes: 0,
            storage_available_bytes: 100_000_000_000,
            power_state: PowerState::Ac,
        }),
        current_job: None,
        state,
        last_seen: Utc::now(),
        enrolled_at: Utc::now(),
    }
}

/// Creates a synthetic job spec.
pub fn synthetic_job(workload: WorkloadType, priority: u8) -> JobSpec {
    JobSpec::new(workload, priority, 3600)
}

/// Creates a set of diverse synthetic workers.
pub fn synthetic_cluster() -> Vec<bg_helper_protocol::resource::WorkerStatus> {
    vec![
        synthetic_worker(Uuid::new_v4(), 8, 32, WorkerState::Idle),
        synthetic_worker(Uuid::new_v4(), 16, 64, WorkerState::Idle),
        synthetic_worker(Uuid::new_v4(), 4, 16, WorkerState::Busy),
        synthetic_worker(Uuid::new_v4(), 32, 128, WorkerState::Online),
    ]
}

/// Creates a synthetic job queue.
pub fn synthetic_jobs() -> Vec<JobSpec> {
    vec![
        synthetic_job(WorkloadType::RangeSum { start: 0, end: 1_000_000 }, 100),
        synthetic_job(WorkloadType::MatrixTile { size: 64, tile: 16 }, 150),
        synthetic_job(WorkloadType::PrimeCount { start: 2, end: 100_000 }, 50),
        synthetic_job(WorkloadType::RangeSum { start: 1_000_000, end: 2_000_000 }, 75),
    ]
}